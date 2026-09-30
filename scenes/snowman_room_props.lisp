; A contact sheet of the snowman room's props (_snowman_room.lisp),
; each scaled to fit a cell of a 4 x 3 grid, to check their shapes and
; POV's transform order before the room is built (phase 6d of the
; snowman port). Not a POV scene. The camera looks down -x, so props
; that face +x in the room (the clock, the outlet, the windows) face it.
; The windows, frame and blinds are turned 30 degrees to show their
; depth, and the mirror tipped to show its glass. Renders at 1200x900
; (its :size); SIZE overrides.
;
; Top row: RoundedBox, OneAxisRoundedBox, the IKEA desk, the mirror.
; Middle row: Quarter_Round and Corner_Round, ModernCross, ModernClock
; (lit, with glass), WallOutlet.
; Bottom row: WindowGlass (with glass), WindowFrame, WindowBlindSlats,
; Axis.

(load "_snowman_room.lisp")

; Cell (col, row): its centre in the z-y plane. Columns run along z.
(defn cell [col row shape]
  (at [[:translate [0 (- 60 (* row 30)) (* (- col 1.5) 30)]]] shape))

(def props
  [; Top row.
   (cell 0 0 (with-surface matte-red
               (at [[:translate [-5 -2 -4]] [:scale [2 2 2]]] (rounded-box 10 4 8 1))))
   (cell 1 0 (with-surface matte-black
               (at [[:scale [1.5 1.5 1.5]]] (one-axis-rounded-box 12 1 9 1.5))))
   (cell 2 0 (at [[:translate [-30 -18 -15]] [:scale [0.4 0.4 0.4]] [:rotate [0 30 0]]]
               (ikea-desk 60 36 30
                          (wood-surface (pigment-at [[:scale [2 2 2]] [:rotate [0 90 0]]] pov-emb-wood-1))
                          matte-black)))
   (cell 3 0 (at [[:translate [-0.5 0 -0.5]] [:scale [16 3 16]] [:rotate [0 -45 0]] [:rotate [0 0 35]]]
               room-mirror))
   ; Middle row.
   (cell 0 1 (with-surface (wood-surface whitewash-pine)
               (group [(at [[:scale [8 8 8]] [:translate [0 -4 -10]]] quarter-round)
                       (at [[:scale [8 8 8]] [:translate [0 -4 2]]] corner-round)])))
   (cell 1 1 (at [[:scale [1 20 20]] [:translate [0 -10 -10]]] modern-cross))
   (cell 2 1 (at [[:scale [2.5 2.5 2.5]]] (modern-clock 11 45 true true)))
   (cell 3 1 (with-surface matte-white (at [[:scale [4 4 4]]] wall-outlet)))
   ; Bottom row.
   (cell 0 2 (at [[:translate [0 -18 -24]] [:scale [0.5 0.5 0.5]] [:rotate [0 30 0]]]
               (window-glass 0.5 36 48 4 5 true)))
   (cell 1 2 (at [[:translate [0 -36 -24]] [:scale [0.35 0.35 0.35]] [:rotate [0 30 0]]]
               (window-frame 72 48)))
   (cell 2 2 (with-surface (wood-surface whitewash-pine)
               (at [[:translate [0 -36 -25]] [:scale [0.35 0.35 0.35]] [:rotate [0 30 0]]]
                 (window-blind-slats 72 1 0.1 50 15 5 3))))
   (cell 3 2 (at [[:rotate [0 -30 0]] [:rotate [0 0 10]] [:scale [1.8 1.8 1.8]]] (axis 6)))])

(def snowman-room-props-scene
  (scene
    {:name       "Snowman Room Props"
     :size       [1200 900]
     :camera     (camera-looking-at [200 45 0] [0 30 0] [0 1 0] 2.3)
     :background [0.55 0.6 0.65]
     :objects
     (concat [(light {:location [300 200 150] :color [0.9 0.9 0.9]})
              (light {:location [300 50 -200] :color [0.4 0.4 0.4]})]
             ; In a BVH, so a ray only tests the props it passes near.
             [(bvh props)])}))
