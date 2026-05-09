; Fixture for tests/sdl/load_form.lisp.
;
; Doubles as a standalone test (every .lisp script in tests/sdl/ has
; to be a valid test on its own per the suite's discovery model) —
; the assert= calls below pass whether this file is run via the
; suite directly or loaded into another script's environment by
; (load "load_form_fixture.lisp"). When loaded, the def's become
; visible in the loading env, which is what load_form.lisp checks.

(def fixture-x 42)
(def fixture-y "hello")

(assert= fixture-x 42)
(assert= fixture-y "hello")

; Last form is the script's "value" — captured by load_form.lisp via
; (def loaded-result (load ...)). Tests both that load returns
; something and that it returns the right thing.
fixture-x
