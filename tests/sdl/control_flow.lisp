; if and do.

(assert= (if true 1 2) 1)
(assert= (if false 1 2) 2)
(assert= (if nil 1 2) 2)
(assert= (if 0 1 2) 1)        ; 0 is truthy

; if without else returns nil on false.
(assert= (if false 1) nil)
(assert= (if true 1) 1)

; do evaluates each form in order, returns the last.
(assert= (do 1 2 3) 3)
(assert= (do) nil)

; do is useful for side-effecting sequences in test setup; the
; intermediate values are discarded.
(assert= (do (+ 1 2) (+ 3 4)) 7)

; if branches are evaluated lazily.
(assert= (if true :ok (/ 1 0)) :ok)
(assert= (if false (/ 1 0) :ok) :ok)
