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

; :transparency key — Phase 1 transmission coefficient.
(def s-glass (surface {:color [0.6 0.8 1.0]
                       :ambient 0.2
                       :specular 0.5
                       :light 0.6
                       :checked false
                       :reflection 0.0
                       :transparency 0.7}))
(assert (surface? s-glass))

; :transparency defaults to 0.0 when omitted. s1 was built with every
; *other* key but no :transparency; an explicit :transparency 0.0
; surface with otherwise-identical fields must compare equal to it.
(def s-opaque-explicit (surface {:color [1.0 0.0 0.0]
                                 :ambient 0.2
                                 :specular 0.5
                                 :light 0.6
                                 :checked false
                                 :reflection 0.0
                                 :transparency 0.0}))
(assert= s1 s-opaque-explicit)

; A differing :transparency makes surfaces non-equal.
(assert (not= s1 s-glass))

; :metallic key — flags a metal surface.
(def s-metal (surface {:color [1.0 0.78 0.34]
                       :ambient 0.2
                       :specular 0.5
                       :light 0.6
                       :checked false
                       :reflection 0.7
                       :metallic true}))
(assert (surface? s-metal))

; :metallic defaults to false when omitted. s1 was built with every
; *other* key but no :metallic; an explicit :metallic false surface
; with otherwise-identical fields must compare equal to it.
(def s-nonmetal-explicit (surface {:color [1.0 0.0 0.0]
                                   :ambient 0.2
                                   :specular 0.5
                                   :light 0.6
                                   :checked false
                                   :reflection 0.0
                                   :transparency 0.0
                                   :metallic false}))
(assert= s1 s-nonmetal-explicit)

; A differing :metallic makes surfaces non-equal.
(def s1-metallic (surface {:color [1.0 0.0 0.0]
                           :ambient 0.2
                           :specular 0.5
                           :light 0.6
                           :checked false
                           :reflection 0.0
                           :metallic true}))
(assert (not= s1 s1-metallic))

; Negative checks: surface? rejects non-surfaces.
(assert (not (surface? nil)))
(assert (not (surface? 42)))
(assert (not (surface? {:color [1 0 0]})))

;; --------------------------------------------------------------------
;; :pigment — procedural colour.
;; --------------------------------------------------------------------

(def pine {:pattern    :wood
           :turbulence 0.05
           :color-map  [[0.0 [0.8 0.6 0.3]] [0.9 [0.6 0.35 0.05]] [1.0 [0.5 0.3 0.1]]]
           :transform  (affine-scale [0.05 0.05 0.05])})

; A pigmented surface doesn't need :color.
(def wood-s (surface {:pigment pine :ambient 0.1 :light 0.6}))
(assert (surface? wood-s))

; Structurally equal pigments make equal surfaces; any difference in the
; pigment makes them differ.
(assert= wood-s (surface {:pigment pine :ambient 0.1 :light 0.6}))
(assert (not= wood-s (surface {:pigment (assoc pine :turbulence 0.1) :ambient 0.1 :light 0.6})))
(assert (not= wood-s (surface {:pigment (assoc pine :wave :ramp) :ambient 0.1 :light 0.6})))

; Turbulence can be per axis; a number means the same on every axis.
(assert= wood-s (surface {:pigment (assoc pine :turbulence [0.05 0.05 0.05]) :ambient 0.1 :light 0.6}))
(assert (not= wood-s (surface {:pigment (assoc pine :turbulence [0.05 0.08 1000]) :ambient 0.1 :light 0.6})))
(assert (not= wood-s (surface {:color [0.5 0.5 0.5] :ambient 0.1 :light 0.6})))

; A checker takes two colours; every optional key is accepted.
(assert (surface? (surface {:pigment {:pattern :checker :colors [[1 1 1] [0 0 0]]}})))
(assert (surface? (surface {:pigment {:pattern :wood :color-map [[0 [1 0 0]] [1 [0 0 1]]]
                                      :turbulence 0.2 :octaves 3 :omega 0.6 :lambda 2.5
                                      :wave :sine}})))

; Pigmented surfaces work on any shape, through with-surface, and render.
(def s (scene {:name "pigment-bindings"
               :camera (camera-looking-at [0 -5 1] [0 0 0] [0 0 1] 1.0)
               :background [0 0 0]
               :objects [(light-white [2 -4 3])
                         (with-surface wood-s (cuboid {:center [0 0 0] :size [1 1 1]}))
                         (plane {:normal [0 0 1] :p0 [0 0 -0.5]
                                 :surface (surface {:pigment {:pattern :checker :colors [[1 1 1] [0 0 0]]}})})]
               :min-samples 1
               :max-samples 1}))
(def t (png-target 8 8))
(assert= (render s t 8 8) t)
