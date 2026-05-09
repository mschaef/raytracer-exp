; SDL port of scenes.rs::scene_cylinder_test.
;
; Three cylinders in distinct orientations exercise the side surface,
; both end caps, and their interaction:
;   - Vertical red on the left, axis +z. Top cap fully visible flat-shaded;
;     side curves around with smooth shading and a wrap-around highlight.
;   - Green pointing end-on at the camera in the middle. p1's cap normal is
;     +y, facing the lens at [0,10,0]. The headline regression test for
;     closed cylinders: must render as a solid flat disk, not the
;     see-through ring caps-disabled would give.
;   - Diagonal blue reflective on the right. Tilts in all three axes so
;     both caps are partially visible alongside the curved side.
;
; The reflective blue surface is constructed via `(reflective ...)`
; from _common.lisp — the SDL doesn't (yet) have surface-field
; accessors, so reflective is color-driven rather than surface-driven.

(load "_common.lisp")

(def cylinder-test-scene
  (scene
    {:name          "Cylinder Test"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :lights        [(light-white [10 10 10])]
     :reflect-limit 2
     :oversample    2
     :objects
     [;; Vertical, axis along +z. Bottom cap on the ground.
      (cylinder {:p0 [-2.5 0 -2] :p1 [-2.5 0 1] :r 0.6 :surface surface-red})

      ;; End-on, axis along +y. p1 (near end) cap normal is +y,
      ;; pointing back toward the camera at [0, 10, 0].
      (cylinder {:p0 [0 -1 0] :p1 [0 2.5 0] :r 0.7 :surface surface-green})

      ;; Diagonal, reflective. p0 sits on the ground at the back-right;
      ;; p1 floats above and forward. Tilts in all three axes so neither
      ;; cap is hidden from the camera.
      (cylinder {:p0 [2 -1 -2] :p1 [3.5 1.5 1] :r 0.4
                 :surface (reflective [0.0 0.0 1.0])})

      ;; Reflective checkered ground plane.
      (plane {:normal [0 0 1] :p0 [0 0 -2] :surface surface-white-c})]}))
