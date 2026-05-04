; Arithmetic with int/float auto-promotion.

(assert= (+) 0)
(assert= (*) 1)
(assert= (+ 1 2 3) 6)
(assert= (* 2 3 4) 24)
(assert= (- 10 1 2 3) 4)
(assert= (- 5) -5)

; Float promotion: any float makes the result float.
(assert= (+ 1 2.0) 3.0)
(assert= (* 2 0.5) 1.0)

; Single-arg division gives reciprocal as float.
(assert= (/ 4) 0.25)

; Multi-arg division always returns float (no rationals in Phase 1).
(assert= (/ 6 2) 3.0)
(assert= (/ 1 4) 0.25)

; Negation of zero stays zero.
(assert= (- 0) 0)
(assert= (- 0.0) -0.0)

; Mixed-type addition.
(assert= (+ 0.5 0.5) 1.0)
(assert= (+ 1 1 1.0) 3.0)
