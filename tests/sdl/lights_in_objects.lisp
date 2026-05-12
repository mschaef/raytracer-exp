; Lights as scene-graph shapes.
;
; Verifies the mechanism: lights live inside `:objects`, get
; transformed by enclosing `(translate ...)` / `(rotate-* ...)` etc.,
; and pass through `(group ...)` and `(bounded ...)` the same way
; geometry does. After the stage-2 collapse `:objects` is the only
; place lights can live; the scene constructor rejects `:lights` to
; surface unmigrated scenes loudly.
;
; Self-contained: tests/sdl/ scripts can't `(load "_common.lisp")`
; because that file lives under scenes/, not tests/sdl/, and load
; paths resolve relative to the loading file's directory. Inline
; surface and camera definitions instead.

(def red    (surface {:color [1.0 0.2 0.2] :ambient 0.2
                      :specular 0.5 :light 0.6}))
(def white  (surface {:color [1.0 1.0 1.0] :ambient 0.2
                      :specular 0.5 :light 0.6}))
(def ground (surface {:color [0.2 0.2 0.2] :ambient 0.2
                      :specular 0.5 :light 0.6 :checked true}))
(def cam    (camera-looking-at [0 6 3] [0 0 0] [0 0 1] 1.0))

;; ----------------------------------------------------------------
;; Coercion: a (light-white ...) value is accepted wherever a shape
;; is expected. The light constructors still return `Value::Light`
;; (so `light?` keeps the obvious semantics), but the shape-arg
;; extraction at the host boundary auto-wraps it into
;; `Shape::Light(...)`. Existing `light?` / `shape?` predicates
;; don't change: a bare light is a light, not a shape.

(def bare-light (light-white [5 5 5]))
(assert (light? bare-light))
(assert (not (shape? bare-light)))

;; Wrapping a light in translate yields a shape (Shape::Transform
;; around Shape::Light). The light position [0 0 0] inside the
;; translate plus the [5 5 5] offset puts the effective world-space
;; light at [5 5 5] — the same position as `bare-light` above.
(def translated-light (translate [5 5 5] (light-white [0 0 0])))
(assert (shape? translated-light))
(assert (not (light? translated-light)))

;; Rotation and scale also accept lights (no rendering implication
;; for an isolated point light — affines don't change color or
;; intensity — but the wrapping itself must not error, and the
;; resulting shape must compose with the rest of the constructors).
(assert (shape? (rotate-z (/ pi 4) (light-white [1 0 0]))))
(assert (shape? (scale [2 2 2] (light-white [0 0 0]))))

;; Group accepts a mix of lights and geometry.
(def mixed-group
  (group [(light-white [0 5 0])
          (sphere {:center [0 0 0] :r 1 :surface red})]))
(assert (shape? mixed-group))

;; Bounded accepts a light (degenerate point AABB). Not useful in
;; isolation, but it must compose so that scripts can wrap a whole
;; subtree containing both lights and geometry without filtering.
(assert (shape? (bounded (light-white [1 2 3]))))

;; ----------------------------------------------------------------
;; Scene construction: lights live in `:objects` alongside geometry.

(def s
  (scene
    {:name          "lights-in-objects"
     :camera        cam
     :background    [0 0 0]
     :reflect-limit 0
     :oversample    1
     :objects
     [(translate [5 5 5] (light-white [0 0 0]))
      (light-point [-5 5 5] [1.0 0.5 0.5] 0.8)
      (sphere {:center [0 0 0] :r 1 :surface white})
      (plane {:normal [0 0 1] :p0 [0 0 -1.5] :surface ground})]}))

(assert (scene? s))

;; Multiple bare lights in `:objects` compose without any
;; per-element ceremony — same auto-wrap path as a single light.
(def s-many
  (scene
    {:name          "lights-many"
     :camera        cam
     :background    [0 0 0]
     :reflect-limit 0
     :oversample    1
     :objects
     [(light-white [10 0 0])
      (light-white [-10 0 0])
      (sphere {:center [0 0 0] :r 1 :surface white})]}))

(assert (scene? s-many))
