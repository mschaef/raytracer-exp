; Surface bindings.

; Construction with full keys.
(def s1 (surface {:color [1.0 0.0 0.0]
                  :ambient 0.2
                  :specular 0.5
                  :light 0.6
                  :checked false
                  :reflection 0.0}))
(assert (surface? s1))

; Defaults: only :color is required.
(def s2 (surface {:color [0.5 0.5 0.5]}))
(assert (surface? s2))

; Two surfaces built with the same fields compare equal (assert= is
; structural for host types).
(def s3 (surface {:color [1.0 0.0 0.0]
                  :ambient 0.2
                  :specular 0.5
                  :light 0.6
                  :checked false
                  :reflection 0.0}))
(assert= s1 s3)

; Differing fields produce non-equal surfaces.
(def s4 (surface {:color [1.0 0.0 0.0]
                  :ambient 0.3
                  :specular 0.5
                  :light 0.6
                  :checked false
                  :reflection 0.0}))
(assert (not= s1 s4))

; Numbers are accepted as int or float and coerced.
(def s5 (surface {:color [1 0 0] :ambient 0 :light 1}))
(assert (surface? s5))

; Negative checks: surface? rejects non-surfaces.
(assert (not (surface? nil)))
(assert (not (surface? 42)))
(assert (not (surface? {:color [1 0 0]})))
