; Comparison operators with chained semantics.

(assert (= 1 1))
(assert (= 1 1 1 1))
(assert (not= 1 2))
(assert (not= 1 1 2))

(assert (< 1 2))
(assert (< 1 2 3 4))
(assert (not (< 1 2 2)))

(assert (<= 1 1 2 3))
(assert (not (<= 2 1)))

(assert (> 3 2 1))
(assert (>= 3 3 2 1))

; Chained comparisons across int/float.
(assert (< 1 1.5 2))
(assert (= 1 1.0))

; Single-arg comparison is vacuously true.
(assert (< 5))
(assert (> 5))

; Equality on collections.
(assert= [1 2 3] [1 2 3])
(assert (not= [1 2 3] [3 2 1]))
(assert= {:a 1} {:a 1})
