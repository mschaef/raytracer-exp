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

; A plain POV pigment with POV's default finish (ambient 0.1, diffuse
; 0.6, no highlight), plus an optional specular strength.
(defn pov-plain
  [color]
  (surface {:color color :ambient 0.1 :light 0.6 :specular 0.0}))

(defn pov-plain-specular [color specular]
  (surface {:color color :ambient 0.1 :light 0.6 :specular specular}))

; Starting points for metals.inc's F_MetalA ("very soft and dull") and
; F_MetalC ("medium reflectivity, holds color well"), using their
; ambient, diffuse (:light), specular and reflection numbers. These
; are *not* flagged :metallic: this renderer's metallic model drops the
; diffuse term, and POV's metal finishes keep theirs. Switch to the
; `metallic` helper in _common.lisp if a harder metal look is wanted.
(defn pov-metal-a [color]
  (surface {:color color :ambient 0.35 :light 0.3 :specular 0.8 :reflection 0.1}))

(defn pov-metal-c [color]
  (surface {:color color :ambient 0.25 :light 0.5 :specular 0.8 :reflection 0.5}))
