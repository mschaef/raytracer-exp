; CSG bindings: (difference a b ...), (intersection a b ...) and
; (merge a b ...).
;
; Construction, display, n-ary forms, composition with transforms and
; with-surface, nesting, and a small render through the CSG hit path.
; The rejection errors (too few operands, triangle or mesh operands)
; are covered by the `csg_rejects_bad_operands` Rust test in
; tests/sdl_suite.rs, since a failed construction can't be caught from
; script.

(def red   (surface {:color [1.0 0.0 0.0] :ambient 0.2 :specular 0.5 :light 0.6}))
(def white (surface {:color [1.0 1.0 1.0] :ambient 0.2 :specular 0.5 :light 0.6}))

(def ball  (sphere {:center [0 0 0] :r 1.0}))
(def inner (sphere {:center [0 0 0] :r 0.9}))
(def cube  (cuboid {:center [0 0 0] :size [1.6 1.6 1.6]}))
(def rod   (cylinder {:p0 [-2 0 0] :p1 [2 0 0] :r 0.3}))
(def half  (plane {:normal [0 0 1] :p0 [0 0 0]}))

; Both operations produce shapes, displayed by operation.
(def carved (difference cube ball))
(def clipped (intersection ball cube))
(assert (shape? carved))
(assert (shape? clipped))
(assert= (str carved) "#<shape difference>")
(assert= (str clipped) "#<shape intersection>")

; Structural equality: same operation and operands compare equal;
; changing the operation or the operand order doesn't.
(assert= carved (difference cube ball))
(assert (not= carved (difference ball cube)))
(assert (not= (intersection ball cube) (difference ball cube)))

; n-ary difference subtracts the union of everything after the first
; operand in one step, POV-Ray style.
(assert= (difference cube ball rod) (difference cube (group [ball rod])))
(assert (not= (difference cube ball rod) (difference (difference cube ball) rod)))

; n-ary intersection folds left.
(assert= (intersection ball cube rod) (intersection (intersection ball cube) rod))

; merge is a union with no internal faces. Like difference, extra
; operands are grouped.
(def merged (merge ball cube))
(assert (shape? merged))
(assert= (str merged) "#<shape merge>")
(assert (not= merged (group [ball cube])))
(assert= (merge ball cube rod) (merge ball (group [cube rod])))

; Every solid primitive is a valid operand, including a plane (as a
; half-space) and a cone. Any operand can be transformed, surfaced,
; bounded, or grouped.
(assert (shape? (difference ball half)))
(assert (shape? (intersection (cone {:p0 [0 0 -1] :p1 [0 0 1] :r 1}) ball)))
(assert (shape? (difference (translate [0.5 0 0] ball)
                            (rotate-z (/ pi 4) (with-surface red cube))
                            (bounded rod))))

; CSG results compose like any other shape, and nest.
(def bowl (difference (difference ball inner) half))
(assert (shape? (scale [1 1 0.5] bowl)))
(assert (shape? (with-surface white bowl)))
(assert (shape? (bounded bowl)))
(assert (shape? (difference bowl (sphere {:center [0 0 1] :r 0.3}))))

; A light inside a CSG operand is allowed: it doesn't enclose
; anything, but it doesn't make the operand non-solid either.
(assert (shape? (difference (group [ball (light-white [0 0 0])]) cube)))

; A scene with unsurfaced CSG operands validates when an enclosing
; with-surface supplies the surface, and it renders.
(def cam (camera-looking-at [0 -5 0] [0 0 0] [0 0 1] 1.0))
(def s (scene {:name "csg-bindings"
               :camera cam
               :background [0 0 0]
               :objects [(light-white [2 -4 3])
                         (with-surface white bowl)
                         (with-surface red (translate [0 0 -2] carved))]
               :min-samples 1
               :max-samples 1}))
(assert (scene? s))
(def t (png-target 8 8))
(assert= (render s t 8 8) t)
