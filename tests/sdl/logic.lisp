; Truthiness: only nil and false are falsy.

(assert (not nil))
(assert (not false))
(assert (not (not 0)))      ; 0 is truthy
(assert (not (not "")))     ; empty string is truthy
(assert (not (not [])))     ; empty vec is truthy

; `and` returns the first falsy value, or the last value if all truthy.
(assert= (and) true)
(assert= (and 1) 1)
(assert= (and 1 2 3) 3)
(assert= (and 1 nil 3) nil)
(assert= (and false 1) false)

; `or` returns the first truthy value, or the last value if all falsy.
(assert= (or) nil)
(assert= (or nil false 5) 5)
(assert= (or nil false) false)
(assert= (or :a :b) :a)

; Short-circuit: arguments after a definitive answer are not evaluated.
; If they were, `(/ 1 0)` would panic.
(assert= (and false (/ 1 0)) false)
(assert= (or :first (/ 1 0)) :first)

; not is a regular function.
(assert= (not true) false)
(assert= (not false) true)
(assert= (not nil) true)
(assert= (not 1) false)
