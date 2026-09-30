; Shared pieces of the snowman scenes, ported from snowman/avatar.pov,
; snowman.inc and friends in the POV-Ray projects
; (github.com/mschaef/povray-projects): the textures, the snowman and
; the bowtie.
;
; The snowman files set `assumed_gamma 1.0`, so their colours are
; already linear and are used as written (no `srgb`, unlike the other
; ports; see _pov.lisp).
;
; POV's `rgbf` colours here all have a filter of 0 (they're plain
; colours), apart from avatar.pov's mirror glass, which uses the
; surface :filter (see snowman_avatar.lisp).

(load "_pov.lisp")

;; --------------------------------------------------------------------
;; Textures
;; --------------------------------------------------------------------

; The two bump normals: Dirty (bumps 0.3, scale 0.1, turbulence 1) and
; Dirtier (bumps 0.7, scale 0.15, turbulence 1.5).
(def dirty
  {:pattern :bumps :amount 0.3 :turbulence 1 :transform (affine-scale [0.1 0.1 0.1])})
(def dirtier
  {:pattern :bumps :amount 0.7 :turbulence 1.5 :transform (affine-scale [0.15 0.15 0.15])})

; MatteFinish: ambient 0.1, diffuse 1.5, specular 0.2 (POV's default
; roughness 0.05 is :shininess 20). `snow-matte-bumped` adds a bump
; normal.
(def matte-finish {:ambient 0.1 :light 1.5 :specular 0.2 :shininess 20})
(defn snow-matte [color]
  (surface (assoc matte-finish :color color)))
(defn snow-matte-bumped [color normal]
  (surface (assoc (assoc matte-finish :color color) :normal normal)))

; MetallicFinish: MatteFinish plus phong 0.9, phong_size 120,
; reflection 0.5 and metallic. The renderer has one highlight, so the
; phong highlight (much the stronger) stands in for both.
(def metallic-finish {:ambient 0.1 :light 1.5 :specular 0.9 :shininess 120
                      :reflection 0.5 :metallic true})
(defn snow-metallic [color]
  (surface (assoc metallic-finish :color color)))

(def matte-white  (snow-matte pov-white))
(def matte-black  (snow-matte pov-black))
(def matte-orange (snow-matte-bumped [1.0 0.5 0.0] dirtier))
(def dirty-snow-white (snow-matte-bumped pov-white dirty))

(def metallic-red   (snow-metallic pov-red))
(def metallic-green (snow-metallic pov-green))
(def metallic-blue  (snow-metallic pov-blue))
(def metallic-black (snow-metallic pov-black))
(def dirty-metallic-red (surface (assoc (assoc metallic-finish :color pov-red) :normal dirtier)))

