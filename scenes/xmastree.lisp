; The Christmas tree, ported from xmastree/xmastree.pov in the POV-Ray
; projects (github.com/mschaef/povray-projects), at the settings its
; final render used: gDetail 4, gTreeStages 6, gAngle 3 (the
; off-centre shot), 4:3 frame. Render at 4:3, e.g. SIZE=640x480.
;
; Geometry follows the original's numbers, in POV's own coordinates
; (see _pov.lisp). The structure is functional rather than POV's
; textual expansion:
;
;   * POV's paths.inc keeps a global, mutated path position. Here a
;     branch is a list of steps (chains, loops, ornament slots, the
;     end cap), and `walk` threads the path position through them with
;     `reduce`, pairing each step with where it starts.
;   * Beads are plain spheres at computed world positions
;     (`affine-apply`), not spheres wrapped in transforms, and the whole
;     tree goes into one `bvh`.
;   * POV's sequential `rand` / `Rand_Gauss` calls become keyed
;     `random` / `random-gaussian` calls, so the tree has the original's
;     statistics but not its exact ornament layout.
;
; Stand-ins for features the renderer doesn't have yet (see
; docs/povray_gap_analysis.md, §5):
;
;   * The stand's T_Wood25 is two layers of wood in POV; here it's the
;     bottom layer alone (see `pov-t-wood25-pigment` in _pov.lisp).
;   * The white sky_sphere and the white hollow sphere of radius 2000
;     around everything become a white background (`xmas-ground` in
;     _pov.lisp).

(load "_pov.lisp")
(load "_trainorn.lisp")

;; --------------------------------------------------------------------
;; Constants (names follow the POV original)
;; --------------------------------------------------------------------

(def seed 700)                    ; RdmA = seed(700)

; POV offsets coincident CSG faces by EPS = 0.000001, which is 100x
; smaller than the renderer's `epsilon` (its self-intersection
; tolerance): a reflection or shadow ray leaving one face could skip the
; other. Offset by the renderer's value instead.
(def eps epsilon)

(def bead-size 0.1)
(def bead-r (/ bead-size 2))
; doPerturbTranslate: Rand_Gauss(0, 1) * BEAD_PERTURB / 2 per axis,
; with BEAD_PERTURB = BEAD_SIZE / 5.
(def bead-jitter (/ (/ bead-size 5) 2))
(def trunk-dia 0.15)
(def tree-stages 6)
(def stage-spacing 2.75)
(def branches-per-layer 10)

(defn deg [d] (deg->rad d))
(defn rot-x [d] (affine-rotation-x (deg d)))
(defn rot-y [d] (affine-rotation-y (deg d)))
(defn rot-z [d] (affine-rotation-z (deg d)))

;; --------------------------------------------------------------------
;; Surfaces
;; --------------------------------------------------------------------

; Green * 0.8 with POV's default finish.
(def surface-beads  (pov-plain [0 0.8 0]))
(def surface-trunk  (pov-metal-e pov-brass3))           ; T_Brass_3E
(def surface-hook   (pov-metal-c pov-silver3))          ; T_Silver_3C
(def surface-stand  (pov-pigmented pov-t-wood25-pigment)) ; T_Wood25

; makeOrnamentByID pairs these pigments with F_MetalC.
(def ornament-colors [pov-gold3 pov-red pov-silver3 pov-yellow pov-blue])

;; --------------------------------------------------------------------
;; Ornaments
;; --------------------------------------------------------------------

; makeOrnamentHook: a crown of prongs on a cap (for the capped styles)
; and a small loop on top. Its own silver surface.
(defn ornament-hook [has-cap]
  (let [loop-ring (translate [0 0.23 0]
                    (rotate-y (deg 90)
                      (rotate-x (deg 90)
                        (torus {:major 0.12 :minor 0.015}))))
        ; A ring, minus its bore, keeping only the parts inside three
        ; crossed cylinders: six prongs.
        crown (difference
                (cylinder {:p0 [0 0 0] :p1 [0 0.13 0] :r (+ 0.15 eps)})
                (cylinder {:p0 [0 0 0] :p1 [0 (+ 0.13 eps) 0] :r 0.075})
                (difference
                  (cylinder {:p0 [0 (- eps) 0] :p1 [0 0.13 0] :r (+ 0.15 (* 2 eps))})
                  (group (for [i (range 3)]
                           (rotate-y (deg (* i 120))
                             (cylinder {:p0 [1 0.13 0] :p1 [-1 0.13 0] :r 0.06}))))))]
    (with-surface surface-hook
      (group (if has-cap [crown loop-ring] [loop-ring])))))

