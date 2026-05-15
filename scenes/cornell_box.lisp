; Cornell box — geometry test scene (Phase 1: single point light).
;
; The canonical Cornell box: a five-walled room — red left wall, green
; right wall, white floor / ceiling / back wall, open front for the
; camera — containing two white boxes: a tall one rotated toward the
; back-left, a short one rotated toward the front-right.
;
; This is Phase 1 of a phased plan. The renderer has no area lights and
; no diffuse global illumination, so the two effects the Cornell box
; was designed to demonstrate — soft shadows and color bleeding — are
; absent. What this scene IS good for: a geometry / shadow / camera
; stress test — a plane-bounded enclosure, rotated cuboids composed
; through Transform, a non-default look-at camera, and a multi-surface
; scene.
;
; Phase 1 lights the room with a single point light at the ceiling
; center (hard shadows). Phase 2 will swap that for a small cluster of
; point lights across a ceiling panel to fake softer shadows — still
; no renderer changes. Faithful soft shadows (area lights) and color
; bleeding (GI) are deferred renderer work.
;
; Coordinate convention follows the rest of scenes/: +z is up (the
; camera up-hint is [0 0 1]), +y is scene depth. The room is the
; 2x2x2 cube centered at the origin; the camera sits in front of the
; open -y face looking down +y toward the back wall.

(load "_common.lisp")

;; Matte surface helper — pure Lambertian, no specular sheen, no
;; reflection. The Cornell box is the textbook matte scene; the modest
;; ambient term keeps shadowed faces off pure black, since the renderer
;; has no GI to fill them.
(def matte
  (fn [color]
    (surface {:color      color
              :ambient    0.15
              :specular   0.0
              :light      0.85
              :checked    false
              :reflection 0.0})))

(def cb-white (matte [0.85 0.85 0.85]))
(def cb-red   (matte [0.75 0.1  0.1]))
(def cb-green (matte [0.1  0.65 0.15]))

;; Camera in front of the open face, framed so the back wall fills the
;; square frame: at distance 5 from the back wall, zoom 2.5 gives a
;; view half-height of exactly 1.0 — the back wall's half-height.
(def cornell-camera
  (camera-looking-at [0 -4 0] [0 0 0] [0 0 1] 2.5))

(def cornell-box-scene
  (scene
    {:name          "Cornell Box"
     :camera        cornell-camera
     :background    [0.0 0.0 0.0]
     :reflect-limit 1
     :objects
     [;; Single point light at the ceiling center (Phase 1).
      (light-white [0 0 0.9])

      ;; --- The room: five infinite planes -------------------------
      ;; Each normal faces inward, into the room.
      (plane {:normal [0  0  1] :p0 [0 0 -1] :surface cb-white})  ; floor
      (plane {:normal [0  0 -1] :p0 [0 0  1] :surface cb-white})  ; ceiling
      (plane {:normal [0 -1  0] :p0 [0 1  0] :surface cb-white})  ; back wall
      (plane {:normal [1  0  0] :p0 [-1 0 0] :surface cb-red})    ; left wall
      (plane {:normal [-1 0  0] :p0 [1  0 0] :surface cb-green})  ; right wall

      ;; --- The two inner boxes ------------------------------------
      ;; Each is built as a cuboid centered at the origin, rotated in
      ;; place about the vertical (z) axis, then translated into
      ;; position. `(translate (rotate-z (cuboid ...)))` composes
      ;; inner-to-outer, so the rotation happens while the box is still
      ;; at the origin — it spins in place rather than orbiting.

      ;; Tall box, back-left, rotated ~+18 degrees.
      (translate [-0.35 0.35 -0.375]
        (rotate-z (/ pi 10)
          (cuboid {:center [0 0 0] :size [0.55 0.55 1.25] :surface cb-white})))

      ;; Short box, front-right, rotated ~-15 degrees.
      (translate [0.4 -0.4 -0.7]
        (rotate-z (/ pi -12)
          (cuboid {:center [0 0 0] :size [0.6 0.6 0.6] :surface cb-white})))]}))
