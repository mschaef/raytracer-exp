; SDL port of scenes.rs::scene_ball_on_plane.
;
; The minimal sanity-check scene: a single blue sphere sitting on
; the reflective checker ground. Useful as a fast smoke test of the
; whole rendering pipeline.

(load "_common.lisp")

(def ball-on-plane-scene
  (scene
    {:name          "Ball on Plane"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :reflect-limit 2
     :objects
     [(light-white [10 10 10])
      (sphere {:center [0 -2 -1] :r 0.66 :surface surface-blue})
      (plane  {:normal [0 0 1]   :p0 [0 0 -2] :surface surface-white-c})]}))
