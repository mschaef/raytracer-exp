; The snowman room's textures (sphere2.pov, utilities.inc), built from
; the textures.inc pigments in _pov.lisp. Colours are used as written:
; the snowman scenes set assumed_gamma 1.0.

(load "_pov.lisp")

; A wood pigment (one map or layers) with POV's default finish
; (ambient 0.1, diffuse 0.6), for the textures that have no finish of
; their own.
(defn wood-surface [pigment]
  (surface {:pigment pigment :ambient 0.1 :light 0.6}))

; Whitewash_Pine: Yellow_Pine under a white layer, `color <1, 1, 1, 0.2>`
; (a filter of 0.2). Through white a filter passes the layer below
; unchanged, the same as a transmit, so it's a transmit here.
(def whitewash-pine (conj pov-yellow-pine {:color [1.0 1.0 1.0 0.2]}))

; Wood_Floor: `brick texture { DMFWood2 } texture { Yellow_Pine }
; mortar 0.125 brick_size <8, 3, 24> rotate <0, 0, 90>`. POV's brick
; takes the mortar texture first, so the joints are DMFWood2 and the
; boards Yellow_Pine. The rotation stands the bricks on end: boards 3
; wide and 24 long across the floor, each course 8 deep.
(def wood-floor
  {:pattern    :brick
   :brick-size [8 3 24]
   :mortar     0.125
   :pigments   [pov-dmf-wood-2 pov-yellow-pine]
   :transform  (pov-transform [[:rotate [0 0 90]]])})

; sphere2.pov's sky_sphere: `gradient y` with Yellow up to 0.499, a hard
; step to <0, 0, 0.1> at 0.5, fading to black by 0.6; `scale 2
; translate -1` puts 0.5 at the horizon. The snowy ground plane hides
; the yellow lower half.
(def room-sky
  {:pattern   :gradient
   :color-map [[0.499 [1 1 0]] [0.5 [0 0 0.1]] [0.6 [0 0 0]]]
   :transform (pov-transform [[:scale [2 2 2]] [:translate [-1 -1 -1]]])})
