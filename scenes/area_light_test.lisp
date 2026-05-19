; SDL scene exercising Phase 4 of the light-types plan: disk area
; lights with a hard-shadow baseline.
;
; A single disk area light hovers above the scene, aimed straight
; down at the checker floor. Phase 4 renders this as a hard-
; shadowed light: the shadow ray comes from the disk *center*
; exactly (Phase 5 will jitter across the disk for soft shadows),
; and the light's contribution is scaled by a Lambertian cosine
; factor against the disk's axis. Two consequences worth seeing in
; the render:
;
;   1. Only the half-space the front face points into receives
;      light from this source. With the disk axis at [0 0 -1] the
;      lit half-space is z < 4 (everything beneath the disk).
;   2. Brightness on the floor falls off smoothly as you move out
;      from directly beneath the disk: cos(theta) where theta is
;      the angle off-axis from the disk center.
;
; Three spheres along +x at progressively larger offsets show the
; cosine attenuation in object shading; the floor catches the
; corresponding gradient.
;
; Intensity is bumped to 3.0 because the cosine attenuation
; reduces effective brightness compared with an unattenuated
; point light. Tune to taste.

(load "_common.lisp")

; Sample budget — Phase 5 turned this scene from hard-shadowed
; into soft-shadowed (area lights now sample across the disk per
; pixel sample), and the default `:max-samples 32` is too tight
; for the penumbra to look clean. The adaptive sampler only
; spends the extra budget on noisy regions; flat-lit and fully-
; shadowed pixels still terminate at `:min-samples`.

(def area-light-test-scene
  (scene
    {:name               "Area Light Test"
     :camera             default-camera
     :background         [0.0 0.0 0.0]
     :reflect-limit      2
     :max-samples        128
     :variance-threshold 0.002
     :objects
     [;; Disk area light, 1.0 radius, aimed straight down.
      (light-area [0 0 4]    ; location (disk center)
                  [0 0 -1]   ; axis (disk normal, direction of emission)
                  1.0        ; radius
                  [1 1 1]    ; white
                  3.0)       ; intensity

      ;; Sphere directly under the light — cos(theta) ≈ 1, fully lit.
      (sphere {:center [ 0 0 -0.5] :r 0.5 :surface surface-red})

      ;; Sphere off to one side — cosine falloff visible.
      (sphere {:center [ 3 0 -0.5] :r 0.5 :surface surface-green})

      ;; Sphere far off — cosine even smaller; mostly the ambient
      ;; term plus what reflection picks up.
      (sphere {:center [-3 0 -0.5] :r 0.5 :surface surface-blue})

      ;; Reflective checker ground. The cosine gradient should be
      ;; clearly visible across this surface.
      (plane  {:normal [0 0 1] :p0 [0 0 -1] :surface surface-white-c})]}))