; The four decorative styles of makeBallOrnament.
(defn ball-body [style]
  (cond
    (= style 0) [(sphere {:center [0 0 0] :r 0.5})]
    (= style 1) [(sphere {:center [0 0.21 0] :r 0.28})
                 (sphere {:center [0 -0.21 0] :r 0.28})]
    (= style 2) [(scale [2 1 2] (sphere {:center [0 0.21 0] :r 0.28}))
                 (cone {:p0 [0 0.21 0] :p1 [0 -0.49 0] :r 0.4})]
    :else (for [i (range 6)
                part [(cylinder {:p0 [0 0 0] :p1 [0 0.55 0] :r 0.04})
                      (sphere {:center [0 0.55 0] :r 0.07})]]
            (rotate-x (deg (* i 60)) part))))

; makeBallOrnament: the body with a hollow cap cylinder on top (style 3,
; the spiky star, has no cap), plus the hook.
(defn ball-ornament [color style]
  (let [has-cap (< style 3)
        body (if has-cap
               (difference (group (conj (ball-body style)
                                        (cylinder {:p0 [0 0 0] :p1 [0 0.6 0] :r 0.12})))
                           (cylinder {:p0 [0 0 0] :p1 [0 (+ 0.6 eps) 0] :r 0.1}))
               (group (ball-body style)))]
    (group [(with-surface (pov-metal-c color) body)
            (translate [0 0.5 0] (ornament-hook has-cap))])))

; trainorn.inc's frame in yellow_wood (`make-frame` in _trainorn.lisp).
(def yellow-frame (make-frame yellow-wood))

; makeTrainOrnament(0, 0): the yellow frame without the train, turned a
; random amount about y.
(defn frame-ornament [key]
  (rotate-y (deg (+ 75 (* 30 (apply random seed (conj key 2)))))
    (scale [0.08 0.08 0.08]
      (translate [-2.5 0.1 0]
        (translate [2.5 0 0]
          (scale [4 4 3] yellow-frame))))))

; makeOrnamentByID: ids 0-19 are five colours x four styles, 20 is the
; frame.
(defn ornament-by-id [id key]
  (if (= id 20)
    (frame-ornament key)
    (ball-ornament (nth ornament-colors (mod id 5)) (quot id 5))))

;; --------------------------------------------------------------------
;; Branches
;; --------------------------------------------------------------------

; A bead's centre in branch-local coordinates, plus its Gaussian jitter.
; `key` identifies the bead, so its jitter doesn't depend on evaluation
; order.
(defn jittered [center key]
  (p+ center (for [axis [0 1 2]]
               (* bead-jitter (apply random-gaussian seed (conj key axis))))))

; A branch as a list of steps, in path order (makeBranch):
;   [:chain n twist]  n pairs of beads along +x, twisted about x
;   [:loop ydir]      two helical loops of 20 beads, rising by ydir
;   [:ornament i]     the i-th ornament slot
;   [:cap]            five beads in a half circle at the tip
(defn branch-steps [stages]
  (concat [[:chain 12 0]]
          (mapcat (fn [i]
                    (concat [[:ornament i]]
                            (if (< i stages)
                              [[:loop (if (= 0 (mod i 2)) 1.1 -1.1)]
                               [:chain 4 0] [:chain 5 180] [:chain 6 0]]
                              [])))
                  (range (+ stages 1)))
          [[:cap]]))

; How far a step moves the path (pathMoveRelative).
(defn step-advance [step]
  (let [kind (first step)]
    (cond (= kind :chain) [(* bead-size (nth step 1)) 0 0]
          (= kind :loop)  [0 (* bead-size (nth step 1)) 0]
          :else           [0 0 0])))

; Pair each step with the path position it starts at, threading the
; position through the steps (paths.inc, functionally). The path begins
; at the trunk's surface (pathBeginAt(<TRUNK_DIA, 0, 0>)).
(defn walk [steps]
  (nth (reduce (fn [[loc placed] step]
                 [(p+ loc (step-advance step)) (conj placed [step loc])])
               [[trunk-dia 0 0] []]
               steps)
       1))

; The bead centres (branch-local, jittered) a step contributes.
(defn step-beads [step loc key]
  (let [kind (first step)]
    (cond
      ; makeBeadChain: move one bead along, then a pair of beads either
      ; side of the path, turned about x by a growing twist.
      (= kind :chain)
      (let [n (nth step 1)
            twist (nth step 2)]
        (for [k (range n)
              side [-1 1]]
          (jittered (p+ (p+ loc [(* bead-size (+ k 1)) 0 0])
                        (affine-apply (rot-x (/ (* k twist) (- n 1)))
                                      [0 0 (* side bead-size)]))
                    (conj key k side))))

      ; makeBeadLoop: two loops of radius 0.3 either side of the path,
      ; each a full turn, rising in opposite directions.
      (= kind :loop)
      (let [ydir (nth step 1)
            n 20
            r 0.3
            offset (+ r bead-size)]
        (concat
          (for [i (range n)]
            (jittered (p+ loc (p+ (affine-apply (rot-y (/ (* i 360) (- n 1))) [0 0 (- r)])
                                  [0 (* (/ (- n i) (- n 1)) bead-size ydir) offset]))
                      (conj key i 0)))
          (for [i (range n)]
            (jittered (p+ loc (p+ (affine-apply (rot-y (+ (/ (* i 360) (- n 1)) 180)) [0 0 (- r)])
                                  [0 (* (/ i (- n 1)) bead-size ydir) (- offset)]))
                      (conj key i 1)))))

      ; makeBeadCap: a half circle of beads around the tip.
      (= kind :cap)
      (for [i (range 5)]
        (jittered (p+ loc (affine-apply (rot-y (+ (/ (* i 180) 4) 180)) [0 0 (- bead-size)]))
                  (conj key i 0)))

      :else [])))

