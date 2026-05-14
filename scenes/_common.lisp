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

;; --------------------------------------------------------------------
;; Camera
;; --------------------------------------------------------------------
;;
;; Matches scenes.rs::default_camera: looking at the origin from
;; (0, 10, 0), world-up = +z, zoom = 1.0 (~53° vertical FOV).

(def default-camera
  (camera-looking-at [0 10 0] [0 0 0] [0 0 1] 1.0))
