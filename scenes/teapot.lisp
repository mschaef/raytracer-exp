; SDL port of scenes.rs::scene_teapot.
;
; Loads models/utah_teapot.obj from the repo root and renders it as a
; transformed mesh on the reflective checker ground. The mesh path
; is written script-relative (../models/...) — (load-obj ...) joins
; against CURRENT_DIR (this script's directory), so the resolved
; absolute path is the same as scenes.rs::scene_teapot's CWD-relative
; "models/utah_teapot.obj" when cargo test runs from the repo root.
;
; The OBJ file is *not* committed to the repo. Drop a Utah teapot
; model at models/utah_teapot.obj and the equivalence test will
; exercise this scene; without it, the test reports a skip and
; passes vacuously.
;
; Without a BVH-builder this scene renders slowly: the teapot has on
; the order of 6000 triangles and Group::hit_test is O(n). The
; (bounded ...) wrapper around the transforms gives a single-level
; AABB acceleration — rays that miss the world-space bound skip the
; entire mesh without paying for inverse-ray transforms. Phase 3 of
; the BVH plan (transformed bounds) is what makes wrapping outside
; the transforms safe.

(load "_common.lisp")

(def teapot-scene
  (scene
    {:name          "Utah Teapot"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :lights        [(light-white [10 10 10])]
     :reflect-limit 2
     :oversample    2
     :objects
     [;; Reflective checkered ground.
      (plane {:normal [0 0 1] :p0 [0 0 -2] :surface surface-white-c})

      ;; Loaded mesh, scaled and positioned to sit on the ground, with
      ;; the bounding box wrapped *outside* the transforms.
      ;; Shape::Transform::bounds() (BVH phase 3) computes a world-space
      ;; AABB by transforming the eight corners of the child's
      ;; local-space bound, so this composes correctly and is slightly
      ;; cheaper than wrapping inside the transforms — rays that miss
      ;; the world-space AABB don't even pay for the per-Transform ray
      ;; inverse-transform before the test happens.
      (bounded
        (translate [0 0 -2]
          (scale [0.5 0.5 0.5]
            (load-obj "../models/utah_teapot.obj" surface-blue))))]}))
