; Phase 6 — shared scene-construction helpers.
;
; Loaded by every scene file in scenes/ via (load "_common.lisp").
; Single source of truth for surface coefficients, the glossy helper,
; the surface-* presets, and the default look-down-+y camera. Mirrors
; the const fns and SURFACE_* constants in src/scenes.rs so ports
; produce byte-identical renders against the Rust pipeline.
;
; Underscore-prefixed filename signals "not a standalone scene" — the
; scenes/ directory's other files all define a Scene value.

;; --------------------------------------------------------------------
;; Surface coefficients
;; --------------------------------------------------------------------
;;
;; Match scenes.rs::AMBIENT, SPECULAR, LIGHT.

(def ambient    0.2)
(def specular   0.5)
(def light      0.6)

;; --------------------------------------------------------------------
;; Surface helpers
;; --------------------------------------------------------------------
;;
;; `glossy` is the lisp equivalent of `surface_glossy` from scenes.rs:
;; matte body with a modest specular highlight, no reflection, no
;; checker pattern. `reflective` mirrors the const fn of the same
;; name — adds a 0.2 reflection coefficient to an existing surface.
;; The reflective helper takes a *color* rather than a Surface
;; because the SDL doesn't (yet) expose surface-field accessors;
;; building from the color keeps it standalone.

(def glossy
  (fn [color]
    (surface {:color      color
              :ambient    ambient
              :specular   specular
              :light      light
              :checked    false
              :reflection 0.0})))

(def reflective
  (fn [color]
    (surface {:color      color
              :ambient    ambient
              :specular   specular
              :light      light
              :checked    false
              :reflection 0.2})))

;; `glassy` builds a see-through surface: same matte body + specular
;; highlight as `glossy`, plus a transmission coefficient. Phase 1
;; transmission is non-refractive (the transmitted ray continues
;; straight through), so a `glassy` sphere shows the geometry behind
;; it undistorted, blended in by `transparency`. Takes the
;; transparency as a parameter since variable transparency is the
;; whole point.

(def glassy
  (fn [color transparency]
    (surface {:color        color
              :ambient      ambient
              :specular     specular
              :light        light
              :checked      false
              :reflection   0.0
              :transparency transparency})))

;; `metallic` builds a metal surface: the :metallic flag tells the
;; renderer to tint both the mirror reflection and the specular
;; highlight by the body color and to suppress the Lambertian diffuse
;; term entirely (metals have essentially no diffuse lobe). A metallic
;; surface is always opaque — :transparency is ignored when :metallic
;; is true. Takes the reflection strength as a parameter since
;; polished-vs-dull is the main knob worth varying.

(def metallic
  (fn [color reflection]
    (surface {:color      color
              :ambient    ambient
              :specular   specular
              :light      light
              :checked    false
              :reflection reflection
              :metallic   true})))

;; --------------------------------------------------------------------
;; Surface presets — match SURFACE_* constants in scenes.rs
;; --------------------------------------------------------------------

(def surface-red    (glossy [1.0 0.0 0.0]))
(def surface-green  (glossy [0.0 1.0 0.0]))
(def surface-blue   (glossy [0.0 0.0 1.0]))
(def surface-purple (glossy [1.0 0.0 1.0]))
(def surface-orange (glossy [1.0 0.5 0.0]))
(def surface-yellow (glossy [1.0 1.0 0.0]))
(def surface-white  (glossy [1.0 1.0 1.0]))
(def surface-black  (glossy [0.0 0.0 0.0]))

; Reflective checkered ground used by most scenes.
(def surface-white-c
  (surface {:color      [0.2 0.2 0.2]
            :ambient    ambient
            :specular   specular
            :light      light
            :checked    true
            :reflection 0.5}))

; Metal presets. The color is the reflectance tint, not a diffuse
; body color — these surfaces take their appearance from what they
; reflect, tinted by these values.
(def surface-gold   (metallic [1.0  0.78 0.34] 0.7))
(def surface-silver (metallic [0.95 0.95 0.95] 0.8))
(def surface-copper (metallic [0.95 0.64 0.54] 0.6))

;; --------------------------------------------------------------------
;; Camera
;; --------------------------------------------------------------------
;;
;; Matches scenes.rs::default_camera: looking at the origin from
;; (0, 10, 0), world-up = +z, zoom = 1.0 (~53° vertical FOV).

(def default-camera
  (camera-looking-at [0 10 0] [0 0 0] [0 0 1] 1.0))

;; --------------------------------------------------------------------
;; Path-tracing / global-illumination defaults
;; --------------------------------------------------------------------
;;
;; A bundle of recommended scene-parameter knobs for GI scenes. The
;; renderer's default behavior has indirect lighting off
;; (`:indirect-limit 0`), so a GI scene has to opt in by setting the
;; four keys below in its `(scene { ... })` map:
;;
;;   :indirect-limit     gi-indirect-limit       ; safety cap (RR is primary)
;;   :min-samples        gi-min-samples          ; floor before variance check
;;   :max-samples        gi-max-samples          ; ceiling
;;   :variance-threshold gi-variance-threshold   ; early-termination spread
;;
;; `:indirect-limit` was the *primary* termination mechanism in
;; Phase 2 (a hard depth cap). Phase 3 added Russian roulette to
;; `shade_pixel`, so termination is now probabilistic — every
;; bounce decides with survival probability `min(surface.light *
;; max(scolor), 0.95)` whether to continue, and surviving paths
;; scale by `1/p` to keep the estimator unbiased. On Cornell-shape
;; matte surfaces (`p ≈ 0.72`) the expected path length is about
;; 3–4 bounces, so `gi-indirect-limit 8` is a comfortable safety
;; ceiling — well above what RR will reach in practice but bounded
;; enough to protect against pathological geometry.
;;
;; The sample budget is bumped well above the renderer's default
;; (32) because indirect rays add variance and we want the adaptive
;; oversampler to have room to resolve it; the variance threshold
;; is tightened so the loop doesn't terminate too eagerly in
;; penumbra regions where indirect contribution is highest. Faster
;; GI scenes (the gi-test smoke render) can override individual
;; knobs without having to remember the whole combination — pull
;; `gi-min-samples` 16 instead of 64, say.

(def gi-indirect-limit     8)
(def gi-min-samples        64)
(def gi-max-samples        1024)
(def gi-variance-threshold 0.003)
