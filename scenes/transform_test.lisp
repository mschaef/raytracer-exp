; Phase 5 — port of scenes.rs::scene_transform_test to the SDL.
;
; Defines `transform-test-scene` for the visual-equivalence harness
; (`phase5_transform_test_scene_matches_rust` in tests/sdl_suite.rs).
; Pure data: no render side effects, so it can be evaluated freely
; from anywhere without touching disk.
;
; Numeric values match scenes.rs::scene_transform_test exactly. The
; harness renders this scene and the Rust scene to two in-memory
; buffers and asserts byte-level equality — pi, the rotation
; divisors, and the surface coefficients all need to round-trip to
; the same f64 the Rust version uses.

;; ----------------------------------------------------------------------
;; Surface presets
;; ----------------------------------------------------------------------
;;
;; Mirror the const fns and SURFACE_* constants in scenes.rs. `glossy`
;; is the lisp-side equivalent of the `surface_glossy` const fn (matte
;; body, modest specular highlight, no reflection); `surface-white-c`
;; is the reflective checkered ground.

(def ambient    0.2)
(def specular   0.5)
(def light      0.6)

(def glossy
  (fn [color]
    (surface {:color      color
              :ambient    ambient
              :specular   specular
              :light      light
              :checked    false
              :reflection 0.0})))

(def surface-red    (glossy [1.0 0.0 0.0]))
(def surface-green  (glossy [0.0 1.0 0.0]))
(def surface-blue   (glossy [0.0 0.0 1.0]))
(def surface-orange (glossy [1.0 0.5 0.0]))
(def surface-yellow (glossy [1.0 1.0 0.0]))
(def surface-purple (glossy [1.0 0.0 1.0]))

(def surface-white-c
  (surface {:color      [0.2 0.2 0.2]
            :ambient    ambient
            :specular   specular
            :light      light
            :checked    true
            :reflection 0.5}))

;; ----------------------------------------------------------------------
;; Camera
;; ----------------------------------------------------------------------
;;
;; Matches scenes.rs::default_camera: looking at the origin from
;; (0, 10, 0), world-up = +z, zoom = 1.0 (~53° vertical FOV).

(def default-camera
  (camera-looking-at [0 10 0] [0 0 0] [0 0 1] 1.0))

;; ----------------------------------------------------------------------
;; Scene
;; ----------------------------------------------------------------------
;;
;; Each object exercises a different transform path; the comments echo
;; the per-object commentary in scenes.rs::scene_transform_test so the
;; two are easy to compare.

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
