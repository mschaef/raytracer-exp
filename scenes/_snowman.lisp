; Shared pieces of the snowman scenes, ported from snowman/avatar.pov,
; snowman.inc and friends in the POV-Ray projects
; (github.com/mschaef/povray-projects): the textures, the snowman and
; the bowtie.
;
; The snowman files set `assumed_gamma 1.0`, so their colours are
; already linear and are used as written (no `srgb`, unlike the other
; ports; see _pov.lisp).
;
; Snowman phase 1 stand-ins (see "Snowman port plan" in CLAUDE.md):
; - The blob body is a union of spheres at the radii where each blob
;   component alone reaches the threshold, less two eye sockets. The
;   real blob blends the spheres into a smooth neck (phase 2).
; - The `Dirty` and `Dirtier` bump normals are left out (phase 3).
; - `rgbf` filter colours are plain colours: every filter here is 0
;   apart from the mirror glass (phase 4).

(load "_pov.lisp")

;; --------------------------------------------------------------------
;; Textures
;; --------------------------------------------------------------------

; MatteFinish: ambient 0.1, diffuse 1.5, specular 0.2 (POV's default
; roughness 0.05 is :shininess 20).
(defn snow-matte [color]
  (surface {:color color :ambient 0.1 :light 1.5 :specular 0.2 :shininess 20}))

; MetallicFinish: MatteFinish plus phong 0.9, phong_size 120,
; reflection 0.5 and metallic. The renderer has one highlight, so the
; phong highlight (much the stronger) stands in for both.
(defn snow-metallic [color]
  (surface {:color color :ambient 0.1 :light 1.5 :specular 0.9 :shininess 120
            :reflection 0.5 :metallic true}))

(def matte-white  (snow-matte pov-white))
(def matte-black  (snow-matte pov-black))
(def matte-orange (snow-matte [1.0 0.5 0.0]))
(def dirty-snow-white matte-white)        ; DirtySnowWhite, less its bumps

(def metallic-red   (snow-metallic pov-red))
(def metallic-green (snow-metallic pov-green))
(def metallic-blue  (snow-metallic pov-blue))
(def metallic-black (snow-metallic pov-black))

;; --------------------------------------------------------------------
;; The snowman (snowman.inc's `snowman` macro)
;; --------------------------------------------------------------------

; A blob component `sphere { c, R, s }` contributes s (1 - d²/R²)² inside
; radius R. Alone, it reaches the threshold t at d = R sqrt(1 - sqrt(t/s)).
(def blob-threshold 0.008)
(defn blob-radius [r strength]
  (* r (sqrt (- 1 (sqrt (/ blob-threshold strength))))))

; Stand-in for the blob: body, head and (in avatar.pov only) a lump
; where the left arm joins, less the two negative eye sockets.
(defn snowman-body [arm-lump?]
  (with-surface dirty-snow-white
    (difference
      (group (concat [(sphere {:center [0 1 0.2] :r (blob-radius 1.1 1)})
                      (sphere {:center [0 2.5 0] :r (blob-radius 0.85 1.4)})]
                     (if arm-lump?
                       [(sphere {:center [0 1.5 1.0] :r (blob-radius 0.3 1.4)})]
                       [])))
      (sphere {:center [0.85 2.8 0.28] :r 0.12})
      (sphere {:center [0.85 2.8 -0.28] :r 0.12}))))

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
            ; The band, DirtyMetallicRed less its bumps.
            (with-surface metallic-red
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
