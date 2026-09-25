; A painted-wood Christmas ornament, ported from ornament/orn.pov in the
; POV-Ray projects (github.com/mschaef/povray-projects): a toy train
; engine standing in front of a yellow eight-pointed frame, on a grey
; tilted backdrop. The parts live in _trainorn.lisp, shared with
; xmastree.lisp. Render at 4:3, e.g. SIZE=800x600.
;
; Every part is painted in fine-grained wood (see `painted-wood`). The
; one stand-in: the smokestack is a flat-shaded mesh, as in the original.

(load "_pov.lisp")
(load "_trainorn.lisp")

(def ornament
  (rotate-y (deg->rad 30)
    (group [(translate [2.5 0 0] (scale [4 4 3] (make-frame yellow-wood)))
            (translate [0 -1.8 0] (scale [1.2 1.2 1.2] train))])))

; plane { <0.5, 0.5, 1>, -12 }: POV normalizes a plane's normal, and the
; plane is 12 units from the origin along it, on the negative side.
(def backdrop-normal (normalize [0.5 0.5 1]))

(def ornament-scene
  (scene
    {:name       "Train Ornament"
     :camera     (pov-camera [2 2 20] [2 0 0])
     :background pov-black
     :objects
     [(light-white [16 16 16])
      (light-white [-16 16 16])
      ornament
      (plane {:normal backdrop-normal :p0 (p* backdrop-normal -12)
              :surface (pov-plain [0.4 0.4 0.4])})]}))
