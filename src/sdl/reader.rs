// Copyright (c) Mike Schaeffer. All rights reserved.
//
// The use and distribution terms for this software are covered by the
// Eclipse Public License 2.0 (https://opensource.org/licenses/EPL-2.0)
// which can be found in the file LICENSE at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
//
// You must not remove this notice, or any other, from this software.

//! Tokenizer + s-expression reader.
//!
//! The reader is character-oriented: there's no separate token stream
//! exposed externally. The character cursor tracks line/column so
//! every `Form` we emit carries its source position.
//!
//! What's recognized:
//! - Whitespace: spaces, tabs, newlines, carriage returns. **Commas
//!   are also whitespace** (Clojure convention).
//! - Line comments: `;` to end of line.
//! - Lists: `( ... )`.
//! - Vectors: `[ ... ]`.
//! - Maps: `{ ... }` with an even number of forms.
//! - Strings: `"..."` with `\n`, `\t`, `\r`, `\\`, `\"` escapes.
//! - Numbers: optional `-`, decimal digits, optional `.<digits>` for
//!   floats. No `e` exponent notation in Phase 1 (easy to add later).
//! - Keywords: `:foo` — stored without the leading `:`.
//! - Symbols: anything else that isn't whitespace or a delimiter.
//! - `nil`, `true`, `false`: read as their respective literal forms.
//! - Reader macro `'x` → `(quote x)`.

use std::rc::Rc;

use crate::sdl::ast::{Form, FormKind};
use crate::sdl::error::Position;
use crate::sdl_read_panic;

/// Read every top-level form in `source`. `filename` is used only for
/// error messages.
pub fn read_all(source: &str, filename: &str) -> Vec<Form> {
    let mut r = Reader::new(source, filename);
    let mut forms = Vec::new();
    r.skip_ws_and_comments();
    while !r.at_end() {
        forms.push(r.read_form());
        r.skip_ws_and_comments();
    }
    forms
}

/// Read exactly one form. Used by the CLI's REPL-ish path and tests
/// that want a single form parsed.
pub fn read_one(source: &str, filename: &str) -> Form {
    let mut r = Reader::new(source, filename);
    r.skip_ws_and_comments();
    if r.at_end() {
        sdl_read_panic!(r.pos(), "expected a form, got end of input");
    }
    let form = r.read_form();
    r.skip_ws_and_comments();
    if !r.at_end() {
        sdl_read_panic!(r.pos(), "unexpected trailing input");
    }
    form
}

struct Reader<'a> {
    src: &'a [u8],
    /// Byte offset into `src`.
    idx: usize,
    /// 1-indexed.
    line: usize,
    /// 1-indexed.
    col: usize,
    file: Rc<String>,
}

impl<'a> Reader<'a> {
    fn new(source: &'a str, filename: &str) -> Self {
        Reader {
            src: source.as_bytes(),
            idx: 0,
            line: 1,
            col: 1,
            file: Rc::new(filename.to_string()),
        }
    }

    fn at_end(&self) -> bool {
        self.idx >= self.src.len()
    }

