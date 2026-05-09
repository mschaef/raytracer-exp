; SDL port of scenes.rs::scene_transform_test.
;
; Defines `transform-test-scene` for the visual-equivalence harness in
; tests/sdl_suite.rs. Pure data: no render side effects.
;
; Each object exercises a different transform path; the comments echo
; the per-object commentary in scenes.rs::scene_transform_test so the
; two are easy to compare.

(load "_common.lisp")

(def transform-test-scene
  (scene
    {:name           "Transform Test"
     :camera         default-camera
     :background     [0.0 0.0 0.0]
     :lights         [(light-white [10 10 10])]
     :reflect-limit  2
     :oversample     2
     :objects
     [;; A unit cube translated to (3, 0, 0). Should look identical
      ;; to a cuboid declared with center=[3,0,0] directly.
      (translate [3 0 0]
        (cuboid {:center [0 0 0] :size [1 1 1] :surface surface-red}))

      ;; A unit cube rotated 30° around z, then translated. Edges and
      ;; corners no longer line up with world axes.
      (translate [-3 0 0]
        (rotate-z (/ pi 6.0)
          (cuboid {:center [0 0 0] :size [1.5 1.5 1.5] :surface surface-green})))

      ;; A unit sphere stretched non-uniformly into an ellipsoid, then
      ;; translated. Verifies the inverse-transpose normal handling
      ;; under non-uniform scale: with a forward-matrix normal
      ;; transform the lighting on the long axis would be visibly wrong.
      (translate [0 3 0]
        (scale [1.5 0.6 0.6]
          (sphere {:center [0 0 0] :r 1 :surface surface-blue})))

      ;; A grouped pair of spheres, then transformed as a unit.
      ;; Confirms Group nests correctly inside Transform.
      (translate [0 -3 0.5]
        (rotate-y (/ pi 4.0)
          (group [(sphere {:center [-0.7 0 0] :r 0.4 :surface surface-orange})
                  (sphere {:center [ 0.7 0 0] :r 0.4 :surface surface-yellow})])))

      ;; Nested transforms: outer translate, inner rotate-x and a
      ;; deeper rotate-z. Each level inverse-transforms the ray on the
      ;; way down, so the leaf sees the composed inverse of all three.
      (translate [0 0 1.5]
        (rotate-x (/ pi 5.0)
          (rotate-z (/ pi 7.0)
            (cuboid {:center [0 0 0] :size [0.8 0.8 0.8] :surface surface-purple}))))

      ;; Reflective checkered ground plane, untransformed. Catches
      ;; shadows from all the transformed objects above.
      (plane {:normal [0 0 1] :p0 [0 0 -2] :surface surface-white-c})]}))
