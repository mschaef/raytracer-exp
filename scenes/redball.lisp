; A green metal ball, ported from redball/red.pov in the POV-Ray projects
; (github.com/mschaef/povray-projects): one sphere in front of a white,
; self-lit backdrop, with a single light. Despite the file name, the ball
; is green. Renders at 640x480 (its :size); SIZE overrides.
;
; The ball's finish is ambient 0.15, diffuse 0.6, specular 0.8,
; roughness 1/100, brilliance 5, metallic, no reflection, and all of it
; carries over: roughness 1/100 is :shininess 100, and :metallic keeps
; the diffuse term (history entry 67). The result matches red.tga's
; darker body and small green highlight.

(load "_pov.lisp")

(def redball-scene
  (scene
    {:name       "Red Ball"
     :size       [640 480]
     ; POV's default camera shape: zoom 1.
     :camera     (pov-camera [0 0 -3] [0 0 0])
     :background pov-black
     ; Hue-preserving clip, chosen by eye (tuning pass). The backdrop's
     ; ambient 1 makes it exactly 1.0, which the default Reinhard (white
     ; 4) greys to about 0.75 on screen and which clips to white here,
     ; as in POV. (red.tga has a black backdrop, which red.pov's plane
     ; contradicts; this follows red.pov.)
     :view       {:curve :hue-clip}
     :objects
     [(light-white [4 4 -4])
      (sphere {:center [0 0 0] :r 1
               :surface (surface {:color pov-green :ambient 0.15 :light 0.6 :specular 0.8
                                 :shininess 100 :brilliance 5 :metallic true})})
      ; plane { <0,0,1>, 10 } with finish { ambient 1 }: a white backdrop
      ; behind the ball, lit by its own ambient term.
      (plane {:normal [0 0 1] :p0 [0 0 10]
              :surface (surface {:color pov-white :ambient 1.0 :light 0.6 :specular 0.0})})]}))
