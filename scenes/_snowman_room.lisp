; The props of the snowman room: sphere2.pov's include files
; (higherorder.inc, furniture.inc, window.inc and the parts of
; utilities.inc it uses) as SDL. Phase 6d of the snowman port; see
; "Snowman port: plan" in CLAUDE.md. snowman_room_props.lisp shows each
; one on its own.
;
; Each POV macro is a function of the same arguments, returning an
; unplaced shape. POV's object-level transforms, applied in the order
; written, are `(at [[:rotate ...] [:translate ...]] shape)`. The room
; is modelled in inches. Colours are used as written (assumed_gamma
; 1.0).
;
; `glass?` arguments stand for sphere2.pov's DO_GLASS switch: with it
; off, POV left the window panes and the clock's glass out.

(load "_snowman.lisp")
(load "_snowman_room_textures.lisp")

;; --------------------------------------------------------------------
;; Helpers
;; --------------------------------------------------------------------

; POV's object transforms, in the order written.
(defn at [steps shape] (transform (pov-transform steps) shape))

; A pigment (one map or layers) with POV transforms applied after its
; own, as in `texture { EMBWood1 scale 2 rotate <0, 90, 0> }`. A solid
; :color layer takes no transform (it's the same everywhere).
(defn pigment-at [steps pigment]
  (let [t (pov-transform steps)
        move (fn [layer]
               (if (get layer :color)
                 layer
                 (assoc layer :transform
                        (affine-compose t (if (get layer :transform) (get layer :transform) (affine-identity))))))]
    (if (map? pigment) (move pigment) (map move pigment))))

; POV's default texture: black, ambient 0.1, diffuse 0.6. Parts with no
; texture of their own and none inherited get it.
(def pov-default-surface (surface {:color [0 0 0] :ambient 0.1 :light 0.6}))

; A black pigment with POV's default finish: the socket holes' `pigment
; { Black }`.
(def pov-black-surface pov-default-surface)

;; --------------------------------------------------------------------
;; utilities.inc
;; --------------------------------------------------------------------

(def matte-red   (snow-matte pov-red))
(def matte-green (snow-matte pov-green))
(def matte-blue  (snow-matte pov-blue))

; FlatGlass: rgbf <1, 1, 1, 0.7>, specular 1, roughness 0.001, ambient
; 0, diffuse 0, reflection 0.04, ior 1.5. The only glass in these scenes
; that refracts: POV 3.7's stock Glass2 and Glass3 (the window panes and
; mirrors) have no ior (history entry 89).
(def flat-glass
  (surface {:color [1 1 1] :ambient 0.0 :light 0.0 :specular 1.0 :shininess 1000
            :reflection 0.04 :filter 0.7 :ior 1.5}))

; Arrow(len, shaft radius, head radius, texture): a shaft along x from
; -0.8 len to 0.8 len, a cone head out to len, and a flared tail cone
; back to -len.
(defn arrow [len cyl-r head-r surface]
  (let [body (* len 0.8)
        head (* len 0.2)]
    (with-surface surface
      (group [(cylinder {:p0 [(- body) 0 0] :p1 [body 0 0] :r cyl-r})
              (pov-cone [body 0 0] head-r [(+ body head) 0 0] 0)
              (pov-cone [(- body) 0 0] cyl-r [(- (- body) head) 0 0] head-r)]))))

; Axis(size): a black ball with red, green and blue double-ended arrows
; along x, y and z.
(defn axis [size]
  (let [ball (/ size 10)
        cyl-r (/ ball 2)]
    (group [(sphere {:center [0 0 0] :r ball :surface matte-black})
            (arrow size cyl-r ball matte-red)
            (at [[:rotate [0 0 90]]] (arrow size cyl-r ball matte-green))
            (at [[:rotate [0 -90 0]]] (arrow size cyl-r ball matte-blue))])))

;; --------------------------------------------------------------------
;; higherorder.inc
;; --------------------------------------------------------------------