    fn pos(&self) -> Position {
        Position::new(self.file.clone(), self.line, self.col)
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.idx).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.src.get(self.idx + offset).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.idx += 1;
        if b == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(b)
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            match self.peek() {
                None => return,
                Some(b) if is_ws(b) => {
                    self.advance();
                }
                Some(b';') => {
                    // Comment to end of line.
                    while let Some(b) = self.peek() {
                        if b == b'\n' {
                            break;
                        }
                        self.advance();
                    }
                }
                _ => return,
            }
        }
    }

    /// Dispatch to the right reader based on the next non-whitespace
    /// character. Caller must ensure whitespace is already skipped.
    fn read_form(&mut self) -> Form {
        let pos = self.pos();
        let b = match self.peek() {
            Some(b) => b,
            None => sdl_read_panic!(pos, "expected a form, got end of input"),
        };

        match b {
            b'(' => self.read_list(),
            b'[' => self.read_vector(),
            b'{' => self.read_map(),
            b')' | b']' | b'}' => {
                sdl_read_panic!(pos, "unexpected '{}'", b as char)
            }
            b'"' => self.read_string(),
            b':' => self.read_keyword(),
            b'\'' => {
                // 'x → (quote x)
                self.advance();
                self.skip_ws_and_comments();
                let inner = self.read_form();
                let quote_sym = Form::new(FormKind::Symbol("quote".to_string()), pos.clone());
                Form::new(FormKind::List(vec![quote_sym, inner]), pos)
            }
            b'-' => {
                // Could be a negative number, or a symbol like `-` or
                // `->>`. Look at the next char: digit ⇒ number,
                // otherwise symbol.
                match self.peek_at(1) {
                    Some(c) if c.is_ascii_digit() => self.read_number(),
                    _ => self.read_symbol_or_literal(),
                }
            }
            b if b.is_ascii_digit() => self.read_number(),
            _ => self.read_symbol_or_literal(),
        }
    }

    fn read_list(&mut self) -> Form {
        let pos = self.pos();
        self.advance(); // consume '('
        let mut items = Vec::new();
        loop {
            self.skip_ws_and_comments();
            match self.peek() {
                None => sdl_read_panic!(pos, "unterminated list"),
                Some(b')') => {
                    self.advance();
                    return Form::new(FormKind::List(items), pos);
                }
                Some(_) => items.push(self.read_form()),
            }
        }
    }

    fn read_vector(&mut self) -> Form {
        let pos = self.pos();
        self.advance(); // consume '['
        let mut items = Vec::new();
        loop {
            self.skip_ws_and_comments();
            match self.peek() {
                None => sdl_read_panic!(pos, "unterminated vector"),
                Some(b']') => {
                    self.advance();
                    return Form::new(FormKind::Vector(items), pos);
                }
                Some(_) => items.push(self.read_form()),
            }
        }
    }

    fn read_map(&mut self) -> Form {
        let pos = self.pos();
        self.advance(); // consume '{'
        let mut pairs: Vec<(Form, Form)> = Vec::new();
        loop {
            self.skip_ws_and_comments();
            match self.peek() {
                None => sdl_read_panic!(pos, "unterminated map"),
                Some(b'}') => {
                    self.advance();
                    return Form::new(FormKind::Map(pairs), pos);
                }
                Some(_) => {
                    let key = self.read_form();
                    self.skip_ws_and_comments();
                    if self.at_end() || self.peek() == Some(b'}') {
                        sdl_read_panic!(
                            key.pos.clone(),
                            "map literal has odd number of forms (key without value)"
                        );
                    }
                    let val = self.read_form();
                    pairs.push((key, val));
                }
            }
        }
    }

    fn read_string(&mut self) -> Form {
        let pos = self.pos();
        self.advance(); // consume opening "
        let mut s = String::new();
        loop {
            match self.peek() {
                None => sdl_read_panic!(pos, "unterminated string literal"),
                Some(b'"') => {
                    self.advance();
                    return Form::new(FormKind::String(s), pos);
                }
                Some(b'\\') => {
                    self.advance();
                    match self.peek() {
                        Some(b'n') => {
                            s.push('\n');
                            self.advance();
                        }
                        Some(b't') => {
                            s.push('\t');
                            self.advance();
                        }
                        Some(b'r') => {
                            s.push('\r');
                            self.advance();
                        }
                        Some(b'"') => {
                            s.push('"');
                            self.advance();
                        }
                        Some(b'\\') => {
                            s.push('\\');
                            self.advance();
                        }
                        Some(b) => sdl_read_panic!(
                            self.pos(),
                            "unknown string escape '\\{}'",
                            b as char
                        ),
                        None => sdl_read_panic!(pos, "unterminated string literal"),
                    }
                }
                Some(_) => {
                    // Read the next codepoint (may be multibyte UTF-8).
                    self.read_utf8_into(&mut s);
                }
            }
        }
    }

    /// Read one UTF-8 codepoint at `idx` into `out`, advancing past
    /// it. Assumes `peek()` already returned `Some(_)`.
    fn read_utf8_into(&mut self, out: &mut String) {
        let start = self.idx;
        let b = self.src[start];
        let len = if b < 0x80 {
            1
        } else if b & 0xE0 == 0xC0 {
            2
        } else if b & 0xF0 == 0xE0 {
            3
        } else if b & 0xF8 == 0xF0 {
            4
        } else {
            // Invalid UTF-8 lead byte. Treat as 1-byte to stay
            // resilient; this is not expected in well-formed input.
            1
        };
        let end = (start + len).min(self.src.len());
        // Advance the cursor character-by-character so line/col stay
        // accurate.
        for _ in 0..(end - start) {
            self.advance();
        }
        out.push_str(std::str::from_utf8(&self.src[start..end]).unwrap_or("\u{FFFD}"));
    }

    fn read_keyword(&mut self) -> Form {
        let pos = self.pos();
        self.advance(); // consume ':'
        let name = self.read_atom_chars();
        if name.is_empty() {
            sdl_read_panic!(pos, "empty keyword");
        }
        Form::new(FormKind::Keyword(name), pos)
    }

    fn read_number(&mut self) -> Form {
        let pos = self.pos();
        let mut s = String::new();
        if self.peek() == Some(b'-') {
            s.push('-');
            self.advance();
        }
        let mut is_float = false;
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() {
                s.push(b as char);
                self.advance();
            } else if b == b'.' && !is_float {
                // Lookahead: only consume `.` if followed by a digit.
                // Otherwise this is the start of something else and
                // we stop. (Symbols can contain `.` but Phase 1
                // numbers don't permit a trailing `.`.)
                if let Some(next) = self.peek_at(1) {
                    if next.is_ascii_digit() {
                        is_float = true;
                        s.push('.');
                        self.advance();
                        continue;
                    }
                }
                break;
            } else {
                break;
            }
        }
        if is_float {
            match s.parse::<f64>() {
                Ok(f) => Form::new(FormKind::Float(f), pos),
                Err(e) => sdl_read_panic!(pos, "invalid float literal '{}': {}", s, e),
            }
        } else {
            match s.parse::<i64>() {
                Ok(i) => Form::new(FormKind::Int(i), pos),
                Err(e) => sdl_read_panic!(pos, "invalid integer literal '{}': {}", s, e),
            }
        }
    }

    /// Read a bare atom — a symbol, or one of the literal-named
    /// forms `nil`/`true`/`false`.
    fn read_symbol_or_literal(&mut self) -> Form {
        let pos = self.pos();
        let s = self.read_atom_chars();
        if s.is_empty() {
            sdl_read_panic!(pos, "expected a form");
        }
        match s.as_str() {
            "nil" => Form::new(FormKind::Nil, pos),
            "true" => Form::new(FormKind::Bool(true), pos),
            "false" => Form::new(FormKind::Bool(false), pos),
            _ => Form::new(FormKind::Symbol(s), pos),
        }
    }

    /// Greedy: read characters until whitespace or a delimiter.
    /// Includes `:` and `'` (but those are dispatched to other
    /// readers before this is called, so they only show up *inside*
    /// a token, e.g. `:as` after the leading-colon dispatch).
    fn read_atom_chars(&mut self) -> String {
        let mut s = String::new();
        while let Some(b) = self.peek() {
            if is_ws(b) || is_delim(b) {
                break;
            }
            // Multibyte-safe append.
            self.read_utf8_into(&mut s);
        }
        s
    }
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b',')
}

fn is_delim(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'"' | b';' | b'\''
    )
}
