; Phase 4 — in-language standard library.
;
; Definitions in this file are loaded by `default_env` after the Rust
; built-ins and host bindings are installed, so they can use anything
; below. Conventionally these are the conveniences whose definitions
; are shorter and more idiomatic in lisp than in Rust.
;
; The compiler bakes this file into the binary via `include_str!`, so
; it's always loaded — every fresh interpreter starts with these
; definitions visible.

;; --------------------------------------------------------------------
;; Math constants
;; --------------------------------------------------------------------

; Tau (= 2π) is available alongside pi for callers who prefer the
; full-turn convention.
(def pi  3.141592653589793)
(def tau 6.283185307179586)

;; --------------------------------------------------------------------
;; Angle conversions
;; --------------------------------------------------------------------

(defn deg->rad [d] (/ (* d pi) 180.0))
(defn rad->deg [r] (/ (* r 180.0) pi))

;; --------------------------------------------------------------------
;; Point helpers
;; --------------------------------------------------------------------
;;
;; Points are 3-element vectors of numbers — the same shape every host
;; binding (Sphere::center, Camera::location, Light::location, …)
;; consumes. `point` is a thin constructor and `x`/`y`/`z` are
;; component accessors. The component-wise arithmetic helpers (`p+`,
;; `p-`, `p*`) ride on top of those — defining them here in lisp keeps
;; the Rust surface area small.

(defn point [x y z] [x y z])

(defn x [p] (nth p 0))
(defn y [p] (nth p 1))
(defn z [p] (nth p 2))

; Component-wise add and subtract. Both operands must be 3-vectors of
; numbers; the host's coercion rules apply (int + float → float, etc.).
(defn p+ [a b]
  [(+ (x a) (x b))
   (+ (y a) (y b))
   (+ (z a) (z b))])

(defn p- [a b]
  [(- (x a) (x b))
   (- (y a) (y b))
   (- (z a) (z b))])

; Scalar multiply: scales each component by `s`.
(defn p* [a s]
  [(* (x a) s)
   (* (y a) s)
   (* (z a) s)])
