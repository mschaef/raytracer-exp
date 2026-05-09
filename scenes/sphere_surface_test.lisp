; SDL port of scenes.rs::scene_sphere_surface_test.
;
; A 5×5 grid of red spheres, each with a different (light, specular)
; combination. The Rust version uses an iterator (0..25).map(...);
; this port uses (map (fn ...) (range 25)) and the Phase 6 mod/quot
; builtins to recover the row/column from the linear index.
;
; Both axes range 0..5; the `- col 2` and `- row 2` shifts center
; the grid on the origin. light = col / 5.0 and specular = row / 5.0
; mirror the Rust expressions exactly.

(load "_common.lisp")

;; --------------------------------------------------------------------
;; Per-sphere surface — same body color, light/specular vary by grid cell.
;; --------------------------------------------------------------------

(def test-surface
  (fn [light specular]
    (surface {:color      [1.0 0.0 0.0]
              :ambient    ambient
              :specular   specular
              :light      light
              :checked    false
              :reflection 0.0})))

;; --------------------------------------------------------------------
;; Build one sphere from a linear index in [0, 25).
;; --------------------------------------------------------------------
;;
;; col is x mod 5 (0..5); row is x quot 5 (0..5). Both surface params
;; come out as fractions of 5: 0.0, 0.2, 0.4, 0.6, 0.8 — same as
;; the Rust expression (x % 5) as f64 / 5.0.

(def make-sphere
  (fn [x]
    (let [col (mod  x 5)
          row (quot x 5)]
      (sphere {:center  [(- col 2) 0 (- row 2)]
               :r       0.4
               :surface (test-surface (/ col 5.0) (/ row 5.0))}))))

(def sphere-surface-test-scene
  (scene
    {:name          "Surface Finish Test"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :lights        [(light-white [5 5 5])]
     :reflect-limit 2
     :oversample    2
     :objects       (map make-sphere (range 25))}))
