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

;; --------------------------------------------------------------------
;; mod, quot — Phase 6 additions
;; --------------------------------------------------------------------
;;
;; `mod` follows Clojure semantics (sign of result matches sign of
;; divisor); `quot` is truncating integer division (toward zero).
;; Both preserve int-ness when both args are ints.

; Plain int / int — int result.
(assert= (mod  10 3)  1)
(assert= (mod  9  3)  0)
(assert= (quot 10 3)  3)
(assert= (quot 9  3)  3)

; Sign-of-divisor invariant for mod, distinct from rem-style "%".
; Negative dividend, positive divisor: mod result is positive.
(assert= (mod -10 3)  2)
(assert= (mod -9  3)  0)
; Positive dividend, negative divisor: mod result is negative.
(assert= (mod 10 -3) -2)

; quot always rounds toward zero, regardless of sign.
(assert= (quot -10 3) -3)
(assert= (quot 10 -3) -3)
(assert= (quot -10 -3) 3)

; Float promotion: any float arg → float result, but the value is
; still produced via the same algebra.
(assert= (mod  10.0 3) 1.0)
(assert= (mod  10 3.0) 1.0)
(assert= (quot 10.0 3) 3.0)
(assert= (quot 10 3.0) 3.0)

; Identity (for non-negative inputs): n = d * (quot n d) + (mod n d).
(def n 17)
(def d 5)
(assert= (+ (* d (quot n d)) (mod n d)) n)

; Division by zero on either op panics — covered visually since
; assert can't catch panics here. The grid generator in
; scenes/sphere_surface_test.lisp is the smoke test that exercises
; mod + quot end-to-end against real iteration.

;; --------------------------------------------------------------------
;; Rounding and conversion.
;; --------------------------------------------------------------------

; floor / ceil / round keep a float a float and leave ints alone.
(assert= (floor 2.7) 2.0)
(assert= (floor -2.2) -3.0)
(assert= (ceil 2.1) 3.0)
(assert= (ceil -2.7) -2.0)
(assert= (round 2.5) 3.0)
(assert= (round -2.5) -3.0)
(assert= (round 2.4) 2.0)
(assert= (floor 7) 7)
(assert (int? (floor 7)))
(assert (float? (floor 7.0)))

; int truncates toward zero and returns an int; float converts.
(assert= (int 2.7) 2)
(assert= (int -2.7) -2)
(assert= (int 5) 5)
(assert (int? (int 2.7)))
(assert= (int (floor -2.2)) -3)
(assert= (float 3) 3.0)
(assert (float? (float 3)))

; Picking one of n choices from a number in [0, 1).
(assert= (int (* 21 0.999)) 20)
(assert= (int (* 21 0.0)) 0)

;; --------------------------------------------------------------------
;; Powers, logs and inverse trig. All return floats.
;; --------------------------------------------------------------------

(assert= (pow 2 10) 1024.0)
(assert= (pow 4 0.5) 2.0)
(assert= (exp 0) 1.0)
(assert= (log 1) 0.0)
(assert (< (abs (- (log (exp 2.5)) 2.5)) 0.000000000001))
(assert= (asin 1) (/ pi 2))
(assert= (acos 1) 0.0)
(assert= (atan 1) (/ pi 4))
(assert= (atan2 1 1) (/ pi 4))
(assert= (atan2 1 -1) (* 3 (/ pi 4)))
(assert= (atan2 -1 0) (- (/ pi 2)))
