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

; light-area: disk area light. Phase 4 of the light-types plan.
; Positional args are (location axis radius color intensity).
(def la (light-area [0 0 5] [0 0 -1] 1.0 [1 1 1] 2.0))
(assert (light? la))

; Structural equality: two area lights built the same way are equal.
(def la2 (light-area [0 0 5] [0 0 -1] 1.0 [1 1 1] 2.0))
(assert= la la2)

; The binding normalizes a non-unit axis at the boundary, so two
; area lights built with parallel axis vectors of different
; magnitudes are structurally equal.
(def la-non-unit (light-area [0 0 5] [0 0 -2] 1.0 [1 1 1] 2.0))
(assert= la la-non-unit)

; Field-level inequality: cover axis, radius, intensity separately
; so a regression in any one of them fails this test distinctly.
(def la-diff-axis (light-area [0 0 5] [1 0 0] 1.0 [1 1 1] 2.0))
(assert (not= la la-diff-axis))

(def la-diff-radius (light-area [0 0 5] [0 0 -1] 0.5 [1 1 1] 2.0))
(assert (not= la la-diff-radius))

(def la-diff-intensity (light-area [0 0 5] [0 0 -1] 1.0 [1 1 1] 1.0))
(assert (not= la la-diff-intensity))

; Area lights are not equal to point or spotlights at the same
; location — the `kind` field differs across variants.
(assert (not= la (light-white [0 0 5])))
(assert (not= la (light-spot [0 0 5] [0 0 -1] [1 1 1] 2.0 (/ pi 8) (/ pi 5))))

; Negative checks.
(assert (not (light? nil)))
(assert (not (light? [1 2 3])))

;; --------------------------------------------------------------------
;; (light {...}): the general, map-keyed constructor.
;; --------------------------------------------------------------------

; Where it overlaps the positional constructors, it builds the same
; light.
(assert= (light {:location [1 2 3]}) (light-white [1 2 3]))
(assert= (light {:location [1 2 3] :color [0.5 0.2 0.1] :intensity 2})
         (light-point [1 2 3] [0.5 0.2 0.1] 2))
(assert= (light {:location [0 0 5] :direction [0 0 -1] :inner-angle 0.2 :outer-angle 0.4})
         (light-spot [0 0 5] [0 0 -1] [1 1 1] 1 0.2 0.4))
(assert= (light {:location [0 0 5] :radius 0.5 :axis [0 0 -2]})
         (light-area [0 0 5] [0 0 -1] 0.5 [1 1 1] 1))

; :point-at aims a cone at a point; it's the same as the direction to it.
(assert= (light {:location [0 0 5] :point-at [0 0 1] :inner-angle 0.2 :outer-angle 0.4})
         (light {:location [0 0 5] :direction [0 0 -1] :inner-angle 0.2 :outer-angle 0.4}))

; Shadowless is its own light.
(assert (light? (light {:location [0 10 0] :shadowless true})))
(assert (not= (light {:location [0 10 0] :shadowless true}) (light-white [0 10 0])))
(assert= (light {:location [0 10 0] :shadowless false}) (light-white [0 10 0]))

; An area light that is also a spotlight: a disk (whose axis defaults to
; the cone's direction) or a parallelogram.
(def spot-disk (light {:location [0 5 0] :point-at [0 0 0] :inner-angle 0.3 :outer-angle 0.6 :radius 1}))
(assert (light? spot-disk))
(assert= spot-disk (light {:location [0 5 0] :point-at [0 0 0] :inner-angle 0.3 :outer-angle 0.6
                           :radius 1 :axis [0 -1 0]}))
(assert (not= spot-disk (light-area [0 5 0] [0 -1 0] 1 [1 1 1] 1)))
(def spot-quad (light {:location [30 35 30] :point-at [0 5 0] :inner-angle 0.35 :outer-angle 0.79
                       :area-u [6 0 0] :area-v [0 6 0] :intensity 1.5}))
(assert (light? spot-quad))
(assert (not= spot-quad (light {:location [30 35 30] :area-u [6 0 0] :area-v [0 6 0] :intensity 1.5})))

; New lights work anywhere a light does: transformed, in groups, in a
; scene that renders.
(assert (shape? (translate [1 0 0] spot-quad)))
(def s (scene {:name "light-constructor"
               :camera (camera-looking-at [0 -5 2] [0 0 0] [0 0 1] 1.0)
               :background [0 0 0]
               :objects [(light {:location [0 0 10] :color [0.3 0.3 0.3] :shadowless true})
                         (light {:location [3 -3 5] :point-at [0 0 0] :inner-angle 0.3 :outer-angle 0.6
                                 :area-u [1 0 0] :area-v [0 1 0]})
                         (sphere {:center [0 0 0] :r 1 :surface (surface {:color [1 0 0]})})]
               :min-samples 1
               :max-samples 1}))
(def t (png-target 8 8))
(assert= (render s t 8 8) t)

;; --------------------------------------------------------------------
;; epsilon: the renderer's self-intersection tolerance.
;; --------------------------------------------------------------------

(assert (float? epsilon))
(assert= epsilon 0.0001)