; RoundedBox(xs, ys, zs, cr): a box from the origin to <xs, ys, zs>
; with edges and corners rounded to radius cr: the box less its edge
; regions, plus a sphere at each corner and a cylinder along each edge.
; The macro lists one edge cylinder twice and leaves out the edge from
; <cr, cr, cr> to <xs - cr, cr, cr>; that's kept (the bottom front edge
; stays square).
(defn rounded-box [xs ys zs cr]
  (let [a cr
        bx (- xs cr) by (- ys cr) bz (- zs cr)
        edge (fn [p0 p1] (cylinder {:p0 p0 :p1 p1 :r cr}))]
    (group
      (concat
        [(difference
           (box [0 0 0] [xs ys zs])
           (difference
             (box [-0.1 -0.1 -0.1] [(+ xs 0.1) (+ ys 0.1) (+ zs 0.1)])
             (box [cr -0.2 cr] [bx (+ ys 0.2) bz])
             (box [-0.2 cr cr] [(+ xs 0.2) by bz])
             (box [cr cr -0.2] [bx by (+ zs 0.2)])))]
        (for [cx [a bx] cy [a by] cz [a bz]]
          (sphere {:center [cx cy cz] :r cr}))
        [(edge [bx by a] [bx by bz])
         (edge [bx a bz] [bx by bz])
         (edge [bx a a] [bx by a])
         (edge [bx a a] [bx a bz])
         (edge [a by a] [a by bz])
         (edge [a a bz] [a by bz])
         (edge [a a a] [a by a])
         (edge [a a a] [a a bz])
         (edge [a by bz] [bx by bz])
         (edge [a a bz] [bx a bz])
         (edge [a by a] [bx by a])
         (edge [a by bz] [bx by bz])]))))

; OneAxisRoundedBox(x, y, z, corner_radius): a slab centred on the y
; axis, x by z and y tall, with its four vertical edges rounded.
(defn one-axis-rounded-box [xd yd zd cr]
  (let [xh (/ xd 2) zh (/ zd 2) e 0.01]
    (group
      [(difference
         (box [(- xh) 0 (- zh)] [xh yd zh])
         (box [(- (- xh) e) (- e) (- (- zh) e)] [(+ (- xh) cr) (+ yd e) (+ (- zh) cr)])
         (box [(+ xh e) (- e) (+ zh e)] [(- xh cr) (+ yd e) (- zh cr)])
         (box [(+ xh e) (- e) (- (- zh) e)] [(- xh cr) (+ yd e) (+ (- zh) cr)])
         (box [(- (- xh) e) (- e) (+ zh e)] [(+ (- xh) cr) (+ yd e) (- zh cr)]))
       (cylinder {:p0 [(- xh cr) 0 (- zh cr)] :p1 [(- xh cr) yd (- zh cr)] :r cr})
       (cylinder {:p0 [(+ (- xh) cr) 0 (- zh cr)] :p1 [(+ (- xh) cr) yd (- zh cr)] :r cr})
       (cylinder {:p0 [(- xh cr) 0 (+ (- zh) cr)] :p1 [(- xh cr) yd (+ (- zh) cr)] :r cr})
       (cylinder {:p0 [(+ (- xh) cr) 0 (+ (- zh) cr)] :p1 [(+ (- xh) cr) yd (+ (- zh) cr)] :r cr})])))

;; --------------------------------------------------------------------
;; furniture.inc
;; --------------------------------------------------------------------

; mirror(): a unit cube of glass with a silver mirror in its bottom
; 0.0001. The glass is `pigment { rgbf <0, 0, 0.1, 0.9> } texture {
; Glass2 }`: clear Glass2 (reflection 0.5, phong 0.3) layered over a
; dark blue filter, as in avatar.pov with Glass3. One surface holds
; one layer, so this keeps Glass2's finish and the dark blue filter
; (there's no reference render to match it to, unlike the avatar's).
(def room-mirror-glass
  (surface {:color [0 0 0.1] :ambient 0.0 :light 0.0 :specular 0.3 :shininess 60
            :reflection 0.5 :filter 0.9}))

(def room-mirror-silver
  (surface {:color pov-silver :ambient 0.0 :light 0.1 :specular 0.9 :shininess 120
            :reflection 0.9 :metallic true}))

(def room-mirror
  (group [(with-surface room-mirror-glass (box [0 0 0] [1 1 1]))
          (with-surface room-mirror-silver
            (intersection (plane {:normal [0 1 0] :p0 [0 0.0001 0]})
                          (box [0 0 0] [1 1 1])))]))

