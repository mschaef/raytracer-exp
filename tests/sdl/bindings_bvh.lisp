; (bvh [shapes...]): like group, but organized as a bounding-volume
; hierarchy. Renders identically to the same shapes in a group (pinned
; byte-for-byte by the `bvh_render_equivalence` Rust test in
; tests/sdl_suite.rs); here we check construction and composition.

(def red (surface {:color [1 0 0] :ambient 0.2 :specular 0.5 :light 0.6}))

(def balls (for [i (range 50)] (sphere {:center [(* 0.5 i) 0 0] :r 0.2})))

; A bvh is a shape, and a bvh of one shape still is.
(def tree (bvh balls))
(assert (shape? tree))
(assert (shape? (bvh [(sphere {:center [0 0 0] :r 1})])))
(assert (shape? (bvh [])))

; Deterministic: the same shapes build the same tree.
(assert= tree (bvh balls))

; Nested groups are flattened, so grouping the input differently gives
; the same tree.
(assert= tree (bvh [(group (for [i (range 25)] (nth balls i)))
                    (group (for [i (range 25 50)] (nth balls i)))]))

; A bvh isn't structurally a group, even though it renders like one.
(assert (not= tree (group balls)))

; Planes have no bounds; they're kept beside the tree.
(assert (shape? (bvh (conj balls (plane {:normal [0 0 1] :p0 [0 0 -1]})))))

; Composes like any shape: transforms, surfaces, CSG (a bvh of solids
; is a solid).
(assert (shape? (translate [1 2 3] tree)))
(assert (shape? (with-surface red tree)))
(assert (shape? (difference (cuboid {:center [12 0 0] :size [30 1 1]}) tree)))

; Renders.
(def s (scene {:name "bvh-bindings"
               :camera (camera-looking-at [12 -20 5] [12 0 0] [0 0 1] 1.0)
               :background [0 0 0]
               :objects [(light-white [0 -10 10]) (with-surface red tree)]
               :min-samples 1
               :max-samples 1}))
(def t (png-target 8 8))
(assert= (render s t 8 8) t)
