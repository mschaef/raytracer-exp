; Phase 4 — math helpers.
;
; Constants `pi` and `tau` and the angle-conversion helpers come from
; the lisp stdlib (`src/sdl/stdlib.lisp`). The transcendentals (sqrt,
; sin, cos, tan) and the variadic min/max/abs are Rust built-ins.

;; --------------------------------------------------------------------
;; Constants
;; --------------------------------------------------------------------

; Tight bounds rather than exact floats, so this passes regardless of
; the exact double-precision representation chosen in stdlib.lisp.
(assert (and (> pi 3.14159) (< pi 3.14160)))
(assert (and (> tau 6.28318) (< tau 6.28319)))

; tau ≈ 2 · pi.
(assert (< (abs (- tau (* 2.0 pi))) 0.0000001))

;; --------------------------------------------------------------------
;; Angle conversions
;; --------------------------------------------------------------------

; Degrees to radians: 0°, 90°, 180°, 360°.
(assert (< (abs (deg->rad 0))   0.0000001))
(assert (< (abs (- (deg->rad 90)  (/ pi 2)))  0.0000001))
(assert (< (abs (- (deg->rad 180) pi))        0.0000001))
(assert (< (abs (- (deg->rad 360) tau))       0.0000001))

; Round-trip.
(assert (< (abs (- (rad->deg (deg->rad 42)) 42)) 0.0000001))

;; --------------------------------------------------------------------
;; min, max — variadic, preserve int-ness when every arg is an int
;; --------------------------------------------------------------------

(assert= (min 1 2)       1)
(assert= (min 5 4 3 2 1) 1)
(assert= (max 1 2)       2)
(assert= (max 5 4 3 2 1) 5)
(assert= (min 7)         7)
(assert= (max 7)         7)

; A single float anywhere → float result, matching +/-/*/etc.
(assert= (min 1 2.0 3) 1.0)
(assert= (max 1.5 2 3) 3.0)

;; --------------------------------------------------------------------
;; abs
;; --------------------------------------------------------------------

(assert= (abs 0)     0)
(assert= (abs 5)     5)
(assert= (abs -5)    5)
(assert= (abs 0.0)   0.0)
(assert= (abs -3.5)  3.5)

;; --------------------------------------------------------------------
;; sqrt
;; --------------------------------------------------------------------

; sqrt always returns a float.
(assert= (sqrt 0)   0.0)
(assert= (sqrt 4)   2.0)
(assert= (sqrt 16)  4.0)
(assert (< (abs (- (sqrt 2) 1.4142135)) 0.0000001))

;; --------------------------------------------------------------------
;; trig
;; --------------------------------------------------------------------

; sin(0) = 0, cos(0) = 1.
(assert (< (abs (sin 0)) 0.0000001))
(assert (< (abs (- (cos 0) 1.0)) 0.0000001))

; sin(pi/2) = 1, cos(pi/2) = 0.
(assert (< (abs (- (sin (/ pi 2)) 1.0)) 0.0000001))
(assert (< (abs (cos (/ pi 2))) 0.0000001))

; sin² + cos² = 1.
(def angle 0.7)
(assert (< (abs (- 1.0 (+ (* (sin angle) (sin angle))
                          (* (cos angle) (cos angle)))))
           0.0000001))

; tan(pi/4) ≈ 1.
(assert (< (abs (- (tan (/ pi 4)) 1.0)) 0.0000001))
