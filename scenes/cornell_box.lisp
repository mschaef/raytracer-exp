; Cornell box — geometry test scene (Phase 2: point-light cluster).
;
; The canonical Cornell box: a five-walled room — red left wall, green
; right wall, white floor / ceiling / back wall, open front for the
; camera — containing two white boxes: a tall one rotated toward the
; back-left, a short one rotated toward the front-right.
;
; This is Phase 2 of a phased plan. The renderer has no area lights and
; no diffuse global illumination, so the two effects the Cornell box
; was designed to demonstrate — soft shadows and color bleeding — can't
; be reproduced faithfully. What this scene IS good for: a geometry /
; shadow / camera stress test — a plane-bounded enclosure, rotated
; cuboids composed through Transform, a non-default look-at camera, and
; a multi-surface scene.
;
; Phase 1 lit the room with a single point light at the ceiling center,
; which gave hard-edged shadows. Phase 2 (this version) swaps that for
; a grid of point lights spread across a ceiling panel: each shading
; point now sees a slightly different set of occluders across the
; cluster, so shadow edges soften into a penumbra. It's a fake — there
; is still no area-light primitive — but it's a pure-SDL fake that
; needs no renderer changes. The cluster's combined intensity sums to
; 1.0 (each light is 1/N of full), so overall brightness matches the
; Phase 1 single-light version. Faithful soft shadows (area lights)
; and color bleeding (GI) remain deferred renderer work.
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

;; --------------------------------------------------------------------
;; Ceiling light panel — a cluster of point lights faking an area light
;; --------------------------------------------------------------------
;;
;; `light-grid-n` point lights per side (so N x N total) are spread
;; evenly across a square panel of half-width `light-panel-hw`, sitting
;; just below the ceiling at z = `light-z`. Each light carries 1/(N*N)
;; of full intensity, so the cluster sums to the same total light as
;; the Phase 1 single source. More lights = smoother penumbra and more
;; shadow rays per shading point; 4 (= 16 lights) is a reasonable
;; trade for this simple scene.

(def light-grid-n   4)
(def light-panel-hw 0.3)
(def light-z        0.9)
(def light-count    (* light-grid-n light-grid-n))
(def light-intensity (/ 1.0 light-count))

;; Build one cluster light from a linear index in [0, light-count).
;; col / row recover the 2D grid position; each light sits at the
;; center of its cell, so the cluster is symmetric about the origin.
(def make-cluster-light
  (fn [k]
    (let [col  (mod  k light-grid-n)
          row  (quot k light-grid-n)
          step (/ (* 2.0 light-panel-hw) light-grid-n)
          lx   (- (* (+ col 0.5) step) light-panel-hw)
          ly   (- (* (+ row 0.5) step) light-panel-hw)]
      (light-point [lx ly light-z] [1.0 1.0 1.0] light-intensity))))

(def cornell-box-scene
  (scene
    {:name          "Cornell Box"
     :camera        cornell-camera
     :background    [0.0 0.0 0.0]
     :reflect-limit 1
     ;; The room geometry is a literal vector; `apply conj` then
     ;; appends each generated cluster light in turn, producing one
     ;; flat objects list. (Object order is irrelevant — the scene
     ;; constructor wraps the whole list in a single Group.)
     :objects
     (apply conj
       [;; --- The room: five infinite planes ---------------------
        ;; Each normal faces inward, into the room.
        (plane {:normal [0  0  1] :p0 [0 0 -1] :surface cb-white})  ; floor
        (plane {:normal [0  0 -1] :p0 [0 0  1] :surface cb-white})  ; ceiling
        (plane {:normal [0 -1  0] :p0 [0 1  0] :surface cb-white})  ; back wall
        (plane {:normal [1  0  0] :p0 [-1 0 0] :surface cb-red})    ; left wall
        (plane {:normal [-1 0  0] :p0 [1  0 0] :surface cb-green})  ; right wall

        ;; --- The two inner boxes --------------------------------
        ;; Each is built as a cuboid centered at the origin, rotated
        ;; in place about the vertical (z) axis, then translated into
        ;; position. `(translate (rotate-z (cuboid ...)))` composes
        ;; inner-to-outer, so the rotation happens while the box is
        ;; still at the origin — it spins in place rather than
        ;; orbiting.

        ;; Tall box, back-left, rotated ~+18 degrees.
        (translate [-0.35 0.35 -0.375]
          (rotate-z (/ pi 10)
            (cuboid {:center [0 0 0] :size [0.55 0.55 1.25] :surface cb-white})))

        ;; Short box, front-right, rotated ~-15 degrees.
        (translate [0.4 -0.4 -0.7]
          (rotate-z (/ pi -12)
            (cuboid {:center [0 0 0] :size [0.6 0.6 0.6] :surface cb-white})))]

       ;; The ceiling light cluster, appended onto the geometry above.
       (map make-cluster-light (range light-count)))}))
