; Type predicates.

(assert (nil? nil))
(assert (not (nil? false)))
(assert (not (nil? 0)))
(assert (not (nil? "")))

(assert (boolean? true))
(assert (boolean? false))
(assert (not (boolean? nil)))
(assert (not (boolean? 1)))

(assert (int? 0))
(assert (int? -42))
(assert (not (int? 1.0)))
(assert (not (int? "42")))

(assert (float? 0.0))
(assert (float? -3.14))
(assert (not (float? 1)))

(assert (number? 1))
(assert (number? 1.5))
(assert (not (number? "1")))
(assert (not (number? :one)))

(assert (string? ""))
(assert (string? "hello"))
(assert (not (string? :hello)))
(assert (not (string? 'hello)))

(assert (keyword? :foo))
(assert (not (keyword? "foo")))
(assert (not (keyword? 'foo)))

(assert (symbol? 'foo))
(assert (not (symbol? :foo)))
(assert (not (symbol? "foo")))

(assert (vector? []))
(assert (vector? [1 2 3]))
(assert (not (vector? {})))
(assert (not (vector? nil)))

(assert (map? {}))
(assert (map? {:a 1}))
(assert (not (map? [])))
(assert (not (map? nil)))

(assert (fn? (fn [x] x)))
(assert (fn? +))
(assert (fn? assoc))
(assert (not (fn? 42)))

; name extracts the printable string for keyword/symbol/string.
(assert= (name :foo) "foo")
(assert= (name 'bar) "bar")
(assert= (name "baz") "baz")
