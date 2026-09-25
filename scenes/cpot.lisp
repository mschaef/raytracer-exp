; A chrome coffee pot with a wooden handle and two glass cups of
; coffee on a checkerboard, ported from cpot/cpot.pov and
; cpot/coffeecup.inc in the POV-Ray projects
; (github.com/mschaef/povray-projects). Render at 4:3, e.g.
; SIZE=800x600.
;
; Stand-ins: the glass (T_Glass4) is see-through without the filter's
; tint, which at its near-white colour changes little. It has no ior in
; the original, so POV didn't refract it either.

(load "_pov.lisp")

(defn cyl [p0 p1 r] (cylinder {:p0 p0 :p1 p1 :r r}))

; POV's torus { major, minor } lies in the xz plane around the origin.
(defn pov-torus [major minor]
  (torus {:center [0 0 0] :axis [0 1 0] :major major :minor minor}))

(def dark-wood (pov-pigmented pov-dark-wood-pigment))

;; --------------------------------------------------------------------
;; The coffee pot
;; --------------------------------------------------------------------

; The spout's axis, which also cuts its hole in the pot.
(def spout-p0 [1 3 0])
(def spout-p1 [5 7 0])

(def pot-casting
  (with-surface pov-chrome
    (group
      [; A metal lip at the bottom of the pot.
       (pov-torus 3.0 0.1)
       ; The pot, with a hole for the spout.
       (difference (cyl [0 0 0] [0 6 0] 3)
                   (cyl [0 0.2 0] [0 6.2 0] 2.8)
                   (cyl spout-p0 spout-p1 0.5))
       ; The spout: a tube, cut off where it would run through the
       ; pot's far side and emptied inside the pot.
       (difference (intersection (difference (cyl spout-p0 spout-p1 0.5)
                                             (cyl spout-p0 spout-p1 0.4))
                                 (box [0 -6 -1] [6 6 1]))
                   (cyl [0 0 0] [0 6 0] 2.8))
       ; The studs joining the handle to the pot.
       (difference (group [(cyl [-4 1 0] [-3 1 0] 0.2)
                           (cyl [-4 5 0] [-3 5 0] 0.2)])
                   (cyl [-4.2 0.75 0] [-4.2 5.25 0] 0.5)
                   (cyl [0 0 0] [0 6 0] 3))
       ; The rings holding the wooden handle.
       (difference (cyl [-4.2 0.75 0] [-4.2 5.25 0] 0.5)
                   (cyl [-4.2 0.5 0] [-4.2 5.5 0] 0.4)
                   (cyl [-4.2 1 0] [-4.2 5.0 0] 0.6))])))

(def lid
  (group
    [(with-surface pov-chrome
       (group [(difference (sphere {:center [0 3.05 0] :r 4.2})
                           (cyl [0 -2 0] [0 6.05 0] 5))
               (translate [0 6.1 0] (pov-torus 3.0 0.1))
               (cyl [0 7.1 0] [0 8.1 0] 0.2)]))
     ; The wooden knob.
     (sphere {:center [0 8.1 0] :r 0.6 :surface dark-wood})]))

(def coffee-pot
  (group
    [pot-casting
     ; The wooden handle.
     (cylinder {:p0 [-4.2 0.25 0] :p1 [-4.2 5.75 0] :r 0.45 :surface dark-wood})
     ; The base.
     (cylinder {:p0 [0 0 0] :p1 [0 -0.75 0] :r 3.1 :surface (pov-plain pov-black)})
     lid]))

;; --------------------------------------------------------------------
;; The cups (coffeecup.inc)
;; --------------------------------------------------------------------

(def coffee
  (surface {:color (srgb [0.65 0.65 0.4]) :ambient 0.1 :light 0.6 :specular 0.3}))

; A glass cup of coffee, one unit tall, standing on the origin with its
; handle toward +x.
(def coffee-cup
  (group
    [(with-surface pov-glass4
       (group [; The body.
               (difference (cyl [0 0 0] [0 1 0] 0.5)
                           (cyl [0 0.1 0] [0 1.1 0] 0.42))
               ; The handle: an upright ring, less the part inside
               ; the body.
               (difference (translate [0.6 0.7 0] (rotate-x (deg->rad 90) (pov-torus 0.18 0.05)))
                           (cyl [0 0 0] [0 1 0] 0.5))
               ; The lip around the top.
               (translate [0 1 0] (pov-torus 0.475 0.025))]))
     ; The coffee, with a shallow dip in its surface.
     (with-surface coffee
       (difference (cyl [0 0.1 0] [0 0.85 0] 0.42)
                   (group [(cyl [0 0.83 0] [0 0.86 0] 0.40)
                           (translate [0 0.845 0] (pov-torus 0.40 0.015))])))]))

; object { coffee_cup scale 4 rotate <0, angle, 0> translate at }
(defn place-cup [angle at]
  (translate at (rotate-y (deg->rad angle) (scale [4 4 4] coffee-cup))))

;; --------------------------------------------------------------------
;; The scene
;; --------------------------------------------------------------------

(def cpot-scene
  (scene
    {:name       "Coffee Pot"
     :camera     (pov-camera [6 13 12] [0 3 3])
     :background pov-black
     :objects
     [(light-white [16 16 -16])
      (light-white [16 16 16])
      coffee-pot
      (place-cup 18 [-6 0 4])
      (place-cup -30 [-1 0 7.5])
      (plane {:normal [0 1 0] :p0 [0 -4 0]
              :surface (pov-pigmented {:pattern   :checker
                                       :colors    [pov-black pov-white]
                                       :transform (affine-scale [5 5 5])})})]}))
