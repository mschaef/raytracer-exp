; Map construction and access. Keys are restricted to keywords in
; Phase 1.

; Constructors: literal vs hash-map function.
(assert= {} (hash-map))
(assert= {:a 1 :b 2} (hash-map :a 1 :b 2))
(assert= (hash-map :x 10) {:x 10})

; get with explicit key.
(def m {:a 1 :b 2 :c 3})
(assert= (get m :a) 1)
(assert= (get m :c) 3)

; get with missing key returns nil by default.
(assert= (get m :missing) nil)

; get with default.
(assert= (get m :missing :fallback) :fallback)
(assert= (get m :a :fallback) 1)

; assoc returns a new map with the binding added/replaced.
(def m1 (assoc m :d 4))
(assert= (get m1 :d) 4)
(assert= (get m :d) nil)   ; original unchanged

(def m2 (assoc m :a 999))
(assert= (get m2 :a) 999)
(assert= (get m :a) 1)     ; original unchanged

; assoc with multiple key/value pairs.
(def m3 (assoc {} :a 1 :b 2 :c 3))
(assert= m3 {:a 1 :b 2 :c 3})

; dissoc removes keys.
(def m4 (dissoc m :a))
(assert= (get m4 :a) nil)
(assert= (get m4 :b) 2)
(assert= (count m4) 2)

(def m5 (dissoc m :a :b :c))
(assert= m5 {})

; dissoc of a missing key is a no-op.
(assert= (dissoc m :missing) m)

; keys and vals (sorted by key for stable ordering).
(assert= (keys {:b 2 :a 1 :c 3}) [:a :b :c])
(assert= (vals {:b 2 :a 1 :c 3}) [1 2 3])
(assert= (keys {}) [])
(assert= (vals {}) [])

; nil treated as an empty map for read ops.
(assert= (get nil :x) nil)
(assert= (get nil :x :default) :default)
(assert= (keys nil) [])
(assert= (vals nil) [])

; assoc onto nil produces a fresh map.
(assert= (assoc nil :a 1) {:a 1})
