; Phase 3 — render-dispatch bindings.
;
; Validates the type predicate, target constructors, and `render`
; without touching disk. The end-to-end render+save+verify path is
; covered by the Rust-side `render_dispatch_save` test in
; tests/sdl_suite.rs, which injects an OUTPUT-PATH binding so this
; harness can stay filesystem-free.

; png-target produces a target value.
(def t (png-target 4 4))
(assert (target? t))
(assert (not (target? nil)))
(assert (not (target? "not a target")))
(assert (not (target? 42)))

; Wrappers preserve target? truth. offset-target and progress-target
; both produce new targets that share the underlying buffer with the
; wrapped target.
(def t-off (offset-target t 1 2))
(assert (target? t-off))

(def t-prog (progress-target t 4 "phase3-test"))
(assert (target? t-prog))

; Wrappers compose: nested wrappers stay valid targets.
(def t-nested (offset-target t-off 0 0))
(assert (target? t-nested))

; A trivial scene rendered into the target. 4×4 keeps the test cheap;
; reflect-limit 0 and a fixed 1 sample per pixel (min == max == 1)
; minimize work. The resulting buffer isn't introspected here —
; `render` returning a target and not panicking is the assertion.
(def red (surface {:color [1 0 0] :ambient 0.5 :light 0.5}))
(def cam (camera-looking-at [0 0 5] [0 0 0] [0 1 0] 1.0))
(def s (scene {:name "phase3-render"
               :camera cam
               :background [0 0 0]
               :objects [(light-white [10 10 10])
                         (sphere {:center [0 0 0] :r 1.0 :surface red})]
               :reflect-limit 0
               :min-samples 1
               :max-samples 1}))

; render returns its target so calls can be chained or threaded.
(def returned (render s t 4 4))
(assert (target? returned))

; Identity preservation is structural for most host types but pointer
; identity for targets — render should return the same target Rc, so
; equality holds.
(assert= returned t)

; Re-rendering into the same target is fine — submit_row tolerates
; repeat writes (idempotent, last-write-wins).
(render s t 4 4)
