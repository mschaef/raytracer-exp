; Scene constructor — pulls together everything from the other binding
; tests. The structural-equality check at the end is the headline
; assertion for Phase 2: a script-built scene compares equal to one
; built from the same parts a second time.

(def red    (surface {:color [1.0 0.0 0.0] :ambient 0.2 :specular 0.5 :light 0.6}))
(def green  (surface {:color [0.0 1.0 0.0] :ambient 0.2 :specular 0.5 :light 0.6}))
(def ground (surface {:color [0.2 0.2 0.2] :ambient 0.2 :specular 0.5
                      :light 0.6 :checked true :reflection 0.5}))

(def cam (camera-looking-at [0 10 0] [0 0 0] [0 0 1] 1.0))

; Wrapped in a fn so we can build the same scene structure twice and
; assert that the two results are structurally equal. (Phase 4 will
; add `defn` as `(def x (fn ...))` sugar; for now `def` + `fn` is the
; idiom.)
;
; After the stage-2 collapse the scene constructor no longer takes a
; `:lights` key — lights live alongside geometry in `:objects` and
; the constructor wraps the list in a top-level `Shape::Group` as
; `Scene::root`. Bare `(light-white ...)` values are auto-wrapped
; into `Shape::Light` by `require_shape_value` at the binding
; boundary, so this composes without any explicit wrap.
(def make-scene
  (fn []
    (scene {:name "Test Scene"
            :camera cam
            :background [0 0 0]
            :objects [(light-white [10 10 10])
                      (sphere {:center [0 0 0] :r 1.0 :surface red})
                      (sphere {:center [3 0 0] :r 0.5 :surface green})
                      (plane  {:normal [0 0 1] :p0 [0 0 -2] :surface ground})]
            :reflect-limit 2
            :min-samples 4
            :max-samples 16
            :variance-threshold 0.01})))

(def s1 (make-scene))
(assert (scene? s1))

; Building the same scene a second time: every leaf is rebuilt, but
; every leaf's structural fields match, so the whole tree compares
; equal.
(def s2 (make-scene))
(assert= s1 s2)

; Differing field → not equal.
(def s3 (scene {:name "Different Scene"
                :camera cam
                :background [0 0 0]
                :objects [(light-white [10 10 10])
                          (sphere {:center [0 0 0] :r 1.0 :surface red})]
                :reflect-limit 2
                :min-samples 4
                :max-samples 16
                :variance-threshold 0.01}))
(assert (not= s1 s3))

; :background, :reflect-limit, :min-samples, :max-samples, and
; :variance-threshold all default if omitted.
(def s4 (scene {:name "Minimal"
                :camera cam
                :objects []}))
(assert (scene? s4))

; Empty :objects is valid (debug mode renders — no lights, no
; geometry, just the background color).
(assert= s4 (scene {:name "Minimal"
                    :camera cam
                    :objects []}))

; Negative check.
(assert (not (scene? nil)))
(assert (not (scene? cam)))
