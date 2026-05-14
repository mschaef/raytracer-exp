; SDL scene exercising the thin-lens depth-of-field camera.
;
; Three spheres at staggered depths along the camera's view axis,
; spread sideways so each one is visible on its own. The camera
; (camera-dof) focuses on the look-at point [0 0 0] — the distance
; from the camera at [0 10 0] is 10 world units — so the middle
; green sphere sits on the focus plane and renders sharp. The red
; sphere is nearer the camera and the blue sphere is farther, so
; both should render visibly blurred, the blur growing with the
; 0.3 aperture radius.
;
; Camera framing note: default-camera-style framing looks down -y
; from [0 10 0], so a larger world-y is *nearer* the camera. The
; red sphere at y = 3 is in front of the focus plane; the blue
; sphere at y = -3 is behind it.

(load "_common.lisp")

(def depth-of-field-test-scene
  (scene
    {:name          "Depth of Field Test"
     :camera        (camera-dof [0 10 0] [0 0 0] [0 0 1] 1.0 0.3)
     :background    [0.0 0.0 0.0]
     :reflect-limit 2
     :objects
     [(light-white [10 10 10])
      (sphere {:center [-2.5  3 0] :r 1.0 :surface surface-red})
      (sphere {:center [ 0.0  0 0] :r 1.0 :surface surface-green})
      (sphere {:center [ 2.5 -3 0] :r 1.0 :surface surface-blue})
      (plane  {:normal [0 0 1] :p0 [0 0 -1] :surface surface-white-c})]}))
