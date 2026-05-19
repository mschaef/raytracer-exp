; SDL scene exercising Phase 5 of the light-types plan: area light
; soft shadows.
;
; A disk area light with a visibly large radius hovers above a
; single sphere; the sphere casts a shadow onto the reflective
; checker floor. Phase 5 routes a per-pixel-sample disk coordinate
; through to `light_vector_area`, so each pixel sample shoots its
; shadow ray from a different jittered point on the disk's
; surface. The result: a soft shadow with a clear penumbra band
; around the umbra. Pixels in the penumbra see some samples reach
; the light and some not — high variance — and the adaptive
; oversampler keeps sampling them until they stabilize. The
; render-samples.png heatmap should light up distinctly in the
; penumbra region, which is the "adaptive sampler doing the
; soft-shadow work" verification the plan calls for.
;
; Geometry: disk light at z=4 with radius 1.5 (large enough to
; cast a visible penumbra), sphere of radius 0.6 at z=1.5
; (between the light and the floor), checker floor at z=-1.
; Intensity bumped to 4.0 since the cosine attenuation reduces
; effective brightness compared with an unattenuated point light.

(load "_common.lisp")

; Sample budget — soft shadows need more shadow rays per pixel than
; the default `:max-samples 32` provides to look clean in the
; penumbra. The adaptive sampler only spends the extra budget on
; noisy regions, so flat-lit and fully-shadowed pixels still
; terminate at `:min-samples`; only the penumbra band pays the
; higher cost. `:variance-threshold 0.002` is tightened from the
; 0.005 default so the sampler actually spends the headroom.

(def soft-shadow-test-scene
  (scene
    {:name               "Soft Shadow Test"
     :camera             default-camera
     :background         [0.0 0.0 0.0]
     :reflect-limit      2
     :max-samples        128
     :variance-threshold 0.005
     :objects
     [;; Disk area light, 1.5 radius — large enough for a clearly
      ;; visible penumbra at the floor distance.
      (light-area [0 0 4]    ; location (disk center)
                  [0 0 -1]   ; axis (disk normal, direction of emission)
                  1.5        ; radius
                  [1 1 1]    ; white
                  4.0)       ; intensity

      ;; The shadow-caster: a single sphere between the light and
      ;; the floor.
      (sphere {:center [0 0 1.5] :r 0.6 :surface surface-red})

      ;; Reflective checker floor — the shadow falls here.
      (plane  {:normal [0 0 1] :p0 [0 0 -1] :surface surface-white-c})]}))
