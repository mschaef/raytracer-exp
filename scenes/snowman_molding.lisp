; A fence of clipped boards, ported from
; snowman/snowman_avatar/moldingtest.pov in the POV-Ray projects
; (github.com/mschaef/povray-projects): a test piece from the snowman
; project, eight boards 12 tall in Yellow_Pine with their tops cut to a
; point, and utilities.inc's axis at the origin, lit by two grey lights
; and a grey spotlight. Renders at 600x600 (its :size; the POV camera's
; frame is square); SIZE overrides.

(load "_snowman_room.lisp")

; FenceBoard(x, y, z, clip width, clip angle): a board x thick, y tall
; and z wide (centred on z = 0), its top corners cut off by two boxes
; turned clip_angle about x and set clip_width either side of the
; centre line.
(defn fence-board [xd yd zd clip-w clip-angle]
  (difference (box [0 0 (- (/ zd 2))] [xd yd (/ zd 2)])
              (at [[:rotate [clip-angle 0 0]] [:translate [0 yd (- clip-w)]]]
                (box [-0.1 0 0] [(+ xd 0.1) -8 -8]))
              (at [[:rotate [clip-angle 0 0]] [:translate [0 yd clip-w]]]
                (box [-0.1 0 0] [(+ xd 0.1) 8 8]))))

; FenceLine(length, y): boards 1 x y x 6, clipped 1.5 either side at
; 45 degrees, every 7 along z while under `length`.
(defn fence-line [len yd]
  (group (for [i (range (int (ceil (/ len 7))))]
           (at [[:translate [0 0 (* i 7)]]] (fence-board 1 yd 6 1.5 45)))))

(def snowman-molding-scene
  (scene
    {:name          "Snowman Molding Test"
     :size          [600 600]
     ; location 50 (<50, 50, 50>), look_at 0, direction 1.5*z, a square
     ; frame.
     :camera        (camera-looking-at [50 50 50] [0 0 0] [0 1 0] 1.5)
     :background    [0 0 0]
     ; global_settings { ambient_light 0 }.
     :ambient-light 0
     :objects
     [(light {:location [50 30 0] :color pov-gray30})
      (light {:location [50 60 0] :color pov-gray30})
      ; The subject spotlight: Gray50 at <20, 3, 3> aimed at the origin,
      ; radius 6, falloff 80.
      (light {:location [20 3 3] :color [0.5 0.5 0.5] :point-at [0 0 0]
              :inner-angle (deg->rad 6) :outer-angle (deg->rad 80)})
      (axis 4)
      (with-surface (wood-surface (pigment-at [[:rotate [90 0 0]]] pov-yellow-pine))
        (fence-line 50 12))]}))
