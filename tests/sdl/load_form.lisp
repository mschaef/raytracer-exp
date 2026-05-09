; Phase 6 — (load <path>) special form.
;
; (load <expr>) evaluates <expr> to a string, reads the file at that
; path, and evaluates each top-level form in the *current*
; environment. Relative paths resolve against the directory of the
; loading file (handled by CurrentDirGuard in src/sdl/mod.rs); the
; loaded file's last form is returned, mirroring eval_source.
;
; Coverage:
; 1. defs in the loaded file are visible in the loading env
; 2. (load ...) returns the value of the last form in the loaded file
; 3. relative paths resolve against the current file's directory
;    (this script lives in tests/sdl/, so "load_form_fixture.lisp"
;    must resolve there too)
; 4. loading the same file twice is benign (def's overwrite, asserts
;    in the fixture pass on each reload)

;; --------------------------------------------------------------------
;; First load: capture the return value, verify the def's are visible.
;; --------------------------------------------------------------------

(def loaded-result (load "load_form_fixture.lisp"))

; Last form of the fixture is `fixture-x`, which is 42.
(assert= loaded-result 42)

; def's from the fixture are now in our env.
(assert= fixture-x 42)
(assert= fixture-y "hello")

;; --------------------------------------------------------------------
;; Second load: idempotence. The fixture's own asserts pass each
;; time, and the def's remain at their stored values.
;; --------------------------------------------------------------------

(load "load_form_fixture.lisp")
(assert= fixture-x 42)
(assert= fixture-y "hello")

;; --------------------------------------------------------------------
;; Loaded code can refer to bindings already present in the loading
;; env. (We can't easily test this in a self-contained way without
;; another fixture file, so we test the converse: bindings defined
;; before (load ...) are still visible after.)
;; --------------------------------------------------------------------

(def caller-binding 99)
(load "load_form_fixture.lisp")
(assert= caller-binding 99)
