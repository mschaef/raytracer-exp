; SDL scene exercising the torus primitive, on its own and in CSG.
;
; Left to right as seen from the default camera:
;
;   * A red ring stood nearly upright, facing the camera: the hole
;     should show the checker floor through it.
;   * A grooved stand like the base of the POV xmastree: a short
;     cylinder minus a torus lying in its top face, which cuts a
;     rounded channel. A gold ring floats above it; the stand and floor
;     should show in its reflection.
;   * A purple ring with one quarter removed by a box: the cut ends of
;     the tube show the cutter's yellow surface. A ray can cross the
;     tube twice, so this is the case that needs two spans per ray.
;
; Camera framing note: default-camera looks down -y from [0 10 0] with
; +z up, so larger y is nearer the camera.

(load "_common.lisp")

(def upright-ring
  (torus {:center [-3 0 -0.8] :axis [0.3 1 0.2] :major 0.9 :minor 0.3
          :surface surface-red}))

(def grooved-stand
  (with-surface surface-white
    (difference (cylinder {:p0 [0 0 -2] :p1 [0 0 -1.6] :r 1.3})
                (torus {:center [0 0 -1.6] :axis [0 0 1] :major 0.9 :minor 0.15}))))

(def gold-ring
  (torus {:center [0 0 -0.6] :axis [0 0.4 1] :major 0.8 :minor 0.2
          :surface surface-gold}))

(def cut-ring
  (translate [3 0 -1.1]
    (rotate-x (/ pi 3)
      (difference (torus {:axis [0 0 1] :major 0.8 :minor 0.3 :surface surface-purple})
                  (cuboid {:center [1 1 0] :size [2 2 2] :surface surface-yellow})))))

(def torus-test-scene
  (scene
    {:name          "Torus Test"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :reflect-limit 3
     :objects
     [(light-white [10 10 10])
      upright-ring
      grooved-stand
      gold-ring
      cut-ring
      (plane {:normal [0 0 1] :p0 [0 0 -2] :surface surface-white-c})]}))