;; --------------------------------------------------------------------
;; The snowman (snowman.inc's `snowman` macro)
;; --------------------------------------------------------------------

; The body: a blob of the body and head, two negative components for
; the eye sockets, and (in avatar.pov only) a lump where the left arm
; joins. Components are [center radius strength], as POV's
; `sphere { center, radius, strength }`.
(defn snowman-body [arm-lump?]
  (blob {:threshold  0.008
         :components (concat [[[0 1 0.2] 1.1 1]
                              [[0 2.5 0] 0.85 1.4]
                              [[0.85 2.80 0.28] 0.16 -0.7]
                              [[0.85 2.80 -0.28] 0.16 -0.7]]
                             (if arm-lump? [[[0 1.5 1.0] 0.3 1.4]] []))
         :surface    dirty-snow-white}))

(def snowman-nose
  (with-surface matte-orange
    (group [(pov-cone [0.8 2.5 0] 0.12 [1.25 2.48 0] 0.06)
            (pov-cone [1.2 2.5 0] 0.06 [1.6 2.3 -0.2] 0)])))

(def snowman-mouth
  (with-surface matte-black
    (group (map (fn [c] (sphere {:center c :r 0.05}))
                [[0.7 2.14 0] [0.67 2.10 0.14] [0.67 2.08 -0.14]
                 [0.65 2.10 0.28] [0.65 2.06 -0.28]]))))

(def snowman-eyes
  (with-surface metallic-black
    (group [(sphere {:center [0.65 2.75 0.28] :r 0.08})
            (sphere {:center [0.65 2.77 -0.28] :r 0.08})])))

; A unit cylinder along +y, scaled: POV's `cylinder { 0, y, 1 scale s }`.
(defn unit-cylinder [s]
  (scale s (cylinder {:p0 [0 0 0] :p1 [0 1 0] :r 1})))

; The top hat. The crown's `scale <0.5, 0, 0.5>` has a zero y, which POV
; changes to 1 (with a warning), so the crown is 1 tall.
(def top-hat
  (transform (pov-transform [[:rotate [10 0 -20]] [:translate [0.2 3.1 0.15]]])
    (group [(with-surface matte-black (unit-cylinder [0.5 1 0.5]))
            (with-surface matte-black (unit-cylinder [0.75 0.1 0.75]))
            ; The band.
            (with-surface dirty-metallic-red
              (translate [0 0.1 0] (unit-cylinder [0.501 0.2 0.501])))])))

; The left arm (avatar.pov only).
(def snowman-arm
  (with-surface metallic-red
    (group [(cylinder {:p0 [0 1.5 1.0] :p1 [0 2.0 1.5] :r 0.07})
            (cylinder {:p0 [0 2.0 1.5] :p1 [0.5 2.5 2.0] :r 0.07})])))

(defn snowman [arm?]
  (group (concat [(snowman-body arm?) snowman-nose snowman-mouth snowman-eyes top-hat]
                 (if arm? [snowman-arm] []))))

;; --------------------------------------------------------------------
;; The bowtie (snowman.inc's `Bowtie` macro)
;; --------------------------------------------------------------------

; Two cones from a point at the origin out to radius 3 at z = +-4, each
; capped by the outer half of a sphere squashed to 3x3x1, and a unit
; sphere at the knot. Unsurfaced; POV places it with
; scale <0.01, 0.05, 0.05>, rotate <-15, 0, 30>, translate.
(def bowtie
  (group [(sphere {:center [0 0 0] :r 1})
          (pov-cone [0 0 0] 0 [0 0 4] 3)
          (translate [0 0 4]
            (scale [3 3 1] (difference (sphere {:center [0 0 0] :r 1})
                                       (box [-1 -1 -1] [1 1 0]))))
          (pov-cone [0 0 0] 0 [0 0 -4] 3)
          (translate [0 0 -4]
            (scale [3 3 1] (difference (sphere {:center [0 0 0] :r 1})
                                       (box [-1 -1 0] [1 1 1]))))]))

(defn placed-bowtie [surface]
  (transform (pov-transform [[:scale [0.01 0.05 0.05]]
                             [:rotate [-15 0 30]]
                             [:translate [0.67 1.85 0.05]]])
    (with-surface surface bowtie)))

;; --------------------------------------------------------------------
;; The mirror, its glass and the compass (avatar.pov and sphere.pov)
;; --------------------------------------------------------------------

; The mirror glass: box { <-5, -0.5, -5>, <5, 0, 5> } with
; pigment { rgbf <0, 0, 0.1, 0.9> } and then texture { Glass3 }, which
; POV layers: Glass3 (near-white, filter 0.9, ambient 0.1, diffuse 0.1,
; reflection 0.1, specular 0.8, roughness 0.0003, phong 1; see
; `pov-glass-3`) over the dark blue filter. Through both, only a little
; dark blue light gets through, and what shows is mostly Glass3's own
; dim body. One surface can't hold two layers, so this is an equivalent
; chosen to match avatar.jpg's floor, a greyish navy (about 34, 34, 58):
; a grey-blue body with a little blue-tinted filter. Glass3 alone (white,
; filter 0.8, tried before its exact values were looked up) left the
; floor light grey; the blue layer alone left it nearly black. The
; highlight exponent (333) is from that first guess at the roughness;
; it's left as tuned.
(def mirror-glass
  (with-surface (surface {:color [0.2 0.2 0.4] :ambient 0.1 :light 0.1
                          :specular 0.8 :shininess 333 :reflection 0.1
                          :filter 0.2})
    (box [-5 -0.5 -5] [5 0 5])))

; The mirror under it: plane { y, -0.4999 } clipped to a thin box, in
; Silver with ambient 0.15, diffuse 0.05, reflection 0.8, phong 0.9,
; phong_size 120, metallic.
(def mirror
  (with-surface (surface {:color [0.90 0.91 0.98] :ambient 0.15 :light 0.05
                          :specular 0.9 :shininess 120 :reflection 0.8
                          :metallic true})
    (intersection (plane {:normal [0 1 0] :p0 [0 -0.4999 0]})
                  (box [-5 0 -5] [5 -0.51 5]))))

; The compass at <2, 2, -2>: a black ball and red, green and blue arrows
; 0.8 long along +x, +y and +z.
(def snowman-compass
  (group [(sphere {:center [2 2 -2] :r 0.16 :surface matte-black})
          (with-surface metallic-red
            (group [(cylinder {:p0 [2 2 -2] :p1 [2.8 2 -2] :r 0.04})
                    (pov-cone [2.8 2 -2] 0.08 [3 2 -2] 0)]))
          (with-surface metallic-green
            (group [(cylinder {:p0 [2 2 -2] :p1 [2 2.8 -2] :r 0.04})
                    (pov-cone [2 2.8 -2] 0.08 [2 3 -2] 0)]))
          (with-surface metallic-blue
            (group [(cylinder {:p0 [2 2 -2] :p1 [2 2 -1.2] :r 0.04})
                    (pov-cone [2 2 -1.2] 0.08 [2 2 -1] 0)]))]))
