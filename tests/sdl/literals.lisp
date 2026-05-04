; Literal values evaluate to themselves.

(assert= 0 0)
(assert= 42 42)
(assert= -7 -7)

(assert= 0.0 0.0)
(assert= 3.14 3.14)
(assert= -2.5 -2.5)

(assert= true true)
(assert= false false)
(assert= nil nil)

(assert= "hello" "hello")
(assert= "" "")

; Keywords compare by name.
(assert= :foo :foo)
(assert (not= :foo :bar))

; Vectors are equal if their elements are equal pairwise.
(assert= [] [])
(assert= [1 2 3] [1 2 3])
(assert (not= [1 2] [1 2 3]))

; Maps are equal if their key/value sets match.
(assert= {} {})
(assert= {:a 1 :b 2} {:b 2 :a 1})
(assert (not= {:a 1} {:a 2}))

; Numbers compare across int/float (Clojure-style cross-type =).
(assert= 1 1.0)
(assert= 0 0.0)