; IKEADesk(x, y, z, desk texture, legs texture): a rounded top 1 thick
; at height y, on four legs of radius 1 set 2 in from the corners.
(defn ikea-desk [xd yd zd desk-surface legs-surface]
  (let [leg (fn [x z] (cylinder {:p0 [x 0 z] :p1 [x (- yd 1) z] :r 1}))]
    (group [(with-surface desk-surface (at [[:translate [0 (- yd 1) 0]]] (rounded-box xd 1 zd 0.3)))
            (with-surface legs-surface
              (group [(leg 2 2) (leg (- xd 2) 2) (leg 2 (- zd 2)) (leg (- xd 2) (- zd 2))]))])))

; Quarter_Round: the quarter of a unit cylinder along z inside the unit
; cube, a molding profile.
(def quarter-round
  (intersection (box [0 0 0] [1 1 1])
                (cylinder {:p0 [0 0 -0.1] :p1 [0 0 1.1] :r 1})))

; Corner_Round: two quarter rounds crossed, for an outside corner.
(def corner-round
  (at [[:translate [0 0.5 0.5]]]
    (intersection (at [[:translate [0 -0.5 -0.5]]] quarter-round)
                  (at [[:translate [0 -0.5 -0.5]] [:rotate [90 0 0]]] quarter-round))))

; InflatableCross(x, y, z, inflation): an upright (the first box, x by
; y by z/3) and a crossbar (between y/2 and 3y/4, from -z/3 to 2z/3),
; each grown by the inflation.
(defn inflatable-cross [xd yd zd xi yi zi]
  (let [i [xi yi zi]]
    (group [(box (p- [0 0 0] i) (p+ [xd yd (/ zd 3)] i))
            (box (p- [0 (* 2 (/ yd 4)) (- (/ zd 3))] i)
                 (p+ [xd (* 3 (/ yd 4)) (* 2 (/ zd 3))] i))])))

; ModernCross: a DMFWood1 cross frame (the cross less a thinner, deeper
; one) with a DMFWood6 inlay down its middle, then scaled to a unit
; height (and 3/4 wide) and 2 deep in x.
(def modern-cross
  (at [[:translate [0 0 1]] [:scale [(/ 1 0.5) (/ 1 4) (/ 1 4)]]]
    (group [(with-surface (wood-surface pov-dmf-wood-1)
              (difference (inflatable-cross 0.5 4 3 0 0 0)
                          (inflatable-cross 0.5 4 3 0.2 -0.1 -0.1)))
            (with-surface (surface (assoc pov-dmf-wood-6-finish :pigment pov-dmf-wood-6))
              (intersection (box [0.125 0 -1] [0.375 4 2])
                            (inflatable-cross 0.5 4 3 0.2 -0.1 -0.1)))])))

; ModernClock(h, m, internal light, glass?): a black drum of radius 4
; facing +x with a white dial, cone hands, twelve ticks (wider at 12,
; 3, 6 and 9), and, when lit, a red light inside the face.
(defn modern-clock [h m light? glass?]
  (let [tick (fn [i]
               (let [w (cond (= i 0) 0.25
                             (= (mod i 3) 0) 0.125
                             :else 0.0675)]
                 (at [[:rotate [(* i 30) 0 0]]]
                   (box [0.1 3.0 (- w)] [0.14 3.8 w]))))]
    (group
      (concat
        [(with-surface matte-black
           (difference (cylinder {:p0 [0 0 0] :p1 [0.75 0 0] :r 4})
                       (cylinder {:p0 [0.1 0 0] :p1 [0.76 0 0] :r 3.9})))
         ; The hub has no texture: POV's default (black).
         (with-surface pov-default-surface
           (cylinder {:p0 [0.45 0 0] :p1 [0.55 0 0] :r 0.5}))
         (with-surface matte-white
           (cylinder {:p0 [0.1 0 0] :p1 [0.11 0 0] :r 3.9}))
         ; Minute and hour hands.
         (with-surface matte-black
           (group [(at [[:scale [0.2 1 0.4]] [:translate [0.1 0 0]] [:rotate [(* m 6) 0 0]]]
                     (pov-cone [0 0 0] 0.5 [0 3.5 0] 0))
                   (at [[:scale [0.1 1 0.4]] [:translate [0.18 0 0]] [:rotate [(* h 30) 0 0]]]
                     (pov-cone [0 0 0] 0.5 [0 2.5 0] 0))]))
         (with-surface matte-black (group (map tick (range 12))))]
        (if glass?
          [(with-surface flat-glass (cylinder {:p0 [0.45 0 0] :p1 [0.5 0 0] :r 3.9}))]
          [])
        (if light?
          [(light {:location [0.42 0 0] :color [1 0 0]})]
          [])))))

