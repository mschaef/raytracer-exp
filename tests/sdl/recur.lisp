; recur: same-frame jump for iteration.

; Classic factorial via tail recursion.
(def fact
  (fn [n]
    ((fn [n acc]
       (if (<= n 1)
         acc
         (recur (- n 1) (* acc n))))
     n 1)))

(assert= (fact 0) 1)
(assert= (fact 1) 1)
(assert= (fact 5) 120)
(assert= (fact 10) 3628800)

; Fibonacci.
(def fib
  (fn [n]
    ((fn [a b k]
       (if (<= k 0)
         a
         (recur b (+ a b) (- k 1))))
     0 1 n)))

(assert= (fib 0) 0)
(assert= (fib 1) 1)
(assert= (fib 2) 1)
(assert= (fib 10) 55)

; recur with destructured params.
(def sum-pair-list
  (fn [pairs acc]
    (if (= (count pairs) 0)
      acc
      (let [[head & tail] pairs
            [a b] head]
        (recur tail (+ acc a b))))))

(assert= (sum-pair-list [[1 2] [3 4] [5 6]] 0) 21)

; Deep recursion that would blow the Rust stack without recur looping.
; 5000 iterations is well past any reasonable stack depth.
(def count-down
  (fn [n]
    (if (<= n 0)
      :done
      (recur (- n 1)))))

(assert= (count-down 5000) :done)
