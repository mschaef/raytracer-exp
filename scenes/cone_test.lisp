; Cone primitive smoke scene — mirrors the layout of cylinder_test.lisp.
;
; Three cones in distinct orientations exercise the lateral surface,
; the single base cap, and the apex tip:
;   - Vertical red on the left, axis +z. Base cap sits on the ground;
;     the lateral surface curves up to the apex with smooth shading and
;     a wrap-around highlight.
;   - Green pointing apex-on at the camera in the middle. The apex (p1)
;     faces the lens at [0,10,0]; the base (p0) is behind. Exercises the
;     apex tip and the s >= 0 nappe-rejection — the cone must render as
;     a solid point-tipped silhouette, not a doubled / see-through one.
;   - Diagonal blue reflective on the right. Tilts in all three axes so
;     the base cap and the lateral surface are both partially visible.
;
; :p0 is the base center (radius :r); :p1 is the apex point. The two
; ends are not interchangeable.
;
; The reflective blue surface is constructed via `(reflective ...)`
; from _common.lisp.

(load "_common.lisp")

(def cone-test-scene
  (scene
    {:name          "Cone Test"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :reflect-limit 2
     :objects
     [(light-white [10 10 10])
      ;; Vertical, axis along +z. Base cap on the ground, apex up.
      (cone {:p0 [-2.5 0 -2] :p1 [-2.5 0 1.5] :r 0.8 :surface surface-red})

      ;; Apex-on, axis along +y. p1 (the apex) points back toward the
      ;; camera at [0, 10, 0]; p0 (the base) is the far end.
      (cone {:p0 [0 -1 0] :p1 [0 2.5 0] :r 0.8 :surface surface-green})

      ;; Diagonal, reflective. p0 (base) sits on the ground at the
      ;; back-right; p1 (apex) floats above and forward. Tilts in all
      ;; three axes so the base cap and the side are both visible.
      (cone {:p0 [2 -1 -2] :p1 [3.5 1.5 1] :r 0.5
             :surface (reflective [0.0 0.0 1.0])})

      ;; Reflective checkered ground plane.
      (plane {:normal [0 0 1] :p0 [0 0 -2] :surface surface-white-c})]}))
