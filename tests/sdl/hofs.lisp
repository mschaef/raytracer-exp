; Phase 4 — higher-order functions: map, filter, reduce, range,
; repeat, apply.

;; --------------------------------------------------------------------
;; map
;; --------------------------------------------------------------------

(def inc (fn [n] (+ n 1)))
(def dbl (fn [n] (* n 2)))

(assert= (map inc []) [])
(assert= (map inc [1 2 3]) [2 3 4])
(assert= (map dbl [1 2 3]) [2 4 6])

; map composes over arbitrary functions; the callable can be any
; Value::Fn (interpreted or native).
(assert= (map (fn [n] (* n n)) [1 2 3 4]) [1 4 9 16])

;; --------------------------------------------------------------------
;; filter
;; --------------------------------------------------------------------

(def positive? (fn [n] (> n 0)))
(def big?      (fn [n] (> n 3)))

(assert= (filter big?       [])              [])
(assert= (filter big?       [1 2 3 4 5 6])   [4 5 6])
(assert= (filter positive?  [-2 -1 0 1 2])   [1 2])
; Truthy-not-equal-to-true is enough; filter keeps anything truthy.
(assert= (filter (fn [x] x) [1 nil 2 false 3]) [1 2 3])

;; --------------------------------------------------------------------
;; reduce
;; --------------------------------------------------------------------

; 3-arity: (reduce f init coll). init is returned for empty coll.
(assert= (reduce + 0 [])         0)
(assert= (reduce + 0 [1 2 3 4])  10)
(assert= (reduce + 100 [1 2 3])  106)
(assert= (reduce * 1 [1 2 3 4])  24)

; 2-arity: (reduce f coll). First element is the seed.
(assert= (reduce + [1 2 3 4]) 10)
(assert= (reduce + [42])      42)         ; single-element shortcut
(assert= (reduce * [2 3 4])   24)

;; --------------------------------------------------------------------
;; range
;; --------------------------------------------------------------------

(assert= (range 0)         [])
(assert= (range 5)         [0 1 2 3 4])
(assert= (range 2 5)       [2 3 4])
(assert= (range 2 2)       [])           ; empty when start = end
(assert= (range 0 10 2)    [0 2 4 6 8])
(assert= (range 5 0 -1)    [5 4 3 2 1])  ; descending
(assert= (range 5 5 -1)    [])

;; --------------------------------------------------------------------
;; repeat
;; --------------------------------------------------------------------

(assert= (repeat 0 :x)  [])
(assert= (repeat 3 :x)  [:x :x :x])
(assert= (repeat 4 0)   [0 0 0 0])
(assert= (repeat 2 [1]) [[1] [1]])

;; --------------------------------------------------------------------
;; apply
;; --------------------------------------------------------------------

; Two-arg form: function + vector of args.
(assert= (apply + [1 2 3])     6)
(assert= (apply + [])          0)
(assert= (apply max [3 1 4 1 5 9 2 6]) 9)

; Variadic form: leading positional args + trailing vector.
(assert= (apply + 1 2 [3 4])   10)
(assert= (apply + 1 [])        1)

; Compose with map: building variadic argument lists from data.
(def args [10 20 30])
(assert= (apply + args) 60)

;; --------------------------------------------------------------------
;; HOFs interact correctly with closures (capturing via fn).
;; --------------------------------------------------------------------

(def add-n (fn [n] (fn [x] (+ x n))))
(assert= (map (add-n 10) [1 2 3]) [11 12 13])
(assert= (filter (fn [x] (> x 5)) (map (add-n 4) [1 2 3 4 5])) [6 7 8 9])
