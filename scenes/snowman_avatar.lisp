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

; The mirror glass: box { <-5, -0.5, -5>, <5, 0, 5> } with
; pigment { rgbf <0, 0, 0.1, 0.9> } and then texture { Glass3 }, which
; POV layers: Glass3 (near-white, filter 0.8, ambient 0.1, diffuse 0.1,
; reflection 0.1, specular 0.8, roughness 0.003) over the dark blue
; filter. Through both, only a little dark blue light gets through, and
; what shows is mostly Glass3's own dim body. One surface can't hold
; two layers, so this is an equivalent chosen to match avatar.jpg's
; floor, a greyish navy (about 34, 34, 58): a grey-blue body with a
; little blue-tinted filter. Glass3 alone (white, filter 0.8) left the
; floor light grey; the blue layer alone left it nearly black.
(def mirror-glass
  (with-surface (surface {:color [0.2 0.2 0.4] :ambient 0.1 :light 0.1
                          :specular 0.8 :shininess 333 :reflection 0.1
                          :filter 0.2})
    (box [-5 -0.5 -5] [5 0 5])))

; The mirror under it: plane { y, -0.4999 } clipped to a thin box, in
; Silver with ambient 0.15, diffuse 0.05, reflection 0.8, phong 0.9,
; phong_size 120, metallic.
(def mirror
  (with-surface (surface {:color [0.90 0.91 0.98] :ambient 0.15 :light 0.05
                          :specular 0.9 :shininess 120 :reflection 0.8
                          :metallic true})
    (intersection (plane {:normal [0 1 0] :p0 [0 -0.4999 0]})
                  (box [-5 0 -5] [5 -0.51 5]))))

; The compass at <2, 2, -2>: a black ball and red, green and blue arrows
; 0.8 long along +x, +y and +z.
(def avatar-compass
  (group [(sphere {:center [2 2 -2] :r 0.16 :surface matte-black})
          (with-surface metallic-red
            (group [(cylinder {:p0 [2 2 -2] :p1 [2.8 2 -2] :r 0.04})
                    (pov-cone [2.8 2 -2] 0.08 [3 2 -2] 0)]))
          (with-surface metallic-green
            (group [(cylinder {:p0 [2 2 -2] :p1 [2 2.8 -2] :r 0.04})
                    (pov-cone [2 2.8 -2] 0.08 [2 3 -2] 0)]))
          (with-surface metallic-blue
            (group [(cylinder {:p0 [2 2 -2] :p1 [2 2 -1.2] :r 0.04})
                    (pov-cone [2 2 -1.2] 0.08 [2 2 -1] 0)]))]))

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
      avatar-compass]}))
