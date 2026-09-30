; A swatch board for the snowman room's textures (phase 6 of the
; snowman port; see "Snowman port: plan" in CLAUDE.md): the stock
; textures.inc woods sphere2.pov uses, its Wood_Floor (a brick pattern
; choosing between two woods) and Whitewash_Pine, under its sky_sphere.
; Not a POV scene: a check that the textures read sensibly before the
; room is built. Renders at 800x450 (its :size); SIZE overrides.
;
; The room is modelled in inches, so the swatches are too: 10-inch
; boxes on a 60-inch patch of floor. Colours are used as written, as in
; the snowman scenes (assumed_gamma 1.0).
;
; Left to right, back row: DMFWood1, DMFWood2, DMFWood6, EMBWood1.
; Front row: Yellow_Pine, Whitewash_Pine. Under them, Wood_Floor.

(load "_snowman_room_textures.lisp")

; The floor's top is at y = 1.5, as in the room: at y = 0 the surface
; would lie exactly on a plane of Wood_Floor's end joints.
(defn swatch [x z surface]
  (with-surface surface (box [(- x 5) 1.5 (- z 5)] [(+ x 5) 11.5 (+ z 5)])))

(def snowman-room-textures-scene
  (scene
    {:name       "Snowman Room Textures"
     :size       [800 450]
     :camera     (camera-looking-at [0 30 -80] [0 12 5] [0 1 0] 1.2)
     :sky        room-sky
     :objects
     [(light {:location [-40 80 -60] :color [1 1 1]})
      (swatch -27 15 (wood-surface pov-dmf-wood-1))
      (swatch  -9 15 (wood-surface pov-dmf-wood-2))
      (swatch   9 15 (surface (assoc pov-dmf-wood-6-finish :pigment pov-dmf-wood-6)))
      (swatch  27 15 (surface (assoc pov-emb-wood-1-finish :pigment pov-emb-wood-1)))
      (swatch  -9 -5 (wood-surface pov-yellow-pine))
      (swatch   9 -5 (wood-surface whitewash-pine))
      (with-surface (wood-surface wood-floor)
        (box [-30 0 -30] [30 1.5 30]))
      ; The snowy ground outside (sphere2.pov's plane at y = -0.01, in
      ; MatteWhite), so the sky shows only above the horizon.
      (plane {:normal [0 1 0] :p0 [0 -10 0]
              :surface (surface {:color [1 1 1] :ambient 0.1 :light 1.5 :specular 0.2 :shininess 20})})]}))
