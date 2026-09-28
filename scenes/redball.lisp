; A green metal ball, ported from redball/red.pov in the POV-Ray projects
; (github.com/mschaef/povray-projects): one sphere in front of a white,
; self-lit backdrop, with a single light. Despite the file name, the ball
; is green. Renders at 640x480 (its :size); SIZE overrides.
;
; The ball's finish is ambient 0.15, diffuse 0.6, specular 0.8,
; roughness 1/100, brilliance 5, metallic, no reflection. Per the
; porting conventions (_pov.lisp) this uses the renderer's own shading:
; the ambient, diffuse and specular numbers carry over, brilliance and
; roughness have no equivalent, and it isn't flagged :metallic, because
; this renderer's metallic model drops the diffuse term that POV's
; metallic finish keeps.

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
     ; as in POV. The highlight, about 1.6, keeps its green tint instead
     ; of rolling off to a grey smudge. (red.tga has a black backdrop,
     ; which red.pov's plane contradicts; this follows red.pov.)
     :view       {:curve :hue-clip}
     :objects
     [(light-white [4 4 -4])
      (sphere {:center [0 0 0] :r 1
               :surface (surface {:color pov-green :ambient 0.15 :light 0.6 :specular 0.8})})
      ; plane { <0,0,1>, 10 } with finish { ambient 1 }: a white backdrop
      ; behind the ball, lit by its own ambient term.
      (plane {:normal [0 0 1] :p0 [0 0 10]
              :surface (surface {:color pov-white :ambient 1.0 :light 0.6 :specular 0.0})})]}))
