; Helpers for scenes ported from POV-Ray.
;
; Loaded by the ported scenes via (load "_pov.lisp"). Like _common.lisp,
; the underscore prefix means "not a standalone scene". See
; docs/povray_gap_analysis.md for the porting conventions. In short:
;
;   * Coordinates carry over unchanged. POV-Ray is left-handed (+x
;     right, +y up, +z away from a camera looking down +z), and so is
;     `camera-looking-at` with up hint [0 1 0]: it puts +x on the right.
;     pov_compass.lisp is the check.
;   * POV `rotate` angles are degrees about the same axes, with the same
;     sense: `rotate z*18` is (rotate-z (deg->rad 18) ...).
;   * POV applies an object's transforms in the order written, so
;     `object { X translate A rotate B }` is (rotate-B (translate-A X)).
;   * Shading uses this renderer's own model. The surfaces below are
;     starting points that borrow POV's numbers loosely, to be tuned by
;     eye, not a translation of POV's finish model.
;   * Colours are sRGB. The POV scenes predate POV-Ray 3.7 and set no
;     `assumed_gamma`, so POV wrote their colour numbers straight to the
;     image: they're display values. This renderer computes in linear
;     light and sRGB-encodes its output, so a POV colour has to be
;     decoded with `srgb` before the renderer sees it.
;
;     Convention: colours in these files are written as POV's numbers,
;     and the helpers here decode them (`pov-plain`, the `pov-metal-*`
;     surfaces, `pov-pigmented` for every colour in a pigment and its
;     layers, and the lights). Anything a scene hands the renderer
;     directly, such as a `surface` :color, a light :color or a
;     :background, must be wrapped in `srgb`. Black and white are the
;     same either way. (Snowman is different: it sets `assumed_gamma
;     1.0`, so its colours are already linear.)

;; --------------------------------------------------------------------
;; sRGB decoding
;; --------------------------------------------------------------------

