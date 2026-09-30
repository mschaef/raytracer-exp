; The snowman on a snowy height field, ported from
; snowman/snowman_avatar/sphere.pov in the POV-Ray projects
; (github.com/mschaef/povray-projects): avatar.pov's snowman, bowtie,
; mirror and compass seen from further off, standing in a drift of snow
; made from the height field imap.tga (in models/, as
; snowman_imap.tga). Renders at 640x480 (its :size); SIZE overrides.
;
; There's no reference render for this one. It shares avatar.pov's
; lights and materials, so it shares its view: the plain clip, as POV
; displayed it.

(load "_snowman.lisp")

; height_field { tga "imap.tga" water_level 0.25 scale <10, 3, 10>
; translate <-5, -0.25, -5> } in DirtySnowWhite, not smoothed. The
; image is grey, brightest about 170 of 255, so the drifts rise to
; about 1.75; the square hole in the image sits under the compass.
(def snow-drift
  (translate [-5 -0.25 -5]
    (scale [10 3 10]
      (with-surface dirty-snow-white
        (height-field {:image "../models/snowman_imap.tga" :water-level 0.25})))))

(def snowman-sphere-scene
  (scene
    {:name       "Snowman Sphere"
     :size       [640 480]
     ; direction 1.5*z (zoom 1.5), POV's default 4:3 frame.
     :camera     (camera-looking-at [12 6.75 12] [0 2.5 0] [0 1 0] 1.5)
     :background pov-black
     :view       {:curve :clip}
     :objects
     [; Moonlight and firelight, as in avatar.pov.
      (light {:location [90 90 90] :color [0.1 0.1 0.2]})
      (light {:location [0.95 2.3 0.5] :color [0.2 0.1 0.0]})
      (snowman true)
      snow-drift
      (placed-bowtie metallic-red)
      mirror-glass
      mirror
      snowman-compass]}))