; The ornament (if any) hanging from ornament slot i, in branch-local
; coordinates. Slots further out are more likely to be filled.
(defn step-ornament [step loc key z-rotate stages]
  (let [i (nth step 1)]
    (when (< (apply random seed (conj key 0)) (/ (+ 1 i) (+ 2 stages)))
      (let [id (int (* 21 (apply random seed (conj key 1))))]
        (translate [0 -0.65 0]
          (translate loc
            (scale [0.7 0.7 0.7]
              (rotate-z (deg (- z-rotate))
                (ornament-by-id id key)))))))))

; One layer's height (makeTree): layers with more stages sit lower.
(defn layer-height [stages]
  (+ 1 (* (+ (- tree-stages stages) 1) stage-spacing)))

; Everything one branch contributes: its beads as world-space spheres,
; and its ornaments as transformed shapes. makeLayer tilts alternate
; branches up by 5° and 10° and spreads them 36° apart, each layer
; turned a further 15° per stage.
(defn branch [stages j]
  (let [z-rotate (if (= 0 (mod j 2)) 5 10)
        to-world (affine-compose
                   (affine-translation [0 (layer-height stages) 0])
                   (affine-compose (rot-y (+ (* j (/ 360 branches-per-layer)) (* 15 stages)))
                                   (rot-z z-rotate)))
        placed (walk (branch-steps stages))]
    (concat
      (for [s (range (count placed))
            center (let [[step loc] (nth placed s)]
                     (step-beads step loc [stages j s]))]
        (sphere {:center (affine-apply to-world center) :r bead-r}))
      (for [[step loc] placed
            :when (= :ornament (first step))
            :let [o (step-ornament step loc [stages j (nth step 1)] z-rotate stages)]
            :when o]
        (transform to-world o)))))

;; --------------------------------------------------------------------
;; The tree
;; --------------------------------------------------------------------

; All layers' beads and ornaments, in one BVH. The beads take the green
; surface; the ornaments carry their own.
(def tree-body
  (with-surface surface-beads
    (bvh (for [stages (range (+ tree-stages 1))
               j (range branches-per-layer)
               shape (branch stages j)]
           shape))))

(def trunk
  (with-surface surface-trunk
    (group [(cylinder {:p0 [0 0 0] :p1 [0 (* (+ tree-stages 2) stage-spacing) 0] :r trunk-dia})
            (sphere {:center [0 (* (+ tree-stages 2) stage-spacing) 0] :r (* trunk-dia 3)})])))

; The stand: a disk with a stepped rim, a groove near the edge and a
; ring of nine overlapping round grooves, plus a rounded lip. (The POV
; source's `translate <0. 0.5, 0>` for the groove is read as <0, 0.5, 0>.)
(def stand
  (with-surface surface-stand
    (group
      [(difference
         (cylinder {:p0 [0 0 0] :p1 [0 0.5 0] :r 4})
         (difference (cylinder {:p0 [0 0.25 0] :p1 [0 (+ 0.5 eps) 0] :r (+ 4 eps)})
                     (cylinder {:p0 [0 (- 0.25 eps) 0] :p1 [0 (+ 0.5 eps) 0] :r 3.75}))
         (torus {:center [0 0.5 0] :major 3.5 :minor 0.125})
         (group (for [i (range 9)]
                  (rotate-y (deg (* i 40))
                    (torus {:center [2.0 0.5 0] :major 0.9 :minor 0.1})))))
       (torus {:center [0 0.25 0] :major 3.75 :minor 0.25})])))

;; --------------------------------------------------------------------
;; Scene
;; --------------------------------------------------------------------

(def xmastree-scene
  (scene
    {:name          "Christmas Tree"
     ; gAngle 3: location <-14,12,0> + <5,1.2,2>*12, look_at <-14,7,0>,
     ; direction 2*z (zoom 2).
     :camera        (camera-looking-at [46 26.4 24] [-14 7 0] [0 1 0] 2.0)
     :background    pov-white
     :reflect-limit 2
     ; The default 4 samples per pixel leaves the soft shadows of the
     ; thin bead chains grainy: the direction to the area light changes
     ; across its disk, so almost every lit pixel varies a little
     ; between samples, and four samples can agree by chance and stop
     ; early. 16 fixes it.
     :min-samples   16
     :max-samples   64
     :objects
     ; The shared lights (gDetail 4: the spotlight is also an area
     ; light) and ground; see "The xmastree harness" in _pov.lisp.
     (concat (xmas-lights true)
             [tree-body trunk stand xmas-ground])}))
