; Back faces phase 3 (history entry 86): spheres and cuboids report the
; far wall to a ray that starts inside them.
;
; The camera and the light are inside a large opaque checked sphere, the
; dome. Before phase 3 a sphere seen from inside reported nothing, so
; the dome was invisible and the render showed the black background; now
; its inside is the backdrop. Inside it, a glass sphere and a glass cube
; in front of an opaque red sphere, which look as they did before: a ray
; through glass now meets the glass's exit face and passes straight on
; (back faces phase 2), where it used to skip it.

(load "_common.lisp")

(def dome-surface
  (surface {:color [0.8 0.8 0.8] :ambient 0.2 :light 0.7 :specular 0.0 :checked true}))

(def glass (glassy [0.6 0.8 1.0] 0.7))

(def back-faces-test-scene
  (scene
    {:name          "Back Faces Test"
     :size          [480 360]
     :camera        (camera-looking-at [0 -8 3] [0 0 1] [0 0 1] 1.0)
     :background    [0 0 0]
     :reflect-limit 2
     :objects
     [(light-white [0 -4 8])
      (sphere {:center [0 0 0] :r 12 :surface dome-surface})
      (sphere {:center [-2 0 1] :r 1.2 :surface glass})
      (cuboid {:center [2 0 1] :size [2 2 2] :surface glass})
      (sphere {:center [0 4 1.5] :r 1.5 :surface surface-red})]}))
