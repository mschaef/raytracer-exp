; SDL scene exercising Phase 2 of the light-types plan: spotlights.
;
; A single white spotlight points straight down at the checker floor
; from above the origin. Three spheres laid out along +x sit at
; progressively larger angular offsets from the cone axis so the
; falloff bands are visible in one frame:
;
;   - red sphere at x=0   — well inside the inner cone, fully lit.
;   - green sphere at x=2.5 — straddles the soft transition band.
;   - blue sphere at x=4    — outside the outer cone; only ambient
;                             and any reflection light it.
;
; The reflective checker floor catches the spotlight's circular
; footprint with a smooth edge between full-brightness inner radius
; and zero-brightness outer radius.
;
; Cone geometry: light at z=4, floor at z=-1 (so 5 units below the
; light). With inner half-angle π/8 (~22.5°) and outer π/5 (~36°),
; the floor footprint has inner radius ~2.07 and outer radius ~3.63 —
; both visible within the frame.

(load "_common.lisp")

(def spotlight-test-scene
  (scene
    {:name          "Spotlight Test"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :reflect-limit 2
     :objects
     [;; The spotlight itself. Aimed straight down (+z → -z).
      (light-spot [0 0 4]    ; location
                  [0 0 -1]   ; direction (unit; binding normalizes)
                  [1 1 1]    ; white
                  2.0        ; intensity
                  (/ pi 8)   ; inner half-angle, ~22.5°
                  (/ pi 5))  ; outer half-angle, ~36°

      ;; Sphere directly under the light: fully inside the inner cone.
      (sphere {:center [0   0 -0.5] :r 0.5 :surface surface-red})

      ;; Sphere at the edge of the inner cone, in the soft band.
      (sphere {:center [2.5 0 -0.5] :r 0.5 :surface surface-green})

      ;; Sphere outside the outer cone — only ambient light reaches it.
      (sphere {:center [4   0 -0.5] :r 0.5 :surface surface-blue})

      ;; Reflective checker floor.
      (plane  {:normal [0 0 1] :p0 [0 0 -1] :surface surface-white-c})]}))
