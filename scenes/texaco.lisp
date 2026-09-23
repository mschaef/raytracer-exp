; The Texaco logo, ported from texaco/texaco.pov in the POV-Ray
; projects (github.com/mschaef/povray-projects): a red metal bowl with a
; silver five-pointed star, cut with a "T", standing across its rim.
; The reference renders are texaco.gif and texaco2.gif in that repo.
;
; Geometry follows the POV original's structure and numbers exactly,
; in POV's own coordinates (see _pov.lisp): the camera looks down +z
; at the origin and the bowl opens toward it. Surfaces are starting
; points in this renderer's shading model, to be tuned by eye.
;
; The POV original animates the star turning about y (`rotate y*clock`,
; clock 0 → -180 over 24 frames). `texaco-at` builds the scene for any
; angle, with or without the white backdrop plane that the newest
; texaco.pov adds; the older texacobackup.pov and texaco_animation.pov
; don't have it, and the black-background reference GIFs were rendered
; without it. `texaco-scene` is the still at clock 0 without the
; backdrop, matching the references.

(load "_pov.lisp")

(defn deg [d] (deg->rad d))

;; --------------------------------------------------------------------
;; The star
;; --------------------------------------------------------------------

; One wedge cutter: two unit-deep boxes 18° apart, the pair turned 36°.
; Five of these, spaced 72° apart, carve the points of the star out of
; a disk. The boxes run 0.01 past both faces of the disk so the cuts
; don't leave coincident surfaces.
(def na-108
  (rotate-z (deg 36)
    (group [(box [0 0 0.01] [1 -1 -1.01])
            (rotate-z (deg 18) (box [0 0 0.01] [1 -1 -1.01]))])))

(def star-ofs 0.32491969)

(defn star-cutter [k]
  (rotate-z (deg (* 72 k)) (translate [star-ofs 0 0] na-108)))

; A unit disk, one unit thick along -z, with the five wedges removed,
; turned so a point faces up.
(def star
  (rotate-z (deg -18)
    (apply difference
           (cylinder {:p0 [0 0 0] :p1 [0 0 -1] :r 1})
           (map star-cutter (range 5)))))

; The star with the "T" cut through it: a vertical stem and a crossbar.
(def texaco-star
  (difference star
              (group [(box [-0.08 0.10 0.01]  [0.08 -1.0 -1.01])
                      (box [-0.32 0.170 0.01] [0.32 0.015 -1.01])])))

;; --------------------------------------------------------------------
;; The logo
;; --------------------------------------------------------------------

(def surface-bowl (pov-metal-c pov-red))
(def surface-star (pov-metal-a pov-white))

; A hemispherical shell open toward the camera (-z): a unit sphere,
; minus a sphere 0.001 smaller, minus a cylinder covering the near half.
(def bowl
  (with-surface surface-bowl
    (difference (sphere {:center [0 0 0] :r 1})
                (sphere {:center [0 0 0] :r 0.999})
                (cylinder {:p0 [0 0 0] :p1 [0 0 -1.01] :r 1.01}))))

; The star flattened to 0.16 deep and centred on the rim plane, then
; turned `angle` degrees about y.
(defn logo-star [angle]
  (with-surface surface-star
    (rotate-y (deg angle)
      (translate [0 0 0.08]
        (scale [0.9 0.9 0.16] texaco-star)))))

(defn texaco-hemi-logo [angle]
  (group [bowl (logo-star angle)]))

;; --------------------------------------------------------------------
;; Scene
;; --------------------------------------------------------------------

; A white backdrop lit only by its own ambient term, far behind the
; logo (POV: plane { z, 10 } with finish { ambient 1 }).
(def backdrop
  (plane {:normal [0 0 1] :p0 [0 0 10]
          :surface (surface {:color pov-white :ambient 1.0 :light 0.0 :specular 0.0})}))

(defn texaco-at [angle backdrop?]
  (scene
    {:name          "Texaco"
     :camera        (pov-camera [0 0 -2.2] [0 0 0])
     :background    pov-black
     :reflect-limit 3
     :objects
     (if backdrop?
       [(light-white [3.25 3.25 -4]) (texaco-hemi-logo angle) backdrop]
       [(light-white [3.25 3.25 -4]) (texaco-hemi-logo angle)])}))

(def texaco-scene (texaco-at 0 false))
