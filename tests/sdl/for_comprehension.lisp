; (for [bindings...] body): list comprehension, desugared into nested
; mapcat / map over fns. The malformed-form errors are covered by the
; desugar_rejects_malformed_forms Rust test.

; One binding is a map.
(assert= (for [x [1 2 3]] (* x 10)) [10 20 30])
(assert= (for [x []] x) [])

; Several bindings nest, the last varying fastest.
(assert= (for [x [1 2] y [:a :b]] [x y]) [[1 :a] [1 :b] [2 :a] [2 :b]])
(assert= (count (for [a (range 3) b (range 4) c (range 5)] 0)) 60)

; Later collections can depend on earlier bindings.
(assert= (for [x (range 4) y (range x)] [x y]) [[1 0] [2 0] [2 1] [3 0] [3 1] [3 2]])

; Patterns destructure like fn parameters.
(assert= (for [[a b] [[1 2] [3 4]]] (+ a b)) [3 7])
(assert= (for [[a [b c]] [[1 [2 3]]]] [c b a]) [[3 2 1]])

; :when filters, :let binds.
(assert= (for [x (range 6) :when (= 0 (mod x 2))] x) [0 2 4])
(assert= (for [x (range 3) y (range 3) :when (not= x y)] [x y]) [[0 1] [0 2] [1 0] [1 2] [2 0] [2 1]])
(assert= (for [x [1 2 3] :let [sq (* x x)]] sq) [1 4 9])
(assert= (for [x (range 4) :let [y (* 2 x)] :when (> y 2)] y) [4 6])
(assert= (for [x [1 2] :when false y [3 4]] [x y]) [])

; The body can be any expression, including another for, and for
; composes with the rest of the language.
(assert= (for [x [1 2]] (for [y [3 4]] (* x y))) [[3 4] [6 8]])
(assert= (->> (range 5) (for [x [10]]) count) 1)

; Closures see the bindings.
(def fs (for [x [1 2 3]] (fn [] x)))
(assert= (map (fn [f] (f)) fs) [1 2 3])

; Quoted for is data.
(assert= (first '(for [x xs] x)) 'for)

; Large comprehensions are linear time (with (reduce conj ...) this
; would take seconds).
(assert= (count (for [i (range 100) j (range 200)] i)) 20000)
