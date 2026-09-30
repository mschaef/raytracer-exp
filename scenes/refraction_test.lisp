; Refraction (history entry 88, refraction plan Phase 3): transparent
; surfaces with an index of refraction bend the light through them, by
; Snell's law on the way in and out, with total internal reflection past
; the critical angle.
;
; In front of a checked wall and floor, from left to right in the render
; (this renderer's camera puts +x on the left): three glass spheres with
; :ior 1.0 (no bending, as before refraction), 1.33 (water) and 1.5
; (glass), and a thick glass slab (ior 1.5) turned 35 degrees. The higher
; the index, the more strongly a sphere shrinks and flips what's behind
; it. The slab's faces are parallel, so what's seen through it keeps its
; direction but is shifted sideways.

(load "_common.lisp")

(defn deg [d] (deg->rad d))

(defn glass [ior]
  (surface {:color [1 1 1] :ambient 0.0 :light 0.0 :specular 0.8
            :shininess 300 :reflection 0.05 :transparency 0.95 :ior ior}))

; Checks shifted half a cell along `offset`: a plane lying exactly on a
; cell boundary would flip between the two colours from rounding alone,
; speckling the render.
(defn checks [c0 c1 offset]
  (surface {:pigment {:pattern :checker :colors [c0 c1]
                      :transform (affine-translation offset)}
            :ambient 0.2 :light 0.8 :specular 0.0}))

(def refraction-test-scene
  (scene
    {:name          "Refraction Test"
     :size          [640 400]
     :camera        (camera-looking-at [0 -10 3] [0 0 1.2] [0 0 1] 1.0)
     :background    [0 0 0]
     :reflect-limit 2
     :objects
     [(light-white [-4 -9 10])
      (plane {:normal [0 0 1] :p0 [0 0 0] :surface (checks [0.9 0.9 0.9] [0.1 0.1 0.1] [0 0 0.5])})
      (plane {:normal [0 -1 0] :p0 [0 4 0] :surface (checks [0.9 0.2 0.2] [0.9 0.9 0.9] [0 0.5 0])})
      (sphere {:center [4.2 0 1.2]  :r 1.2 :surface (glass 1.0)})
      (sphere {:center [1.4 0 1.2]  :r 1.2 :surface (glass 1.33)})
      (sphere {:center [-1.4 0 1.2] :r 1.2 :surface (glass 1.5)})
      (translate [-4.2 0 1.1]
        (rotate-z (deg 35)
          (cuboid {:center [0 0 0] :size [2.2 0.8 2.2] :surface (glass 1.5)})))]}))
