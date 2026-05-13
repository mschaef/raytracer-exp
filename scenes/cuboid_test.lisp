; SDL port of scenes.rs::scene_cuboid_test.
;
; Three cuboids (tall red, small green, wide blue) plus a yellow
; sphere for primitive-mixing confirmation, all sitting on the
; reflective checker ground.

(load "_common.lisp")

(def cuboid-test-scene
  (scene
    {:name          "Cuboid Test"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :reflect-limit 2
     :objects
     [(light-white [10 10 10])
      ;; Tall narrow red box at the origin.
      (cuboid {:center [0 0 0]      :size [1.5 1.5 2.5] :surface surface-red})
      ;; Small green cube floating to the right.
      (cuboid {:center [3 0 0.5]    :size [1 1 1]       :surface surface-green})
      ;; Wide flat blue slab on the left.
      (cuboid {:center [-2.5 0.5 -0.75] :size [1.5 2 0.5] :surface surface-blue})
      ;; Sphere for visual reference and to confirm interaction with
      ;; existing primitives still works.
      (sphere {:center [1 -2.5 0.5] :r 0.6 :surface surface-yellow})
      ;; Reflective checkered ground plane.
      (plane {:normal [0 0 1] :p0 [0 0 -2] :surface surface-white-c})]}))
