; Three wooden blocks on a checkerboard, seen from straight above,
; ported from magic/nba.pov in the POV-Ray projects
; (github.com/mschaef/povray-projects). Left to right: T_Wood23 (pale
; pine), T_Wood7 under a nearly clear pink layer, and T_Wood28
; (orange). Render at 4:3, e.g. SIZE=640x480.
;
; The woods are woods.inc's two-layer textures (see _pov.lisp). In
; nba.pov the middle block's `pigment { rgbt <1, 0.7, 0.7, 0.9> }`
; replaces T_Wood7's top layer (POV applies a pigment written after a
; layered texture to its top layer), leaving a 90%-clear pink over the
; bottom grain.
;
; Stand-in: the white sphere of radius 5000 around everything is left
; out. It only shows where nothing else is, and the background is
; black there anyway (its inside faces a camera it encloses).

(load "_pov.lisp")

; box { <-1, -0.5, -2>, <1, 0.5, 2> }: long along z.
(defn block [x pigment]
  (with-surface (pov-pigmented pigment)
    (translate [x 0 0] (box [-1 -0.5 -2] [1 0.5 2]))))

(def pink-over-wood7
  [(first pov-t-wood7) {:color [1.0 0.7 0.7 0.9]}])

(def nba-scene
  (scene
    {:name       "NBA"
     ; POV's look_at straight down along the default up (y) falls back
     ; to right = +x, up = +z.
     :camera     (camera-looking-at [0 8 0] [0 0 0] [0 0 1] 1.0)
     :background pov-black
     :objects
     [(light-white [4 5 4])
      (light-white [4 5 -4])
      (light-white [-4 5 4])
      (light-white [-4 5 -4])
      (block -3 pov-t-wood23)
      (block 0 pink-over-wood7)
      (block 3 pov-t-wood28)
      (plane {:normal [0 1 0] :p0 [0 -2 0]
              :surface (pov-pigmented {:pattern   :checker
                                       :colors    [pov-white pov-black]
                                       :transform (affine-scale [2 2 2])})})]}))
