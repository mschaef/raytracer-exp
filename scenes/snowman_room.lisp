; The snowman's room, ported from snowman/snowman_avatar/sphere2.pov
; in the POV-Ray projects (github.com/mschaef/povray-projects): a
; 12-foot room (modelled in inches) with a wood floor and molding, two
; windows with blinds on the -x wall, a clock, a cross and two outlets
; on the walls, and an IKEA desk in the corner with a mat, a mirror and
; the snowman on it. Snow and a night sky show through the windows.
; Renders at 800x800 (its :size; sphere2.pov's frame is square); SIZE
; overrides.
;
; sphere2.pov's DO_LIGHTS, DO_AREA_LIGHTING and DO_GLASS were quality
; switches for modelling; the finished render has them all on, and so
; does this port by default (`room-lights?`, `room-area?`, `room-glass?`
; below). With the lights off the room is as sphere2.pov renders by
; default: POV's ambient light at 1 and the clock's red light.
; DO_HEIGHT_FIELD's yard isn't ported (it names yard.tga, which isn't
; in the repo; there's a yard.png).
;
; Kept as written: the bowtie isn't moved with the snowman (it floats,
; a tenth of an inch across, near the middle of the room), and the
; Wood_Floor scale is commented out, so the boards are 3 inches wide
; and 24 long.

(load "_snowman_room.lisp")

; The modelling switches.
(def room-lights? true)
(def room-area?   true)
(def room-glass?  true)

;; --------------------------------------------------------------------
;; The room
;; --------------------------------------------------------------------

(def light-tan [0.85 0.7 0.6])

; The walls: LightTan, MatteFinish, `normal { wrinkles 0.1 scale 0.03 }`.
(def wall-surface
  (surface (assoc matte-finish
                  :color light-tan
                  :normal {:pattern :wrinkles :amount 0.1 :transform (affine-scale [0.03 0.03 0.03])})))

; A 144-inch box less a 136-inch interior (4-inch walls, a 1-inch floor
; slab) and two window openings in the -x wall; the wood floor laid
; 0.5 thick on the slab.
(def room-shell
  (group [(with-surface wall-surface
            (difference (box [-72 0 -72] [72 108 72])
                        (box [-68 1 -68] [68 107 68])
                        (box [-66 24 -54] [-74 96 -6])
                        (box [-66 24 6] [-74 96 54])))
          (with-surface (wood-surface wood-floor)
            (box [-68 1 -68] [68 1.5 68]))]))

; Quarter-round molding along three walls (not the window wall).
(def molding
  (with-surface (wood-surface whitewash-pine)
    (group [(at [[:scale [1 1 136]] [:translate [-68 1.5 -68]]] quarter-round)
            (at [[:rotate [0 -90 0]] [:scale [136 1 1]] [:translate [66 1.5 -68]]] quarter-round)
            (at [[:rotate [0 90 0]] [:scale [136 1 1]] [:translate [-68 1.5 68]]] quarter-round)])))

; A window: the lower and upper sashes (the lower one set further out),
; the frame, and the blinds just inside.
(defn window [z blind-width]
  (group [(at [[:translate [(+ -72 1.375) 60 z]]] (window-glass 0.5 36 48 4 5 room-glass?))
          (at [[:translate [(+ -72 3) 24 z]]] (window-glass 0.5 36 48 4 5 room-glass?))
          (at [[:translate [-72 24 z]]] (window-frame 72 48))
          (with-surface (wood-surface whitewash-pine)
            (at [[:translate [-67 24 (- z 1)]]]
              (window-blind-slats 72 1 0.1 blind-width 15 5 3)))]))

;; --------------------------------------------------------------------
;; The furniture
;; --------------------------------------------------------------------

(def desk
  (at [[:translate [-57 0 -66]]]
    (ikea-desk 60 36 30
               (surface (assoc pov-emb-wood-1-finish
                               :pigment (pigment-at [[:scale [2 2 2]] [:rotate [0 90 0]]] pov-emb-wood-1)))
               matte-black)))

(def desk-mat
  (with-surface matte-black
    (at [[:rotate [0 -15 0]] [:translate [-24 36 -48]]] (one-axis-rounded-box 24 0.1 18 0.1))))

(def furniture
  [desk
   desk-mat
   (at [[:scale [10 0.2 10]] [:rotate [0 -45 0]] [:translate [-44 36 -50]]] room-mirror)
   (at [[:translate [-44 36 -48]]] (snowman false))
   (placed-bowtie metallic-red)
   (with-surface matte-white (at [[:rotate [0 90 0]] [:translate [-36 12 -67.75]]] wall-outlet))
   (with-surface matte-white (at [[:translate [-68 12 0]]] wall-outlet))
   (at [[:scale [1 12 12]] [:translate [-68 54 -4]]] modern-cross)
   (at [[:scale [1 1.2 1.2]] [:rotate [0 -90 0]] [:translate [-24 60 -64]]]
     (modern-clock 11 45 true room-glass?))
   (window -54 50)
   (window 6 52)
   (at [[:translate [0 48 0]]] (axis 6))])

;; --------------------------------------------------------------------
;; Lights
;; --------------------------------------------------------------------

; With DO_LIGHTS: no ambient light, a blue moon (an area light 200
; across), a Gray30 light near the ceiling at the front, and a white
; spotlight overhead aimed down (radius 6, falloff 80), an area light
; 6 across with DO_AREA_LIGHTING.
(def room-light-list
  (if room-lights?
    [(light {:location [-2000 2000 0] :color [0.1 0.1 0.5]
             :area-u [200 0 0] :area-v [0 200 0]})
     (light {:location [0 106 -60] :color pov-gray30})
     (let [spot {:location    [0 95 0]
                 :color       [1 1 1]
                 :point-at    [0 0 -5]
                 :inner-angle (deg->rad 6)
                 :outer-angle (deg->rad 80)}]
       (light (if room-area? (assoc (assoc spot :area-u [6 0 0]) :area-v [0 6 0]) spot)))]
    []))

;; --------------------------------------------------------------------
;; The scene
;; --------------------------------------------------------------------

(def snowman-room-scene
  (scene
    {:name          "Snowman Room"
     :size          [800 800]
     ; direction 1.5*z (zoom 1.5), up y, right x: a square frame.
     :camera        (camera-looking-at [26 62 36] [-56 36 -36] [0 1 0] 1.5)
     :sky           room-sky
     :ambient-light (if room-lights? 0 1)
     :objects
     (concat room-light-list
             [; The snowy ground outside.
              (plane {:normal [0 1 0] :p0 [0 -0.01 0] :surface matte-white})
              (bvh (concat [room-shell molding] furniture))])}))
