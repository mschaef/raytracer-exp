; Handedness check for POV-Ray ports.
;
; The red/green/blue arrow compass from the POV projects (xmastree,
; braids, train), in POV's own coordinates: a black ball at the origin
; with arrows along +x (red), +y (green) and +z (blue), seen from a
; camera in front of the origin at -z. In POV-Ray this renders with red
; pointing right, green pointing up, and blue pointing away from the
; camera. If this scene renders the same way, POV coordinates and
; rotations can be ported unchanged (see _pov.lisp). The compass
; itself is `pov-compass` in _pov.lisp.

(load "_pov.lisp")

(def pov-compass-scene
  (scene
    {:name       "POV Compass"
     :camera     (pov-camera [1.5 1.2 -3] [0 0 0])
     :background (srgb [0.3 0.3 0.3])
     :objects    [(light-white [5 10 -10])
                  pov-compass]}))
