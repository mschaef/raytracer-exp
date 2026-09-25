; The compass test scene, ported from train/train.pov in the POV-Ray
; projects (github.com/mschaef/povray-projects). Despite its name, the
; file draws only the xmastree compass (red +x, green +y, blue +z) on the
; white ground, seen from <10,10,10>. Render at 4:3, e.g. SIZE=640x480.
;
; It uses the xmastree harness from _pov.lisp at gDetail 0, which means
; the spotlight is a plain spotlight with no area light, so the compass
; casts hard shadows.

(load "_pov.lisp")

(def train-scene
  (scene
    {:name       "Train"
     ; location <1,1,1> * 10, look_at <0,0,0>, direction 2*z.
     :camera     (camera-looking-at [10 10 10] [0 0 0] [0 1 0] 2.0)
     :background pov-white
     :objects    (concat (xmas-lights false)
                         [(translate [0 2 0] pov-compass) xmas-ground])}))
