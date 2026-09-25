; The painted-wood train ornament's parts, from ornament/orn.pov and
; xmastree/trainorn.inc in the POV-Ray projects
; (github.com/mschaef/povray-projects). Used by ornament.lisp (the whole
; ornament) and xmastree.lisp (its frame, as one of the tree's
; ornaments). Load _pov.lisp first.

(load "_smokestack.lisp")

;; --------------------------------------------------------------------
;; Painted wood
;; --------------------------------------------------------------------

; red_wood, blue_wood, yellow_wood and black_wood: fine-grained wood
; (turbulence 0.05, scaled down to 0.05) in three shades of one colour.
(defn painted-wood [c0 c1 c2]
  {:pattern    :wood
   :turbulence 0.05
   :color-map  [[0.0 c0] [0.9 c1] [1.0 c2]]
   :transform  (affine-scale [0.05 0.05 0.05])})

(def red-wood    (painted-wood [0.75 0.125 0.125] [0.875 0.0625 0.0625] [1 0 0]))
(def blue-wood   (painted-wood [0.125 0.125 0.75] [0.0625 0.0625 0.875] [0 0 1]))
(def yellow-wood (painted-wood [0.75 0.75 0.125] [0.875 0.875 0.0625] [1 1 0]))
(def black-wood  (painted-wood [0.125 0.125 0.125] [0.0625 0.0625 0.0625] [0 0 0]))

;; --------------------------------------------------------------------
;; The frame and the engine
;; --------------------------------------------------------------------

; makeFrame: an eight-pointed star (a box and the same box turned 45°)
; with a round hole, in the given wood.
(defn make-frame [wood]
  (with-surface (pov-pigmented wood)
    (difference (group [(rotate-z (deg->rad 45) (box [-1.5 -1.5 -0.3] [1.5 1.5 0.3]))
                        (box [-1.5 -1.5 -0.3] [1.5 1.5 0.3])])
                (cylinder {:p0 [0 0 -1] :p1 [0 0 1] :r 1.2}))))

(defn wheel [x z0 z1]
  (cylinder {:p0 [x -1.35 z0] :p1 [x -1.35 z1] :r 0.8}))

; The engine: a red boiler, blue footplate, yellow cab with a window
; cut through, black roof, four red wheels and the smokestack. Each part
; is painted where it's defined, so its grain moves with the engine.
(def train
  (group
    [(with-surface (pov-pigmented red-wood)
       (group [(cylinder {:p0 [0 0 0] :p1 [3 0 0] :r 1})
               (wheel 0.8 1.2 1.4)
               (wheel 3.95 1.2 1.4)
               (wheel 0.8 -1.2 -1.4)
               (wheel 3.95 -1.2 -1.4)]))
     (with-surface (pov-pigmented blue-wood)
       (box [5.15 -1 -1.2] [-0.25 -1.5 1.2]))
     (with-surface (pov-pigmented yellow-wood)
       (difference (box [3 1.5 1] [5 -1 -1])
                   (box [3.5 0.5 1.2] [4.5 1.5 -1.2])))
     (with-surface (pov-pigmented black-wood)
       (box [5.15 1.5 -1.2] [2.15 1.9 1.2]))
     ; POV applies texture { black_wood } to the smokestack, but the mesh
     ; already carries its own white texture, which wins; so it's white,
     ; there and here.
     (translate [0.9 1.7 0] smokestack)]))