; One sRGB-encoded channel to linear (the IEC 61966-2-1 curve, the
; inverse of the renderer's output encoding).
(defn srgb-channel [c]
  (if (<= c 0.04045)
    (/ c 12.92)
    (pow (/ (+ c 0.055) 1.055) 2.4)))

; An sRGB colour, [r g b] or [r g b t], to linear. A transmit t isn't a
; colour and passes through unchanged. POV-Ray 3.7's `srgb` keyword does
; the same.
(defn srgb [c]
  (let [rgb [(srgb-channel (nth c 0)) (srgb-channel (nth c 1)) (srgb-channel (nth c 2))]]
    (if (= (count c) 4) (conj rgb (nth c 3)) rgb)))

; A pigment map with every colour decoded: :color, :colors and the
; colours of :color-map (whose positions stay as they are).
(defn srgb-pigment-layer [layer]
  (let [layer (if (get layer :color) (assoc layer :color (srgb (get layer :color))) layer)
        layer (if (get layer :colors) (assoc layer :colors (map srgb (get layer :colors))) layer)]
    (if (get layer :color-map)
      (assoc layer :color-map (map (fn [[v c]] [v (srgb c)]) (get layer :color-map)))
      layer)))

; A pigment, one map or a vector of layers, with every colour decoded.
(defn srgb-pigment [pigment]
  (if (map? pigment)
    (srgb-pigment-layer pigment)
    (map srgb-pigment-layer pigment)))

;; --------------------------------------------------------------------
;; Colours (colors.inc)
;; --------------------------------------------------------------------

(def pov-white [1.0 1.0 1.0])
(def pov-black [0.0 0.0 0.0])
(def pov-red   [1.0 0.0 0.0])
(def pov-green [0.0 1.0 0.0])
(def pov-blue  [0.0 0.0 1.0])
(def pov-yellow [1.0 1.0 0.0])

;; These are POV's numbers (sRGB); the surface helpers decode them.

;; metals.inc and golds.inc pigments.
(def pov-gold3   [1.0 0.775 0.375])   ; P_Gold3
(def pov-silver3 [0.94 0.93 0.90])    ; P_Silver3
(def pov-brass3  [0.58 0.42 0.20])    ; P_Brass3

;; --------------------------------------------------------------------
;; Geometry
;; --------------------------------------------------------------------

; POV `box { <a>, <b> }`: an axis-aligned box given by two opposite
; corners, in either order. Returns an unsurfaced cuboid; wrap it in
; (with-surface ...) or use it as a CSG operand.
(defn box [a b]
  (cuboid {:center (p* (p+ a b) 0.5)
           :size   [(abs (- (x b) (x a)))
                    (abs (- (y b) (y a)))
                    (abs (- (z b) (z a)))]}))

; The arrow from the POV projects' `makeArrow` macro: a cylinder shaft
; of radius `diameter` from `start`, ending in a cone head of radius
; 1.5 * diameter * arrow-scale and length 3 * diameter * arrow-scale
; whose tip is at `end`. Unsurfaced.
(defn pov-arrow [start end diameter arrow-scale]
  (let [dir      (p- end start)
        head-len (* diameter arrow-scale 3)
        mid      (p- end (p* dir (/ head-len (magnitude dir))))]
    (group [(cylinder {:p0 start :p1 mid :r diameter})
            (cone     {:p0 mid :p1 end :r (* diameter arrow-scale 1.5)})])))

;; --------------------------------------------------------------------
;; Camera
;; --------------------------------------------------------------------

; POV's default camera (`direction 1*z`, `up y`) has the same vertical
; field of view as zoom 1.0 here (about 53°). A `direction k*z` camera
; is zoom k.
(defn pov-camera [location look-at]
  (camera-looking-at location look-at [0 1 0] 1.0))

;; --------------------------------------------------------------------
;; Surfaces
;; --------------------------------------------------------------------

; A procedural pigment (see `surface`'s :pigment key: a pigment map, or
; a vector of layers) with POV's default finish.
(defn pov-pigmented [pigment]
  (surface {:pigment (srgb-pigment pigment) :ambient 0.1 :light 0.6 :specular 0.0}))

;; POV colour maps are lists of two-colour entries, [v0 v1 c0 c1]: the
;; colour runs from c0 at v0 to c1 at v1. Where one entry's c1 differs
;; from the next entry's c0 the map steps, which a repeated value
;; reproduces here. Colours may be [r g b t], with a transmit t.
(defn pov-color-map [entries]
  (mapcat (fn [[v0 v1 c0 c1]] [[v0 c0] [v1 c1]]) entries))

;; POV transforms written in POV's order, e.g.
;; [[:scale [0.15 0.5 1]] [:rotate [5 10 5]] [:translate [-2 0 0]]],
;; composed into one affine. A :rotate is in degrees and turns about x,
;; then y, then z, as POV's does.
(defn pov-transform [steps]
  (reduce (fn [acc [op v]]
            (affine-compose
              (cond (= op :scale)     (affine-scale v)
                    (= op :translate) (affine-translation v)
                    (= op :rotate)    (affine-compose
                                        (affine-rotation-z (deg->rad (z v)))
                                        (affine-compose (affine-rotation-y (deg->rad (y v)))
                                                        (affine-rotation-x (deg->rad (x v))))))
              acc))
          (affine-identity)
          steps))

;; --------------------------------------------------------------------
;; woods.inc and woodmaps.inc
;; --------------------------------------------------------------------
;
; A T_Wood texture is two layers: an opaque grain underneath and a
; partly transmitting grain on top, which adds streaks and darker late
; wood. Here each T_Wood is a pigment layer vector, bottom first, for
; `surface`'s :pigment (see `pov-pigmented`).

; P_WoodGrain1A, the bottom grain of most T_Wood textures.
(defn pov-wood-grain-1a [color-map]
  {:pattern    :wood
   :turbulence 0.04
   :octaves    3
   :color-map  color-map
   :transform  (affine-scale [0.05 0.05 1])})

; P_WoodGrain1B, the top grain over P_WoodGrain1A: coarser, tilted
; rings, off the object's axis.
(defn pov-wood-grain-1b [color-map]
  {:pattern    :wood
   :turbulence [0.1 0.5 1]
   :octaves    5
   :lambda     3.25
   :color-map  color-map
   :transform  (pov-transform [[:scale [0.15 0.5 1]] [:rotate [5 10 5]] [:translate [-2 0 0]]])})

; P_WoodGrain7A (T_Wood7's bottom), whose turbulence differs per axis.
(defn pov-wood-grain-7a [color-map]
  {:pattern    :wood
   :turbulence [0.05 0.08 1000]
   :octaves    4
   :color-map  color-map
   :transform  (affine-scale [0.15 0.15 1])})

; P_WoodGrain7B (T_Wood7's top): fine noise streaks along z.
(defn pov-wood-grain-7b [color-map]
  {:pattern   :bozo
   :color-map color-map
   :transform (affine-scale [0.01 0.01 100000])})

; M_Wood7A, which woodmaps.inc repeats as M_Wood13A: yellow pine.
(def pov-m-wood-7a
  (let [a [0.60 0.35 0.20]
        b [0.90 0.65 0.30]]
    (pov-color-map [[0.0 0.1 a a] [0.1 0.9 a b] [0.9 1.0 b a]])))

; M_Wood7B: opaque yellow streaks fading to clear.
(def pov-m-wood-7b
  (let [y  [0.90 0.65 0.30 0.00]
        y3 [0.90 0.65 0.30 0.30]
        clear [1.0 1.0 1.0 1.0]]
    (pov-color-map [[0.0 0.1 y y3] [0.1 1.0 y3 clear]])))

; M_Wood13B (the active one of the two in woodmaps.inc).
(def pov-m-wood-13b
  (let [y  [0.90 0.65 0.30 0.00]
        y3 [0.90 0.65 0.30 0.30]
        clear [1.0 1.0 1.0 1.0]]
    (pov-color-map [[0.0 0.4 clear y3] [0.4 0.5 y y3] [0.5 1.0 y3 clear]])))

; M_Wood15A, with its two-colour entries (which join end to end)
; flattened to single colours.
(def pov-m-wood-15a
  (let [a (p* [0.504 0.310 0.078] 0.7)
        b (p* [0.531 0.325 0.090] 0.8)
        c (p* [0.547 0.333 0.090] 0.5)
        d (p* [0.504 0.310 0.075] 0.6)
        e (p* [0.559 0.322 0.102] 0.4)
        f (p* [0.531 0.325 0.086] 0.4)]
    [[0.0 a] [0.25 b] [0.40 c] [0.50 d] [0.70 e] [0.98 f] [1.0 a]]))

; M_Wood15B.
(def pov-m-wood-15b
  (pov-color-map [[0.00 0.25 [0.404 0.210 0.078 0.20] [0.431 0.225 0.090 0.80]]
                  [0.25 0.40 [0.431 0.225 0.090 0.80] [0.447 0.233 0.090 0.20]]
                  [0.40 0.50 [0.447 0.233 0.090 0.20] [0.404 0.210 0.075 0.60]]
                  [0.50 0.70 [0.404 0.210 0.075 0.60] [0.459 0.222 0.102 0.20]]
                  [0.70 0.98 [0.459 0.222 0.102 0.20] [0.431 0.225 0.086 0.40]]
                  [0.98 1.00 [0.431 0.225 0.086 0.40] [0.404 0.210 0.078 0.10]]]))

; M_Wood18A: orange, with dark late-wood bands.
(def pov-m-wood-18a
  (let [o50 [1.0 0.50 0.0]
        o45 [1.0 0.45 0.0]
        o40 [1.0 0.40 0.0]
        o36 [1.0 0.36 0.0]]
    (pov-color-map [[0.00 0.15 o50 (p* o50 0.5)]
                    [0.15 0.25 (p* o50 0.5) (p* o45 0.7)]
                    [0.25 0.28 (p* o45 0.8) (p* o36 0.3)]
                    [0.28 0.40 (p* o36 0.3) (p* o40 0.4)]
                    [0.40 0.50 (p* o40 0.4) (p* o40 0.6)]
                    [0.50 0.70 (p* o50 0.6) (p* o50 0.7)]
                    [0.70 0.98 (p* o45 0.7) (p* o45 0.5)]
                    [0.98 1.00 (p* o45 0.5) o50]])))

; M_Wood18B.
(def pov-m-wood-18b
  (pov-color-map [[0.00 0.25 [0.50 0.26 0.12 0.30] [0.54 0.29 0.13 0.40]]
                  [0.25 0.40 [0.54 0.29 0.13 0.40] [0.55 0.28 0.10 0.60]]
                  [0.40 0.50 [0.55 0.28 0.10 0.60] [0.50 0.23 0.15 1.00]]
                  [0.50 0.70 [0.50 0.23 0.15 1.00] [0.56 0.29 0.17 0.60]]
                  [0.70 0.98 [0.56 0.29 0.17 0.60] [0.54 0.29 0.13 0.40]]
                  [0.98 1.00 [0.54 0.29 0.13 0.40] [0.50 0.26 0.12 0.30]]]))

; The textures: pigment layer vectors, bottom first.
(def pov-t-wood7  [(pov-wood-grain-7a pov-m-wood-7a) (pov-wood-grain-7b pov-m-wood-7b)])  ; yellow pine, ragged grain
(def pov-t-wood23 [(pov-wood-grain-1a pov-m-wood-7a) (pov-wood-grain-1b pov-m-wood-13b)]) ; M_Wood13A is M_Wood7A
(def pov-t-wood25 [(pov-wood-grain-1a pov-m-wood-15a) (pov-wood-grain-1b pov-m-wood-15b)])
(def pov-t-wood28 [(pov-wood-grain-1a pov-m-wood-18a) (pov-wood-grain-1b pov-m-wood-18b)])

; textures.inc's Dark_Wood: coarse (unscaled) rings with a hard step.
(def pov-dark-wood-pigment
  {:pattern    :wood
   :turbulence 0.2
   :color-map  [[0.8 [0.43 0.24 0.05]] [0.8 [0.40 0.33 0.06]] [1.0 [0.20 0.03 0.03]]]})

; textures.inc's Chrome_Texture: grey, ambient 0.3, diffuse 0.7,
; reflection 0.15, specular 0.8.
(def pov-chrome
  (surface {:color (srgb [0.658824 0.658824 0.658824])
            :ambient 0.3 :light 0.7 :specular 0.8 :reflection 0.15}))

; glass_old.inc's T_Glass4: rgbf <0.98, 1, 0.99, 0.75> with F_Glass4
; (ambient 0.1, diffuse 0.1, reflection 0.25, specular 1). POV's filter
; tints what shows through, and this renderer's transparency doesn't,
; but at this near-white colour the difference is slight. With no
; interior (no ior) POV doesn't refract it either.
(def pov-glass4
  (surface {:color (srgb [0.98 1.0 0.99])
            :ambient 0.1 :light 0.1 :specular 1.0 :reflection 0.25
            :transparency 0.75}))

; A plain POV pigment with POV's default finish (ambient 0.1, diffuse
; 0.6, no highlight), plus an optional specular strength.
(defn pov-plain
  [color]
  (surface {:color (srgb color) :ambient 0.1 :light 0.6 :specular 0.0}))

(defn pov-plain-specular [color specular]
  (surface {:color (srgb color) :ambient 0.1 :light 0.6 :specular specular}))

; Starting points for metals.inc's F_MetalA ("very soft and dull"),
; F_MetalC ("medium reflectivity, holds color well") and F_MetalE
; (below), using their
; ambient, diffuse (:light), specular and reflection numbers. These
; are *not* flagged :metallic: this renderer's metallic model drops the
; diffuse term, and POV's metal finishes keep theirs. Switch to the
; `metallic` helper in _common.lisp if a harder metal look is wanted.
(defn pov-metal-a [color]
  (surface {:color (srgb color) :ambient 0.35 :light 0.3 :specular 0.8 :reflection 0.1}))

(defn pov-metal-c [color]
  (surface {:color (srgb color) :ambient 0.25 :light 0.5 :specular 0.8 :reflection 0.5}))

; F_MetalE, "very highly polished & reflective".
(defn pov-metal-e [color]
  (surface {:color (srgb color) :ambient 0.1 :light 0.7 :specular 0.8 :reflection 0.8}))

;; --------------------------------------------------------------------
;; The compass (makeCompass)
;; --------------------------------------------------------------------

; The red/green/blue arrow compass from the POV projects (xmastree,
; braids, train): a black ball at the origin with arrows along +x (red),
; +y (green) and +z (blue), each running from -1 to 1.
(defn pov-compass-arrow [start end color]
  (with-surface (pov-plain-specular color 0.3) (pov-arrow start end 0.05 1.5)))

(def pov-compass
  (group [(sphere {:center [0 0 0] :r 0.1 :surface (pov-plain-specular pov-black 0.3)})
          (pov-compass-arrow [-1 0 0] [1 0 0] pov-red)
          (pov-compass-arrow [0 -1 0] [0 1 0] pov-green)
          (pov-compass-arrow [0 0 -1] [0 0 1] pov-blue)]))

;; --------------------------------------------------------------------
;; The xmastree harness
;; --------------------------------------------------------------------
;
; xmastree.pov, braids.pov and train.pov share their lights and
; backdrop: they began as copies of one file.

; The two lights: a shadowless Gray60 fill light overhead, and a
; White*1.5 spotlight at <0,5,0> + 30 aimed at <0,5,0>, radius 20° and
; falloff 45° (both half-angles). At gDetail > 1 the spotlight is also a
; 6x6 area light (POV's area_light <6,0,0>, <0,6,0>, which lies in the
; xy plane whatever the spotlight's direction); pass `area?` for that.
(defn xmas-lights [area?]
  (let [spot {:location    [30 35 30]
              :point-at    [0 5 0]
              :inner-angle (deg->rad 20)
              :outer-angle (deg->rad 45)
              :intensity   1.5}]
    [(light {:location [0 100 0] :color (srgb [0.6 0.6 0.6]) :shadowless true})
     (light (if area? (assoc spot :area-u [6 0 0] :area-v [0 6 0]) spot))]))

; The white ground plane. The white sky_sphere and the white hollow
; sphere of radius 2000 around everything become a white :background.
(def xmas-ground
  (plane {:normal [0 1 0] :p0 [0 0 0] :surface (pov-plain pov-white)}))
