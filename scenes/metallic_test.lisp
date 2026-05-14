; SDL scene exercising the basic metallic surface flag.
;
; Three metal spheres — gold, silver, copper — on the reflective
; checker ground. The :metallic flag tells the renderer to tint each
; sphere's mirror reflection and specular highlight by its body color
; and to suppress the diffuse term, so the spheres take their
; appearance from what they reflect (each other, the checker floor)
; rather than from a flat diffuse body color. Contrast with a plain
; (glossy ...) sphere, which is lit primarily by its diffuse term.
;
; reflect-limit is bumped to 3 so the metal-to-metal reflections
; between the three spheres resolve a couple of bounces deep.
;
; Camera framing note: default-camera looks down -y from [0 10 0],
; so the three spheres at y = 0 are spread horizontally across the
; frame on the checker plane.

(load "_common.lisp")

(def metallic-test-scene
  (scene
    {:name          "Metallic Test"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :reflect-limit 3
     :objects
     [(light-white [10 10 10])
      (sphere {:center [-2 0 -1] :r 0.66 :surface surface-gold})
      (sphere {:center [ 0 0 -1] :r 0.66 :surface surface-silver})
      (sphere {:center [ 2 0 -1] :r 0.66 :surface surface-copper})
      (plane  {:normal [0 0 1]   :p0 [0 0 -2] :surface surface-white-c})]}))
