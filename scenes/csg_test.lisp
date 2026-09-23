; SDL scene exercising CSG: (difference ...) and (intersection ...).
;
; Three objects on the reflective checker ground, left to right as
; seen from the default camera:
;
;   * Carved cube (red, with yellow cut faces): a cube minus a sphere
;     slightly larger than its half-width, which leaves only the eight
;     corners. The sphere carries its own yellow surface, so every
;     face the sphere cut shows yellow; the cube's original faces stay
;     red. Checks that cut faces take the cutter's surface and that
;     their normals are flipped: the concave yellow faces should be lit
;     on the side facing the light, not the far side.
;   * Rounded cube (glassy blue): a sphere intersected with a cube,
;     seen through. Transmitted rays start inside the solid, so this
;     checks that a ray leaving a CSG solid finds its back face.
;   * Bowl (gold, metallic): a sphere minus a slightly smaller sphere
;     minus a half-space, opening upward and tilted a little toward the
;     camera. The inside of the bowl should be visible and lit, the rim
;     a thin flat ring, and it should reflect its surroundings.
;
; All three cast shadows on the floor that should follow the cut
; shapes, not the uncut primitives: the carved cube's shadow in
; particular should show gaps between its corners.
;
; Camera framing note: default-camera looks down -y from [0 10 0] with
; +z up, so larger y is nearer the camera.

(load "_common.lisp")

(def surface-glass (glassy [0.6 0.8 1.0] 0.6))

(def carved-cube
  (difference (cuboid {:center [0 0 0] :size [1.6 1.6 1.6] :surface surface-red})
              (sphere {:center [0 0 0] :r 1.0 :surface surface-yellow})))

(def rounded-cube
  (with-surface surface-glass
    (intersection (sphere {:center [0 0 0] :r 1.0})
                  (cuboid {:center [0 0 0] :size [1.6 1.6 1.6]}))))

; The half-space removed from the shell is the side the bowl opens
; toward: mostly up (+z), tilted 30° toward the camera (+y), i.e. the
; points where p·[0 0.5 0.866] >= 0. A plane's solid side is opposite
; its normal, so the normal points the other way. Plane normals must be
; unit length.
(def bowl
  (with-surface surface-gold
    (difference (sphere {:center [0 0 0] :r 1.0})
                (sphere {:center [0 0 0] :r 0.92})
                (plane  {:normal [0 -0.5 (- (/ (sqrt 3.0) 2))] :p0 [0 0 0]}))))

(def csg-test-scene
  (scene
    {:name          "CSG Test"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :reflect-limit 3
     :objects
     [(light-white [10 10 10])
      (translate [-3 0 -1.2] (rotate-z (/ pi 5) carved-cube))
      (translate [0 0 -1.2] (rotate-z (/ pi 8) rounded-cube))
      (translate [3 0 -1] bowl)
      (plane {:normal [0 0 1] :p0 [0 0 -2] :surface surface-white-c})]}))
