; Cornell box — the canonical diffuse-interreflection test scene.
;
; A five-walled room — red left wall, green right wall, white floor /
; ceiling / back wall, open front for the camera — containing two
; white boxes: a tall one rotated toward the back-left, a short one
; rotated toward the front-right. Lit by a single disk area light
; near the ceiling.
;
; This is Phase 3 of the Cornell box plan, closed out by Phases 2-3
; of the path-tracing plan. Earlier phases stood in for missing
; renderer features: Phase 1 had a single point light (hard
; shadows), Phase 2 spread a 4x4 grid of point lights across a
; ceiling panel to fake soft shadows. Phase 3 (this version) uses
; the real things — a disk area light produces faithful soft
; shadows in `light_vector_area`, and `:indirect-limit gi-indirect-
; limit` turns on path-traced diffuse interreflection in
; `shade_pixel`. Path-tracing Phase 3 added Russian roulette,
; making the indirect contribution unbiased and energy-conserving
; while keeping the same `gi-indirect-limit` knob as a worst-case
; safety net. The two effects the Cornell box was originally
; designed to demonstrate — soft shadows around the boxes and
; color bleeding from the red/green walls onto adjacent white
; surfaces — should both be visible in the render.
;
; Matte ambient is dropped to 0 across the room. The `:ambient` field
; was a fake stand-in for indirect light filling shadowed faces off
; pure black; with real indirect light coming from the indirect
; branch, ambient is no longer needed (and would otherwise just
; brighten everything uniformly, washing out the color bleed effect
; we're after).
;
; Sample budget is bumped well above the renderer default. Indirect
; lighting adds variance — each diffuse hit's bounce direction is
; random, and a typical pixel only sees one bounce per primary ray —
; so the adaptive oversampler needs much more headroom to resolve
; clean color bleed. The values pulled from `_common.lisp`'s gi-*
; defaults are tuned for a Cornell-shaped indoor scene; expect a
; non-trivial render time.
;
; Coordinate convention follows the rest of scenes/: +z is up (the
; camera up-hint is [0 0 1]), +y is scene depth. The room is the
; 2x2x2 cube centered at the origin; the camera sits in front of the
; open -y face looking down +y toward the back wall.

(load "_common.lisp")

;; Matte surface helper — pure Lambertian, no specular sheen, no
;; reflection. Ambient is zero: with `:indirect-limit > 0` the indirect
;; branch in `shade_pixel` fills shadowed faces with bounced light
;; that's actually picked up from the walls (which is what produces
;; the color bleed), so adding a constant ambient term on top would
;; just wash out the effect.
(defn matte [color]
  (surface {:color      color
            :ambient    0.0
            :specular   0.0
            :light      0.85
            :checked    false
            :reflection 0.0}))

(def cb-white (matte [0.85 0.85 0.85]))
(def cb-red   (matte [0.75 0.1  0.1]))
(def cb-green (matte [0.1  0.65 0.15]))

;; Camera in front of the open face, framed so the back wall fills the
;; square frame: at distance 5 from the back wall, zoom 2.5 gives a
;; view half-height of exactly 1.0 — the back wall's half-height.
(def cornell-camera
  (camera-looking-at [0 -4 0] [0 0 0] [0 0 1] 1.5))

;; Ceiling area light. A disk radius 0.4 just below the ceiling,
;; aimed straight down. The disk is invisible to the camera (lights
;; never hit-test) but its emitter geometry drives soft-shadow
;; sampling — `light_vector_area` picks a per-pixel-sample point on
;; the disk for each shadow ray, so penumbra pixels see high variance
;; and the adaptive sampler resolves them naturally. Intensity 1.0;
;; the wall albedos are already a touch dim (0.75 / 0.65 / 0.85) so
;; the room doesn't blow out.
(def cornell-light
  (light-area [0 0 0.95]      ; center, just below the ceiling plane
              [0 0 -1]        ; axis (front face pointing down)
              0.4             ; disk radius
              [1.0 1.0 1.0]   ; white
              1.0))           ; intensity

(def cornell-box-scene
  (scene
    {:name               "Cornell Box"
     :camera             cornell-camera
     :background         [0.0 0.0 0.0]
     :reflect-limit      1
     ;; GI parameters pulled from _common.lisp's gi-* defaults.
     ;; Tuned for Cornell-shape indoor scenes; see the comment block
     ;; over those defs.
     :indirect-limit     gi-indirect-limit
     :min-samples        gi-min-samples
     :max-samples        gi-max-samples
     :variance-threshold gi-variance-threshold
     :objects
     [;; --- The room: five infinite planes ----------------------
      ;; Each normal faces inward, into the room.
      (plane {:normal [0  0  1] :p0 [0 0 -1] :surface cb-white})  ; floor
      (plane {:normal [0  0 -1] :p0 [0 0  1] :surface cb-white})  ; ceiling
      (plane {:normal [0 -1  0] :p0 [0 1  0] :surface cb-white})  ; back wall
      (plane {:normal [1  0  0] :p0 [-1 0 0] :surface cb-red})    ; left wall
      (plane {:normal [-1 0  0] :p0 [1  0 0] :surface cb-green})  ; right wall

      ;; --- The two inner boxes ---------------------------------
      ;; Each is built as a cuboid centered at the origin, rotated
      ;; in place about the vertical (z) axis, then translated into
      ;; position. `(translate (rotate-z (cuboid ...)))` composes
      ;; inner-to-outer, so the rotation happens while the box is
      ;; still at the origin — it spins in place rather than
      ;; orbiting.
      ;;
      ;; Both boxes are matte white; rather than repeating
      ;; `:surface cb-white` on each cuboid, the shared surface is
      ;; lifted onto a single `(with-surface ...)` wrapper around
      ;; both. The color bleed from the red and green walls onto
      ;; the boxes' facing sides is the whole point of the GI
      ;; phase — these *are* the surfaces we expect to see picking
      ;; up wall color.
      (with-surface cb-white
        (group
          [;; Tall box, back-left, rotated ~+18 degrees.
           (translate [-0.35 0.35 -0.375]
             (rotate-z (/ pi 10)
               (cuboid {:center [0 0 0] :size [0.55 0.55 1.25]})))

           ;; Short box, front-right, rotated ~-15 degrees.
           (translate [0.4 -0.4 -0.7]
             (rotate-z (/ pi -12)
               (cuboid {:center [0 0 0] :size [0.6 0.6 0.6]})))]))

      ;; The ceiling area light.
      cornell-light]}))
