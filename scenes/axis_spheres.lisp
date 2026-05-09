; SDL port of scenes.rs::scene_axis_spheres.
;
; A central white sphere with three small RGB accent spheres
; positioned along +x, +y, +z. Useful as a quick orientation
; sanity check — red is right, green is "into the screen," blue
; is up, given the look-down-+y default camera.

(load "_common.lisp")

(def axis-spheres-scene
  (scene
    {:name          "Axis Spheres"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :lights        [(light-white [10 10 10])]
     :reflect-limit 2
     :oversample    2
     :objects
     [(sphere {:center [0 0 0] :r 1.0  :surface surface-white})
      (sphere {:center [3 0 0] :r 0.25 :surface surface-red})
      (sphere {:center [0 3 0] :r 0.25 :surface surface-green})
      (sphere {:center [0 0 3] :r 0.25 :surface surface-blue})]}))