; StandardRoundedHead: a round-headed screw pointing -x (a half
; ellipsoid head with a slot, a countersink cone and a shank).
(def standard-rounded-head
  (group [(difference (scale [0.5 1 1] (sphere {:center [0 0 0] :r 1}))
                      (box [0 -1 -1] [-1 1 1])
                      (box [1 -1 -0.1] [0.1 1 0.1]))
          (pov-cone [0 0 0] 1 [-0.75 0 0] 0)
          (cylinder {:p0 [0 0 0] :p1 [-1.5 0 0] :r 0.5})]))

; BlankFaceplate: a plate 4 tall and 2.25 wide, its face at x = 0.25,
; with quarter-round edges and corner-round corners.
(def blank-faceplate
  (group [(box [0.125 -2 -1.125] [0.25 2 1.125])
          (at [[:scale [0.25 0.25 2.25]] [:translate [0 2 -1.125]]] quarter-round)
          (at [[:scale [0.25 -0.25 2.25]] [:translate [0 -2 -1.125]]] quarter-round)
          (at [[:scale [0.25 0.25 4]] [:rotate [90 0 0]] [:translate [0 2 1.125]]] quarter-round)
          (at [[:scale [0.25 0.25 4]] [:rotate [-90 0 0]] [:translate [0 -2 -1.125]]] quarter-round)
          (at [[:scale [0.25 0.25 0.25]] [:translate [0 2 1.125]]] corner-round)
          (at [[:rotate [-90 0 0]] [:scale [0.25 0.25 0.25]] [:translate [0 2 -1.125]]] corner-round)
          (at [[:scale [0.25 -0.25 0.25]] [:translate [0 -2 1.125]]] corner-round)
          (at [[:rotate [-90 0 0]] [:scale [0.25 -0.25 0.25]] [:translate [0 -2 -1.125]]] corner-round)]))

(defn faceplate-screw [y] (at [[:scale [0.125 0.125 0.125]] [:translate [0.25 y 0]]] standard-rounded-head))
(def faceplate-top-screw (faceplate-screw 1.65))
(def faceplate-center-screw (faceplate-screw 0))
(def faceplate-bottom-screw (faceplate-screw -1.65))

(def one-screw-faceplate-base
  (difference blank-faceplate (at [[:translate [0.00001 0 0]]] faceplate-center-screw)))

(def faceplate-socket-cutout
  (intersection (box [-1 -0.5 -0.875] [0.251 0.5 0.875])
                (cylinder {:p0 [-1 0 0] :p1 [0.3 0 0] :r 0.75})))

(def standard-faceplate
  (difference one-screw-faceplate-base
              (at [[:scale [1 1.1 1.1]] [:translate [0 0.875 0]]] faceplate-socket-cutout)
              (at [[:scale [1 1.1 1.1]] [:translate [0 -0.875 0]]] faceplate-socket-cutout)))

; A socket: the cutout shape less two blade slots and a ground hole,
; each cut in black.
(def faceplate-socket
  (difference faceplate-socket-cutout
              (with-surface pov-black-surface
                (group [(box [0.252 0 -0.3] [0 0.3 -0.2])
                        (box [0.252 0 0.2] [0 0.3 0.3])
                        (box [0.252 -0.1 -0.125] [0 -0.3 0.125])
                        (cylinder {:p0 [0.252 -0.3 0] :p1 [0 -0.3 0] :r 0.125})]))))

