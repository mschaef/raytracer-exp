; Composition / transformation bindings: group, translate, scale,
; rotate-x/y/z, rotate-axis, transform, bounded, bounded-with, plus
; affine-* and aabb constructors.

(def red (surface {:color [1.0 0.0 0.0] :ambient 0.2 :specular 0.5 :light 0.6}))
(def sp (sphere {:center [0 0 0] :r 1.0 :surface red}))

; Group of leaf shapes.
(def grp (group [sp
                 (sphere {:center [3 0 0] :r 0.5 :surface red})]))
(assert (shape? grp))

; translate, scale, rotate-* all return shapes.
(assert (shape? (translate [1 2 3] sp)))
(assert (shape? (scale [2 2 2] sp)))
(assert (shape? (rotate-x 0.5 sp)))
(assert (shape? (rotate-y 0.5 sp)))
(assert (shape? (rotate-z 0.5 sp)))
(assert (shape? (rotate-axis [1 1 0] 0.5 sp)))

; bounded auto-computes the bound from the child.
(def bsp (bounded sp))
(assert (shape? bsp))

; Two equivalent translation shapes compare equal.
(def t1 (translate [1 0 0] sp))
(def t2 (translate [1 0 0] sp))
(assert= t1 t2)

; Different offsets → not equal.
(def t3 (translate [0 1 0] sp))
(assert (not= t1 t3))

; Affine constructors.
(def i (affine-identity))
(assert (affine? i))

(def at (affine-translation [1 2 3]))
(assert (affine? at))

(def as (affine-scale [2 2 2]))
(assert (affine? as))

(def arx (affine-rotation-x 0.5))
(def ary (affine-rotation-y 0.5))
(def arz (affine-rotation-z 0.5))
(assert (affine? arx))
(assert (affine? ary))
(assert (affine? arz))

(def ara (affine-rotation-axis [0 0 1] 0.5))
(assert (affine? ara))

; Compose two affines and check it's still an affine. Composition is
; "apply b first, then a" — same convention as Affine::compose.
(def ac (affine-compose at as))
(assert (affine? ac))

; Inverse of identity is identity (structurally equal).
(assert= (affine-inverse i) i)

; transform binding accepts an Affine and a shape.
(def trf (transform at sp))
(assert (shape? trf))

; AABB construction.
(def box (aabb [-1 -1 -1] [1 1 1]))
(assert (aabb? box))

; bounded-with takes an explicit AABB.
(def bw (bounded-with box sp))
(assert (shape? bw))

; Two AABBs with the same corners are equal.
(def box2 (aabb [-1 -1 -1] [1 1 1]))
(assert= box box2)

; Negative checks.
(assert (not (affine? sp)))     ; shape, not affine
(assert (not (aabb? sp)))       ; shape, not aabb
(assert (not (shape? box)))     ; aabb, not shape
