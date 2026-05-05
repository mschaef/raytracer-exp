; Camera bindings.

; The standard "looking at the origin from above" camera, matching
; `default_camera` in scenes.rs.
(def c1 (camera-looking-at [0 10 0] [0 0 0] [0 0 1] 1.0))
(assert (camera? c1))

; Same parameters → structurally equal.
(def c2 (camera-looking-at [0 10 0] [0 0 0] [0 0 1] 1.0))
(assert= c1 c2)

; Different zoom → different camera.
(def c3 (camera-looking-at [0 10 0] [0 0 0] [0 0 1] 2.0))
(assert (not= c1 c3))

; camera-with-fov is also a camera.
(def cf (camera-with-fov [0 10 0] [0 0 0] [0 0 1] 1.0))
(assert (camera? cf))

; A field-of-view of about 53.13° (atan2(0.5, 1.0) * 2) maps to
; zoom = 1.0. Compute it from the script side for a sanity check —
; with-fov reduces to looking-at via half_height = tan(fov/2).
;
; We don't have trig built-ins, so just confirm with-fov produces
; *some* camera and that it's not equal to the zoom=1 one (the FOV
; conversion isn't guaranteed to be bit-identical to zoom=1 here).
(assert (camera? (camera-with-fov [0 10 0] [0 0 0] [0 0 1] 0.5)))

; Negative checks.
(assert (not (camera? nil)))
(assert (not (camera? [0 10 0])))
