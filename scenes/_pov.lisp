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
;     eye, not a translation of POV's finish model. Colours are used as
;     written, with no gamma conversion.

;; --------------------------------------------------------------------
;; Colours (colors.inc)
;; --------------------------------------------------------------------

(def pov-white [1.0 1.0 1.0])
(def pov-black [0.0 0.0 0.0])
(def pov-red   [1.0 0.0 0.0])
(def pov-green [0.0 1.0 0.0])
(def pov-blue  [0.0 0.0 1.0])
(def pov-yellow [1.0 1.0 0.0])

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

; A procedural pigment (see `surface`'s :pigment key) with POV's
; default finish.
(defn pov-pigmented [pigment]
  (surface {:pigment pigment :ambient 0.1 :light 0.6 :specular 0.0}))

;; woods.inc. T_Wood25 is two wood layers, the top one partly
;; transparent (P_WoodGrain1A with M_Wood15A under P_WoodGrain1B with
;; M_Wood15B). Layered textures aren't supported, so this is the bottom
;; layer alone. The colour map is woodmaps.inc's M_Wood15A, whose
;; two-colour entries join end to end, flattened to single colours.
(def pov-t-wood25-pigment
  (let [a (p* [0.504 0.310 0.078] 0.7)
        b (p* [0.531 0.325 0.090] 0.8)
        c (p* [0.547 0.333 0.090] 0.5)
        d (p* [0.504 0.310 0.075] 0.6)
        e (p* [0.559 0.322 0.102] 0.4)
        f (p* [0.531 0.325 0.086] 0.4)]
    ; P_WoodGrain1A: wood, turbulence 0.04, octaves 3, scale <0.05, 0.05, 1>.
    {:pattern    :wood
     :turbulence 0.04
     :octaves    3
     :color-map  [[0.0 a] [0.25 b] [0.40 c] [0.50 d] [0.70 e] [0.98 f] [1.0 a]]
     :transform  (affine-scale [0.05 0.05 1])}))

; A plain POV pigment with POV's default finish (ambient 0.1, diffuse
; 0.6, no highlight), plus an optional specular strength.
(defn pov-plain
  [color]
  (surface {:color color :ambient 0.1 :light 0.6 :specular 0.0}))

(defn pov-plain-specular [color specular]
  (surface {:color color :ambient 0.1 :light 0.6 :specular specular}))

; Starting points for metals.inc's F_MetalA ("very soft and dull"),
; F_MetalC ("medium reflectivity, holds color well") and F_MetalE
; (below), using their
; ambient, diffuse (:light), specular and reflection numbers. These
; are *not* flagged :metallic: this renderer's metallic model drops the
; diffuse term, and POV's metal finishes keep theirs. Switch to the
; `metallic` helper in _common.lisp if a harder metal look is wanted.
(defn pov-metal-a [color]
  (surface {:color color :ambient 0.35 :light 0.3 :specular 0.8 :reflection 0.1}))

(defn pov-metal-c [color]
  (surface {:color color :ambient 0.25 :light 0.5 :specular 0.8 :reflection 0.5}))

; F_MetalE, "very highly polished & reflective".
(defn pov-metal-e [color]
  (surface {:color color :ambient 0.1 :light 0.7 :specular 0.8 :reflection 0.8}))

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
    [(light {:location [0 100 0] :color [0.6 0.6 0.6] :shadowless true})
     (light (if area? (assoc spot :area-u [6 0 0] :area-v [0 6 0]) spot))]))

; The white ground plane. The white sky_sphere and the white hollow
; sphere of radius 2000 around everything become a white :background.
(def xmas-ground
  (plane {:normal [0 1 0] :p0 [0 0 0] :surface (pov-plain pov-white)}))
