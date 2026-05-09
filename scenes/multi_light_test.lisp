; SDL port of scenes.rs::scene_multi_light_test.
;
; Two colored point lights flanking a white sphere. The center sphere
; should show a clear red-on-the-left, blue-on-the-right gradient with
; a magenta band where both lights reach. The smaller accent spheres
; confirm contributions still sum correctly when multiple objects are
; present, and the ground catches color-tinted shadows from each light
; cast in opposite directions.

(load "_common.lisp")

(def multi-light-test-scene
  (scene
    {:name          "Multi-Light Test"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :reflect-limit 2
     :oversample    2
     :lights
     [;; Red light coming from image-left, slightly behind the camera.
      (light-point [-5 5 5] [1.0 0.2 0.2] 1.0)
      ;; Blue light from image-right, lower intensity so the asymmetry
      ;; between the two contributions is visible.
      (light-point [ 5 5 5] [0.2 0.4 1.0] 0.7)]
     :objects
     [;; Central white sphere — picks up whatever color the lights
      ;; throw at it without bias.
      (sphere {:center [0 0 0] :r 1 :surface surface-white})
      ;; Smaller accent spheres for visual reference and to confirm
      ;; shadows from one occluder don't affect another.
      (sphere {:center [-2.5 0 -0.5] :r 0.4 :surface surface-white})
      (sphere {:center [ 2.5 0 -0.5] :r 0.4 :surface surface-white})
      ;; Reflective checkered ground plane.
      (plane {:normal [0 0 1] :p0 [0 0 -1.5] :surface surface-white-c})]}))
