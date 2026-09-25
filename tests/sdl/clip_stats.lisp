; `clip-stats`: how much of what's been written to a png-target clipped
; in the 8-bit encode (phase 1 of the view transform plan in CLAUDE.md).
;
; The scenes here are just a background, so every pixel has the same
; known colour.

(defn flat-scene [background]
  (scene {:name "clip-stats"
          :camera (camera-looking-at [0 0 5] [0 0 0] [0 1 0] 1.0)
          :background background
          :objects []
          :min-samples 1
          :max-samples 1}))

; Nothing written yet.
(def t (png-target 4 2))
(assert= (clip-stats t) {:pixels 0 :clipped 0 :red 0 :green 0 :blue 0 :max 0.0})

; An in-range background clips nothing; :max is its brightest channel.
(render (flat-scene [0.25 0.5 0.75]) t 4 2)
(assert= (clip-stats t) {:pixels 8 :clipped 0 :red 0 :green 0 :blue 0 :max 0.75})

; Exactly 1.0 isn't clipping.
(def t1 (png-target 2 2))
(render (flat-scene [1 1 1]) t1 2 2)
(assert= (get (clip-stats t1) :clipped) 0)

; An over-bright red background clips every pixel, in red only.
(def t2 (png-target 3 2))
(render (flat-scene [2.5 0.5 0]) t2 3 2)
(assert= (clip-stats t2) {:pixels 6 :clipped 6 :red 6 :green 0 :blue 0 :max 2.5})

; Rendering through a wrapper counts into the same buffer, so
; compositing a second render adds to the totals.
(render (flat-scene [0.1 1.5 3.0]) (offset-target t2 0 0) 3 2)
(def after (clip-stats t2))
(assert= (get after :pixels) 12)
(assert= (get after :clipped) 12)
(assert= [(get after :red) (get after :green) (get after :blue)] [6 6 6])
(assert= (get after :max) 3.0)
