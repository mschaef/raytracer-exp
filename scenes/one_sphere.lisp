; SDL port of scenes.rs::scene_one_sphere.
;
; Single orange sphere surrounded by three reflective checker planes
; (negative x, y, z faces). Tests reflection composition — the sphere
; appears in each plane, and the planes also reflect each other.

(load "_common.lisp")

(def one-sphere-scene
  (scene
    {:name          "Single Sphere, Reflective Planes"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :reflect-limit 2
     :oversample    2
     :objects
     [(light-white [10 10 10])
      (sphere {:center [0 0 0] :r 1 :surface surface-orange})
      (plane {:normal [1 0 0] :p0 [-2  0  0] :surface surface-white-c})
      (plane {:normal [0 1 0] :p0 [ 0 -2  0] :surface surface-white-c})
      (plane {:normal [0 0 1] :p0 [ 0  0 -2] :surface surface-white-c})]}))
