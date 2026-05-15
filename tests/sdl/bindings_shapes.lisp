; Leaf shape bindings: sphere, plane, cuboid, triangle, cylinder, cone.

(def red (surface {:color [1.0 0.0 0.0] :ambient 0.2 :specular 0.5 :light 0.6}))
(def green (surface {:color [0.0 1.0 0.0] :ambient 0.2 :specular 0.5 :light 0.6}))

; Sphere.
(def sp (sphere {:center [0 0 0] :r 1.0 :surface red}))
(assert (shape? sp))

; Two spheres built the same way are equal.
(def sp2 (sphere {:center [0 0 0] :r 1.0 :surface red}))
(assert= sp sp2)

; A sphere with a different surface is not equal.
(def sp3 (sphere {:center [0 0 0] :r 1.0 :surface green}))
(assert (not= sp sp3))

; Plane.
(def pl (plane {:normal [0 0 1] :p0 [0 0 -2] :surface red}))
(assert (shape? pl))

; Cuboid.
(def cu (cuboid {:center [1 0 0] :size [1 1 1] :surface red}))
(assert (shape? cu))

; Triangle with explicit vertex normals.
(def tri (triangle {:vertices [[0 0 0] [1 0 0] [0 1 0]]
                    :normals  [[0 0 1] [0 0 1] [0 0 1]]
                    :surface red}))
(assert (shape? tri))

; Triangle with implicit (geometric) face normal.
(def tri2 (triangle {:vertices [[0 0 0] [1 0 0] [0 1 0]]
                     :surface red}))
(assert (shape? tri2))

; The triangle with implicit and the one with the matching explicit
; normals should be structurally equal: the geometric face normal of
; the (XY plane, CCW) triangle is +z, exactly what we passed to tri.
(assert= tri tri2)

; Cylinder.
(def cy (cylinder {:p0 [0 0 -2] :p1 [0 0 1] :r 0.6 :surface red}))
(assert (shape? cy))

; Cone. :p0 is the base (radius :r), :p1 the apex point.
(def co (cone {:p0 [0 0 -2] :p1 [0 0 1] :r 0.6 :surface red}))
(assert (shape? co))

; Two cones built the same way are equal.
(def co2 (cone {:p0 [0 0 -2] :p1 [0 0 1] :r 0.6 :surface red}))
(assert= co co2)

; The ends are not interchangeable: swapping base and apex is a
; different cone.
(def co-flipped (cone {:p0 [0 0 1] :p1 [0 0 -2] :r 0.6 :surface red}))
(assert (not= co co-flipped))

; A cone and a cylinder with the same p0/p1/r/surface are distinct shapes.
(assert (not= co cy))

; Negative checks.
(assert (not (shape? nil)))
(assert (not (shape? red)))     ; surface, not shape
