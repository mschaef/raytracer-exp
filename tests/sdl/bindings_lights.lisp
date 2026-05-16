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

; light-spot: directed cone light. Phase 2 of the light-types plan.
; Positional args are (location direction color intensity inner outer).
(def ls (light-spot [0 0 5] [0 0 -1] [1 1 1] 2.0 (/ pi 8) (/ pi 5)))
(assert (light? ls))

; Two spotlights built the same way compare equal (structural eq
; through Rc-wrapped Light with derived PartialEq on LightKind::Spot).
(def ls2 (light-spot [0 0 5] [0 0 -1] [1 1 1] 2.0 (/ pi 8) (/ pi 5)))
(assert= ls ls2)

; The binding normalizes a non-unit direction at the boundary, so two
; spotlights built with parallel direction vectors of different
; magnitudes are structurally equal.
(def ls-non-unit
  (light-spot [0 0 5] [0 0 -2] [1 1 1] 2.0 (/ pi 8) (/ pi 5)))
(assert= ls ls-non-unit)

; Different parameters → not equal. Cover each spot-specific field
; (direction, inner-angle, outer-angle) plus a shared field
; (intensity) so a regression in any one of them fails this test.
(def ls-diff-dir
  (light-spot [0 0 5] [1 0  0] [1 1 1] 2.0 (/ pi 8) (/ pi 5)))
(assert (not= ls ls-diff-dir))

(def ls-diff-inner
  (light-spot [0 0 5] [0 0 -1] [1 1 1] 2.0 (/ pi 6) (/ pi 5)))
(assert (not= ls ls-diff-inner))

(def ls-diff-outer
  (light-spot [0 0 5] [0 0 -1] [1 1 1] 2.0 (/ pi 8) (/ pi 4)))
(assert (not= ls ls-diff-outer))

(def ls-diff-intensity
  (light-spot [0 0 5] [0 0 -1] [1 1 1] 1.0 (/ pi 8) (/ pi 5)))
(assert (not= ls ls-diff-intensity))

; Spotlights and point lights at the same location are not
; structurally equal — the `kind` field differs.
(assert (not= ls (light-white [0 0 5])))

; Negative checks.
(assert (not (light? nil)))
(assert (not (light? [1 2 3])))
