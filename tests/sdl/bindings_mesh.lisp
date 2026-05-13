; Phase 7 — load-obj binding.
;
; (load-obj <path-string> <surface>) reads a Wavefront OBJ file from
; disk and returns it as a Shape (specifically a Shape::Group of
; Shape::Triangles, all sharing the supplied surface). Path resolution
; matches (load ...): relative paths anchor against the loading
; file's directory via CURRENT_DIR.
;
; Coverage:
; 1. Loading succeeds for a sibling .obj file (relative path).
; 2. The returned value is a shape (not an error or nil).
; 3. The shape can be wrapped in transforms and bounded() — i.e. it
;    composes with the rest of the shape constructors as expected.

(def s (surface {:color [1.0 0.5 0.0] :ambient 0.2 :light 0.8}))

;; --------------------------------------------------------------------
;; Direct load — relative path, sibling file.
;; --------------------------------------------------------------------

(def mesh (load-obj "load_obj_fixture.obj" s))

(assert (shape? mesh))

;; --------------------------------------------------------------------
;; Composition — wrap the loaded mesh in transforms and a bounding box,
;; same idiom scenes use to position and accelerate meshes.
;; --------------------------------------------------------------------

(def positioned
  (translate [3 0 -1] mesh))

(assert (shape? positioned))

(def scaled
  (scale [0.5 0.5 0.5] mesh))

(assert (shape? scaled))

(def accelerated
  (bounded (translate [0 0 -2] (scale [1.5 1.5 1.5] mesh))))

(assert (shape? accelerated))

;; --------------------------------------------------------------------
;; The loaded mesh can serve as the only object in a renderable scene.
;; Don't actually render here — that's the equivalence harness's job.
;; This just confirms the type composition works.
;; --------------------------------------------------------------------

(def mesh-scene
  (scene {:name          "mesh-load-test"
          :camera        (camera-looking-at [0 0 5] [0 0 0] [0 1 0] 1.0)
          :background    [0 0 0]
          :objects       [(light-white [10 10 10])
                          mesh]
          :reflect-limit 1
          :min-samples   1
          :max-samples   1}))

(assert (scene? mesh-scene))
