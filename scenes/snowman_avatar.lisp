; The snowman avatar, ported from snowman/snowman_avatar/avatar.pov in
; the POV-Ray projects (github.com/mschaef/povray-projects): a beaten-up
; snowman with a red bowtie and top hat, standing on a mirror under a
; slab of glass, lit by dim blue moonlight and a faint orange firelight.
; The original renders at 65x75 (avatar.jpg); this keeps the shape at
; 520x600 (its :size); SIZE overrides.
;
; The view is the plain clip, as POV displayed it: nothing in this
; dimly lit scene reaches 1.0 (the brightest value is about 0.75), so
; the clip changes nothing and the image keeps avatar.jpg's levels.
; Measured over six regions (under the brim, hat band, nose, bowtie,
; body, floor) at avatar.jpg's size, it's within about 4 levels of it
; on average. The default Reinhard curve darkened it (about 8 levels
; off), and an exposure of +/-0.5 moved it further away.

(load "_snowman.lisp")

; The mirror, its glass and the compass are in _snowman.lisp (sphere.pov
; has them too).

(def snowman-avatar-scene
  (scene
    {:name       "Snowman Avatar"
     :size       [520 600]
     ; direction 1.5*z (zoom 1.5), up y, right 65/75 x: the 65:75 frame.
     :camera     (camera-looking-at [2.8 3.675 1.8] [0 2.5 0] [0 1 0] 1.5)
     :background pov-black
     :view       {:curve :clip}
     :objects
     [; Moonlight and firelight.
      (light {:location [90 90 90] :color [0.1 0.1 0.2]})
      (light {:location [0.95 2.3 0.5] :color [0.2 0.1 0.0]})
      (snowman true)
      (placed-bowtie metallic-red)
      mirror-glass
      mirror
      snowman-compass]}))
