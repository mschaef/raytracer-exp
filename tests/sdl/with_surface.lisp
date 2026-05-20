; (with-surface ...) and Phase-2 optional :surface on leaves.
;
; The Phase-2 surface-decoupling work made `:surface` optional on
; every leaf primitive and added `(with-surface s shape)` to supply
; one from outside. A leaf without an explicit surface inherits from
; the deepest enclosing `with-surface`; an explicit leaf surface
; wins over any enclosing wrapper ("innermost wins"). Leaves with
; neither are scene-construction errors caught by the validator at
; `(scene ...)` time — see the `with_surface_validation_fails` Rust
; test in tests/sdl_suite.rs for that side.

(def gold (surface {:color [1.0 0.8 0.2] :ambient 0.3 :specular 0.6 :light 0.5 :metallic true}))
(def red  (surface {:color [1.0 0.0 0.0] :ambient 0.2 :specular 0.5 :light 0.6}))
(def blue (surface {:color [0.0 0.0 1.0] :ambient 0.2 :specular 0.5 :light 0.6}))

; A primitive without :surface still constructs — Phase 2's whole
; point. The result is a Shape; what kind of leaf-surface it carries
; isn't observable from SDL (Option<Surface> is a Rust detail).
(def bare-sphere (sphere {:center [0 0 0] :r 1.0}))
(assert (shape? bare-sphere))

; Same goes for every other primitive kind.
(assert (shape? (plane    {:normal [0 0 1] :p0 [0 0 -2]})))
(assert (shape? (cuboid   {:center [1 0 0] :size [1 1 1]})))
(assert (shape? (triangle {:vertices [[0 0 0] [1 0 0] [0 1 0]]})))
(assert (shape? (cylinder {:p0 [0 0 0] :p1 [0 1 0] :r 0.5})))
(assert (shape? (cone     {:p0 [0 0 0] :p1 [0 1 0] :r 0.5})))

; with-surface produces a Shape.
(def gold-sphere (with-surface gold bare-sphere))
(assert (shape? gold-sphere))

; Structural equality: same surface + same child = equal.
(def gold-sphere-2 (with-surface gold (sphere {:center [0 0 0] :r 1.0})))
(assert= gold-sphere gold-sphere-2)

; Different decorating surface → not equal.
(def red-sphere (with-surface red bare-sphere))
(assert (not= gold-sphere red-sphere))

; The decoration node is distinct from the bare leaf: wrapping
; changes the shape (the validator now sees a Surfaced ancestor for
; the inner leaf, even though the leaf itself is unchanged).
(assert (not= gold-sphere bare-sphere))

; A scene built from a wrapped unsurfaced leaf must validate
; successfully — `(scene ...)` would `sdl-panic!` if validation
; failed.
(def cam (camera-looking-at [0 0 5] [0 0 0] [0 1 0] 1.0))
(def scn (scene {:name "with-surface positive"
                 :camera cam
                 :background [0 0 0]
                 :objects [(light-white [10 10 10])
                           gold-sphere]
                 :reflect-limit 0
                 :min-samples 1
                 :max-samples 1}))
(assert (scene? scn))

; Mixed: a group with one unsurfaced leaf (gets default) and one
; explicitly-surfaced leaf (keeps its own). Both leaves are
; reachable; validation passes because the unsurfaced one is under
; the with-surface wrapper.
(def mixed (with-surface gold
             (group [(sphere {:center [-1 0 0] :r 0.5})
                     (sphere {:center [ 1 0 0] :r 0.5 :surface blue})])))
(def mixed-scene (scene {:name "with-surface mixed"
                         :camera cam
                         :background [0 0 0]
                         :objects [(light-white [10 10 10])
                                   mixed]
                         :reflect-limit 0
                         :min-samples 1
                         :max-samples 1}))
(assert (scene? mixed-scene))

; Composability check — with-surface goes through transform,
; bounded, group, etc. without surprises.
(assert (shape? (translate [1 2 3] (with-surface gold bare-sphere))))
(assert (shape? (with-surface gold (translate [1 2 3] bare-sphere))))
(assert (shape? (group [(with-surface gold bare-sphere)
                        (with-surface red bare-sphere)])))

; Optional surface on load-obj: zero-arg path form is constructable
; (the actual loader call would fail without a file on disk, so we
; only test the explicit-surface form here — the missing-surface
; form is exercised by tests/sdl/bindings_mesh.lisp via the existing
; fixture).
; (no assertion — just documentation that the load-obj surface arg is now optional.)
