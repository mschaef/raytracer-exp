; str: concatenation with type-specific formatting.

(assert= (str) "")
(assert= (str "hello") "hello")
(assert= (str "hello " "world") "hello world")

; Numbers, keywords, etc. get their Display form.
(assert= (str 42) "42")
(assert= (str 1.5) "1.5")
(assert= (str :foo) ":foo")
(assert= (str 'bar) "bar")

; nil contributes nothing (matches Clojure).
(assert= (str "a" nil "b") "ab")

; Mixed types.
(assert= (str "x=" 42 ", y=" 3.5) "x=42, y=3.5")

; String escapes round-trip through the reader.
(assert= "a\nb" (str "a" "
b"))

; assert= with a description string (positive case only — assertion
; passes, so the description is just metadata).
(assert= 1 1 "1 should equal 1")
(assert true "true is truthy")
