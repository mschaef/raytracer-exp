; quote returns its argument unevaluated.

; A quoted symbol is a symbol value, not a lookup.
(def s (quote foo))
(assert (symbol? s))
(assert= (name s) "foo")

; The reader macro 'x is sugar for (quote x).
(assert= 'foo (quote foo))
(assert (symbol? 'bar))

; Quoting a list produces a vec of values (we don't have a separate
; list type at runtime).
(def xs '(1 2 3))
(assert (vector? xs))
(assert= xs [1 2 3])

; Symbols inside a quoted form are NOT looked up.
(def x 999)
(assert= '(x y z) [(quote x) (quote y) (quote z)])
(assert (not= '(x) [999]))

; Quoted vectors and maps work the same way.
(assert= '[1 2 3] [1 2 3])
(assert= '{:a 1 :b 2} {:a 1 :b 2})

; Quoted nested structures.
(assert= '[1 [2 [3]]] [1 [2 [3]]])
