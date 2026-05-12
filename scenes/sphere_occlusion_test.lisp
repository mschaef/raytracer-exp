; SDL port of scenes.rs::scene_sphere_occlusion_test.
;
; Six spheres at varying depths, with the foreground purple sphere
; declared LAST in the objects list — proper occlusion (nearest-hit
; selection across all candidates, not first-hit) is required to
; render this correctly. If the renderer ever regresses to "first
; hit wins," the purple sphere disappears behind the green one.

(load "_common.lisp")

(def sphere-occlusion-test-scene
  (scene
    {:name          "Occlusion Test"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :reflect-limit 2
     :oversample    2
     :objects
     [(light-white [5 5 5])
      (sphere {:center [ 1.5  2.0  0.0] :r 0.7 :surface surface-orange})
      (sphere {:center [ 3.0  0.0  0.0] :r 1.0 :surface surface-red})
      (sphere {:center [-3.0  0.0  0.0] :r 1.0 :surface surface-blue})
      (sphere {:center [ 0.0  0.0  0.0] :r 1.0 :surface surface-green})
      (sphere {:center [ 0.0 -4.0  0.0] :r 3.0 :surface surface-yellow})
      ;; Foreground sphere at the back of the list — proper occlusion
      ;; required to make this visible.
      (sphere {:center [-1.5  2.0  0.0] :r 0.7 :surface surface-purple})]}))
