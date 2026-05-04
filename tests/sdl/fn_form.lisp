; Anonymous and named functions.

; Anonymous fn called inline.
(assert= ((fn [x] (* x x)) 4) 16)
(assert= ((fn [a b] (+ a b)) 3 4) 7)
(assert= ((fn [] 42)) 42)

; Named fn (the name is mostly for debug output but bindable).
(def square (fn [x] (* x x)))
(assert= (square 5) 25)
(assert= (square 0) 0)

; Functions are first-class values.
(def my-fn square)
(assert= (my-fn 6) 36)
(assert (fn? square))
(assert (fn? my-fn))

; Multi-form body — implicit do.
(def add-and-double (fn [a b]
                      (def __ignored (+ a b))   ; first form is side-effect
                      (* 2 (+ a b))))
(assert= (add-and-double 3 4) 14)

; & rest captures remaining args as a vec.
(def variadic (fn [a & xs] [a xs]))
(assert= (variadic 1) [1 []])
(assert= (variadic 1 2 3 4) [1 [2 3 4]])
