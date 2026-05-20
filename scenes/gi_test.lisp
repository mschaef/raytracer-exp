; gi_test — minimal color-bleed smoke render.
;
; The "is this working" test render for path-traced indirect lighting.
; A small saturated-red box sits right next to a white sphere on the
; reflective checker ground, lit by a single ceiling area light. The
; effect to see: the side of the sphere facing the red box picks up a
; visible red tint that the direct-lighting term alone cannot account
; for. That is diffuse interreflection — the indirect branch in
; `shade_pixel` bouncing one ray off the red wall, picking up its
; albedo, and contributing it to the sphere's shading.
;
; Sample budgets are deliberately lower than the Cornell box so this
; renders in a fraction of the time but still shows the effect
; clearly — the scene is open (no enclosing room) and most pixels
; either see no indirect light or one strong bounce, so the indirect
; variance is concentrated in a narrow region rather than spread
; everywhere. The adaptive oversampler will route extra samples
; toward the area on the sphere where the red box sits — which is
; exactly the region we want to look clean. Bump :max-samples
; further if the tint looks noisy.
;
; The ground plane is `surface-white-c` (the standard reflective
; checker), not a matte plane, because a reflective ground both
; tests that the indirect branch composes correctly with the
; reflection branch in `shade_pixel` and gives the scene some
; visual grounding.
;
; Coordinate convention follows the rest of scenes/: +z is up, +y
; is scene depth. The default-camera frame puts the colored box and
; the white sphere comfortably in view.

(load "_common.lisp")

;; Matte body for the colored box. Saturated red so the bounce off
;; it has a strong tint; ambient zero so the bounce isn't drowned
;; out by an ambient term that would brighten the sphere uniformly.
(def matte
  (fn [color]
    (surface {:color      color
              :ambient    0.0
              :specular   0.0
              :light      0.9
              :checked    false
              :reflection 0.0})))

(def gi-red   (matte [0.85 0.08 0.08]))
(def gi-white (matte [0.85 0.85 0.85]))

(def gi-test-scene
  (scene
    {:name               "GI Test"
     :camera             default-camera
     :background         [0.0 0.0 0.0]
     :reflect-limit      1
     ;; Indirect lighting on, with sample budgets sized for a fast
     ;; smoke render rather than a final image. The defaults pulled
     ;; from gi-* are tuned for Cornell-shaped enclosed scenes;
     ;; this open scene needs less. `:min-samples 16` is a quarter
     ;; of `gi-min-samples`; `:max-samples 256` a quarter of
     ;; `gi-max-samples`.
     :indirect-limit     gi-indirect-limit
     :min-samples        16
     :max-samples        256
     :variance-threshold gi-variance-threshold
     :objects
     [;; The white sphere — the receiver of indirect light.
      (sphere {:center [-0.7 0 -0.5] :r 0.5 :surface gi-white})

      ;; The red box, placed close enough to the sphere that a
      ;; sizeable fraction of the sphere's facing hemisphere sees
      ;; it. The box is centered just off the +x side of the
      ;; sphere; the bounce off its front (-x) face is what tints
      ;; the sphere's facing side.
      (cuboid {:center [0.7 0 -0.7] :size [0.6 1.2 0.6]
               :surface gi-red})

      ;; Ceiling-ish area light, off-axis so the boxes cast shadows
      ;; into the visible part of the frame and the sphere has a
      ;; well-defined lit / shadowed split. Disk axis straight down.
      (light-area [0 0 4]            ; center
                  [0 0 -1]           ; axis (front face down)
                  0.6                ; disk radius
                  [1.0 1.0 1.0]      ; white
                  2.0)               ; intensity

      ;; Reflective checker ground.
      (plane {:normal [0 0 1] :p0 [0 0 -1] :surface surface-white-c})]}))