; WallOutlet: the faceplate, its screw and two sockets, facing +x.
; Unsurfaced but for the black slots: sphere2.pov gives it MatteWhite.
(def wall-outlet
  (group [standard-faceplate
          faceplate-center-screw
          (at [[:scale [1 1.05 1.05]] [:translate [0.08 0.875 0]]] faceplate-socket)
          (at [[:scale [1 1.05 1.05]] [:translate [0.08 -0.875 0]]] faceplate-socket)]))

;; --------------------------------------------------------------------
;; window.inc
;; --------------------------------------------------------------------

; WindowGlass(x, y, z, y panes, z panes): a white sash x thick, y tall
; and z wide, with a 1-inch border and muntins between the panes, and
; (with glass) a Glass3 pane a quarter of the way in.
(defn window-glass [xd yd zd y-panes z-panes glass?]
  (let [ysp (/ (- yd 2) y-panes)
        zsp (/ (- zd 2) z-panes)
        half (/ xd 2)]
    (group
      (concat
        [(with-surface matte-white
           (group
             (concat
               [(difference (box [0 0 0] [xd yd zd])
                            (box [-0.1 1 1] [(+ xd 0.1) (- yd 1) (- zd 1)]))]
               (for [s (range 1 y-panes)]
                 (box [0 (+ (- (* s ysp) half) 1) 0] [xd (+ (* s ysp) half 1) zd]))
               (for [s (range 1 z-panes)]
                 (box [0 0 (+ (- (* s zsp) half) 1)] [xd yd (+ (* s zsp) half 1)])))))]
        (if glass?
          [(with-surface (surface pov-glass-3)
             (box [(/ xd 4) 1 1] [(* 2 (/ xd 4)) (- yd 1) (- zd 1)]))]
          [])))))

; WindowFrame(y, z): the casing, a sill 5 deep with a rounded nose, and
; two channels for the sashes, in Whitewash_Pine.
(defn window-frame [yd zd]
  (with-surface (wood-surface whitewash-pine)
    (difference
      (group [(box [0 0 0] [5 0.5 zd])
              (box [0 yd 0] [4 0 0.5])
              (box [0 yd zd] [4 0 (- zd 0.5)])
              (box [0 yd 0] [4 (- yd 0.5) zd])
              (cylinder {:p0 [5 0.25 0] :p1 [5 0.25 zd] :r 0.25})])
      (box [0.5 0.25 0.25] [1.75 (- yd 0.25) (- zd 0.25)])
      (box [2.25 0.25 0.25] [3.5 (- yd 0.25) (- zd 0.25)]))))

; ZAxisRoundedSlat(x, y, z): a slat x wide and y thick with rounded
; long edges, running z along the z axis.
(defn z-axis-rounded-slat [xd yd zd]
  (let [r (/ yd 2)]
    (group [(box [r 0 0] [(- xd r) yd zd])
            (cylinder {:p0 [r r 0] :p1 [r r zd] :r r})
            (cylinder {:p0 [(- xd r) r 0] :p1 [(- xd r) r zd] :r r})])))

; WindowBlindSlat(x, y, z, cord offset, cords): a slat with a notch for
; each lift cord, spaced evenly from `cord offset` in from each end.
(defn window-blind-slat [xd yd zd cord-ofs cords]
  (let [r (/ yd 2)
        cord-min cord-ofs
        cord-max (- zd cord-ofs)
        spacing (/ (- cord-min cord-max) (- cords 1))]
    (apply difference
           (concat [(z-axis-rounded-slat xd yd zd)]
                 (for [i (range cords)]
                   (at [[:rotate [90 0 0]]
                        [:translate [(* r 1.5) (/ zd 2) (- (+ (- (* i spacing)) cord-min) (/ yd 2))]]]
                     (z-axis-rounded-slat (- xd (* r 3)) yd zd)))))))

; WindowBlindSlats(y, slat width, slat thickness, z, angle, cord offset,
; cords): y / width slats stacked up y, each tilted by `angle` about z.
; Unsurfaced; sphere2.pov gives them Whitewash_Pine.
(defn window-blind-slats [yd slat-w slat-t zd angle cord-ofs cords]
  (let [slat (window-blind-slat slat-w slat-t zd cord-ofs cords)]
    (bvh (for [i (range (int (ceil (/ yd slat-w))))]
           (at [[:rotate [0 0 angle]] [:translate [0 (* i slat-w) 0]]] slat)))))
