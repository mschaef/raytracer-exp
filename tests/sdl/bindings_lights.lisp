; Light bindings.

; light-white: full-intensity white point light at a location.
(def lw (light-white [5.0 5.0 5.0]))
(assert (light? lw))

; light-point: explicit color and intensity.
(def lr (light-point [-5 5 5] [1.0 0.2 0.2] 1.0))
(assert (light? lr))

; Two lights built the same way compare equal (structural eq through Rc).
(def lw2 (light-white [5.0 5.0 5.0]))
(assert= lw lw2)

(def lr2 (light-point [-5 5 5] [1.0 0.2 0.2] 1.0))
(assert= lr lr2)

; Different parameters → not equal.
(def lr3 (light-point [-5 5 5] [1.0 0.2 0.2] 0.5))
(assert (not= lr lr3))

; light-white at the same location as a light-point with white color
; and intensity 1.0 should be structurally equal.
(def lw-via-point (light-point [5 5 5] [1.0 1.0 1.0] 1.0))
(def lw-direct (light-white [5 5 5]))
(assert= lw-via-point lw-direct)

; Negative checks.
(assert (not (light? nil)))
(assert (not (light? [1 2 3])))
