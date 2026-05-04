; Vector construction and access.

; Constructors.
(assert= (vector) [])
(assert= (vector 1 2 3) [1 2 3])
(assert= (vec 1 2 3) [1 2 3])

; nth (0-indexed).
(assert= (nth [10 20 30] 0) 10)
(assert= (nth [10 20 30] 2) 30)

; count works on vec, map, string, nil.
(assert= (count []) 0)
(assert= (count [1 2 3 4]) 4)
(assert= (count {:a 1 :b 2}) 2)
(assert= (count "hello") 5)
(assert= (count nil) 0)

; first / rest.
(assert= (first [10 20 30]) 10)
(assert= (rest [10 20 30]) [20 30])
(assert= (first []) nil)
(assert= (rest []) [])
(assert= (first nil) nil)
(assert= (rest nil) [])

; conj appends to a vec.
(assert= (conj [] 1) [1])
(assert= (conj [1 2] 3) [1 2 3])
(assert= (conj [1] 2 3 4) [1 2 3 4])
(assert= (conj nil :first) [:first])

; conj does not mutate the original.
(def original [1 2 3])
(def extended (conj original 4))
(assert= original [1 2 3])
(assert= extended [1 2 3 4])

; Vectors of mixed types.
(assert= [1 :two "three" [4]] [1 :two "three" [4]])
