; Downward closures — fns capture their defining environment.

(def make-adder
  (fn [n]
    (fn [x] (+ x n))))

(def add5 (make-adder 5))
(def add10 (make-adder 10))

(assert= (add5 3) 8)
(assert= (add10 3) 13)
(assert= (add5 (add10 1)) 16)

; Each call to make-adder produces an independent closure.
(def add1a (make-adder 1))
(def add1b (make-adder 1))
(assert= (add1a 100) 101)
(assert= (add1b 100) 101)

; Closure over let-bound names.
(def counter-example
  (let [base 100]
    (fn [delta] (+ base delta))))
(assert= (counter-example 5) 105)
(assert= (counter-example -50) 50)

; Higher-order functions.
(def apply-twice (fn [f x] (f (f x))))
(assert= (apply-twice (fn [n] (* n 2)) 3) 12)
(assert= (apply-twice (make-adder 1) 0) 2)
