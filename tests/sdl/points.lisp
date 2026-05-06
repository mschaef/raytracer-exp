; Phase 4 — point helpers.
;
; Defined in the lisp stdlib (`src/sdl/stdlib.lisp`) on top of the
; Phase 1 built-ins. Points are 3-element vectors, the same shape
; consumed by every host binding (Sphere::center, Camera::location, …).

;; --------------------------------------------------------------------
;; Constructor + accessors
;; --------------------------------------------------------------------

(assert= (point 1 2 3) [1 2 3])
(assert= (point 0 0 0) [0 0 0])

(def p (point 4 5 6))
(assert= (x p) 4)
(assert= (y p) 5)
(assert= (z p) 6)

; Accessors work on any 3-vector, not just things built via `point` —
; the host bindings happily accept any vector.
(assert= (x [10 20 30]) 10)
(assert= (z [10 20 30]) 30)

;; --------------------------------------------------------------------
;; Component-wise add and subtract
;; --------------------------------------------------------------------

(assert= (p+ [1 2 3] [4 5 6]) [5 7 9])
(assert= (p+ [0 0 0] [1 1 1]) [1 1 1])
(assert= (p- [4 5 6] [1 2 3]) [3 3 3])
(assert= (p- [1 2 3] [1 2 3]) [0 0 0])

; Mixing int and float operands promotes to float per the host
; arithmetic rules.
(assert= (p+ [1 2 3] [0.5 0.5 0.5]) [1.5 2.5 3.5])

;; --------------------------------------------------------------------
;; Scalar multiply
;; --------------------------------------------------------------------

(assert= (p* [1 2 3] 0)  [0 0 0])
(assert= (p* [1 2 3] 2)  [2 4 6])
(assert= (p* [1 2 3] -1) [-1 -2 -3])

;; --------------------------------------------------------------------
;; Composition: midpoint, scaling, etc.
;; --------------------------------------------------------------------

; (a + b) / 2  — midpoint.
(def midpoint (fn [a b] (p* (p+ a b) 0.5)))
(assert= (midpoint [0 0 0] [4 4 4]) [2.0 2.0 2.0])
(assert= (midpoint [-1 -1 -1] [1 1 1]) [0.0 0.0 0.0])
