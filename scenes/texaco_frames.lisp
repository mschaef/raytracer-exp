; Renders the Texaco logo animation from the POV original: the star
; turns about y from 0° to -180° over 24 frames (POV: `rotate y*clock`,
; +KI0 +KF-180). Not a scene definition: run it with the sdl_run binary,
; which evaluates the script for its side effects:
;
;   cargo run --release --bin sdl_run -- scenes/texaco_frames.lisp
;
; Writes texaco00.png ... texaco23.png to the current directory. The POV
; build script turned the frames into an animated GIF with netpbm; any
; GIF or video tool will do the same here.

(load "texaco.lisp")

(def frame-count 24)
(def width 320)
(def height 240)

(defn pad2 [n] (if (< n 10) (str "0" n) (str n)))

(defn render-frame [f]
  (let [angle (/ (* -180.0 f) frame-count)
        path  (str "texaco" (pad2 f) ".png")
        t     (progress-target (png-target width height) height path)]
    (render (texaco-at angle false) t width height)
    (save-png t path)))

(map render-frame (range frame-count))

; Evaluate to nil so sdl_run has nothing to print.
nil
