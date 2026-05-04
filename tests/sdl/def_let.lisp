; def and let bindings; lexical scoping.

(def x 10)
(assert= x 10)

; def can re-bind in the same env.
(def x 20)
(assert= x 20)

; let creates a new scope.
(assert= (let [a 1 b 2] (+ a b)) 3)

; Sequential bindings: later binders see earlier ones.
(assert= (let [a 1
               b (+ a 1)
               c (+ a b)] [a b c]) [1 2 3])

; Inner let shadows outer.
(def y 100)
(assert= (let [y 1] y) 1)
(assert= y 100)  ; outer y unchanged

; let body is implicit do.
(assert= (let [a 1]
           (+ a 1)
           (+ a 2)
           (+ a 3)) 4)

; def returns nil; verify side effect by reading the binding back.
(def k 99)
(assert= k 99)
