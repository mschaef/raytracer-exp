; Vector destructuring in let and fn.

; Basic positional destructuring.
(let [[a b c] [1 2 3]]
  (assert= a 1)
  (assert= b 2)
  (assert= c 3))

; Wildcard discards a slot.
(let [[_ b _] [10 20 30]]
  (assert= b 20))

; Missing positional slots bind to nil (Clojure-style lenient).
(let [[a b c] [1]]
  (assert= a 1)
  (assert= b nil)
  (assert= c nil))

; & rest captures the remainder as a vec.
(let [[a & xs] [1 2 3 4 5]]
  (assert= a 1)
  (assert= xs [2 3 4 5]))

(let [[a b & xs] [1 2]]
  (assert= a 1)
  (assert= b 2)
  (assert= xs []))

; :as binds the whole vector.
(let [[a b :as v] [10 20]]
  (assert= a 10)
  (assert= b 20)
  (assert= v [10 20]))

; & rest combined with :as.
(let [[head & tail :as all] [1 2 3 4]]
  (assert= head 1)
  (assert= tail [2 3 4])
  (assert= all [1 2 3 4]))

; Nested destructuring.
(let [[a [b c] d] [1 [2 3] 4]]
  (assert= a 1)
  (assert= b 2)
  (assert= c 3)
  (assert= d 4))

; Destructuring in fn parameter list.
(def sum-pair (fn [[a b]] (+ a b)))
(assert= (sum-pair [3 4]) 7)

; Nested destructuring in fn parameters.
(def origin-distance
  (fn [[[x1 y1] [x2 y2]]]
    (+ (* (- x2 x1) (- x2 x1))
       (* (- y2 y1) (- y2 y1)))))
(assert= (origin-distance [[0 0] [3 4]]) 25)

; & rest in fn params.
(def variadic (fn [a & rest] [a (count rest)]))
(assert= (variadic 1) [1 0])
(assert= (variadic 1 2 3 4) [1 3])

; Destructuring with rest in fn params.
(def first-and-rest (fn [[head & tail]] [head tail]))
(assert= (first-and-rest [1 2 3]) [1 [2 3]])
(assert= (first-and-rest [1]) [1 []])
