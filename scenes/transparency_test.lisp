; SDL scene exercising Phase 1 transparency (transmission without
; refraction).
;
; A partially transparent sphere sits between the camera and an
; opaque red sphere, both resting on the reflective checker ground.
; The red sphere — and the checker floor — should be visible
; *through* the near sphere, blended in by its transparency
; coefficient. Because Phase 1 transmission is non-refractive (the
; transmitted ray continues straight through), the geometry behind
; the glass sphere shows up undistorted; the near sphere also shows
; its own shaded front surface mixed in.
;
; Camera framing note: default-camera looks down -y from [0 10 0],
; so a larger y is *nearer* the camera. The glassy sphere at y = 2
; is therefore in front of the opaque red sphere at y = -2.

(load "_common.lisp")

; Light blue glass, 70% transparent.
(def surface-glass (glassy [0.6 0.8 1.0] 0.7))

(def transparency-test-scene
  (scene
    {:name          "Transparency Test"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :reflect-limit 2
     :objects
     [(light-white [10 10 10])
      (sphere {:center [0  2 -1] :r 0.66 :surface surface-glass})
      (sphere {:center [0 -2 -1] :r 0.66 :surface surface-red})
      (plane  {:normal [0 0 1]   :p0 [0 0 -2] :surface surface-white-c})]}))
