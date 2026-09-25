; Rope braids, ported from braids/braids.pov in the POV-Ray projects
; (github.com/mschaef/povray-projects): six ropes of beads wound around
; each other and twisting up a vertical axis, 200 rows high, seen from
; xmastree's close "ground" camera (gAngle 2). Render at 4:3, e.g.
; SIZE=640x480.
;
; The original builds 200 x 6 x 8 = 9,600 spheres with nested #while
; loops; here that's one `for` comprehension computing each sphere's
; centre, and a `bvh` over the result. It uses the xmastree harness from
; _pov.lisp (lights, white ground and background). gDetail is 4, so the
; spotlight is also an area light, and the compass (drawn only below
; gDetail 4) is left out.

(load "_pov.lisp")

(defn rot-y [d] (affine-rotation-y (deg->rad d)))

; One bead: row i, rope j, strand k. In the original each strand is a
; sphere at <0.075, 0.1 i, 0> turned about y by (-9 i + 45 k)°, then
; each rope of 8 strands is moved out by 0.25 and turned about y by
; (60 j + 10 i)°.
(defn bead [i j k]
  (let [place (affine-compose (rot-y (+ (* 60 j) (* 10 i)))
                              (affine-compose (affine-translation [0.25 0 0])
                                              (rot-y (+ (* -9 i) (* 45 k)))))]
    (sphere {:center (affine-apply place [0.075 (* 0.1 i) 0]) :r 0.05})))

(def braids
  (with-surface (pov-plain pov-blue)
    (bvh (for [i (range 200) j (range 6) k (range 8)]
           (bead i j k)))))

(def braids-scene
  (scene
    {:name        "Braids"
     ; gAngle 2: location <0,3,0> + 5, look_at <0,3,0>, direction 2*z.
     :camera      (camera-looking-at [5 8 5] [0 3 0] [0 1 0] 2.0)
     :background  pov-white
     ; As in xmastree: the area light needs more than the default 4
     ; samples per pixel for clean soft shadows.
     :min-samples 16
     :max-samples 64
     :objects     (concat (xmas-lights true) [braids xmas-ground])}))
