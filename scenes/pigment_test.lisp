; SDL scene exercising procedural pigments.
;
; Left to right as seen from the default camera:
;
;   * A wooden cube, turned about two axes: rings on the faces the
;     pattern's z axis crosses, straight grain on the others. Because the
;     surface is attached inside the transforms, the grain turns with
;     the cube.
;   * A sphere of pale wood with heavy turbulence, so the rings wander.
;   * A cylinder of dark wood (POV's Dark_Wood colours, with a hard edge
;     in the colour map) whose pigment has its own transform, cut by a box to show that CSG faces take the same
;     texture.
;
; The floor is a two-colour checker pigment scaled to 1.5 units, in
; place of the fixed `:checked` pattern.
;
; Camera framing note: default-camera looks down -y from [0 10 0] with
; +z up, so larger y is nearer the camera.

(load "_common.lisp")

(def light-wood
  {:pattern    :wood
   :turbulence 0.1
   :color-map  [[0.0 [0.85 0.62 0.32]]
                [0.6 [0.74 0.50 0.24]]
                [1.0 [0.45 0.27 0.10]]]
   :transform  (affine-scale [0.25 0.25 0.25])})

(def wavy-wood (assoc light-wood :turbulence 0.3 :transform (affine-scale [0.3 0.3 0.3])))

(def dark-wood
  {:pattern    :wood
   :turbulence 0.05
   :color-map  [[0.0 [0.43 0.24 0.05]]
                [0.8 [0.40 0.33 0.06]]
                [0.8 [0.20 0.03 0.03]]
                [1.0 [0.20 0.03 0.03]]]
   :transform  (affine-scale [0.15 0.15 0.15])})

(defn wood [pigment]
  (surface {:pigment pigment :ambient 0.2 :light 0.7 :specular 0.3}))

(def floor-checker
  (surface {:pigment {:pattern :checker :colors [[0.9 0.9 0.9] [0.25 0.25 0.3]]
                      :transform (affine-scale [1.5 1.5 1.5])}
            :ambient 0.2 :light 0.6}))

(def pigment-test-scene
  (scene
    {:name          "Pigment Test"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :objects
     [(light-white [10 10 10])
      (translate [-3 0 -1.1]
        (rotate-z 0.6
          (rotate-x 0.5
            (with-surface (wood light-wood)
              (cuboid {:center [0 0 0] :size [1.4 1.4 1.4]})))))
      (sphere {:center [0 0 -1] :r 1 :surface (wood wavy-wood)})
      (translate [3 0 -2]
        (with-surface (wood dark-wood)
          (difference (cylinder {:p0 [0 0 0] :p1 [0 0 1.6] :r 0.8})
                      (cuboid {:center [0.6 0.6 1.4] :size [1 1 1]}))))
      (plane {:normal [0 0 1] :p0 [0 0 -2] :surface floor-checker})]}))
