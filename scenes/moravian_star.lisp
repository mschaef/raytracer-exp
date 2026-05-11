; A "Moravian-style" star built as a triangle mesh: an inner cube
; with a 4-sided pyramid extending from each of its six faces. Six
; points total, 24 triangles in all. Each pyramid contributes 4
; lateral triangles (its base is shared with the cube face and is
; therefore not rendered); the cube edges become ridges where
; adjacent pyramids meet, so the surface is closed without the
; cube faces ever appearing as such.
;
; The classic 26-point Moravian star is built the same way on a
; rhombicuboctahedron base — this is the same construction with
; a cube as the simplest convex polyhedron one can stellate.

(load "_common.lisp")

;; --------------------------------------------------------------------
;; Mesh helpers
;; --------------------------------------------------------------------

;; Build the four triangular sides of one pyramid. The base corners
;; `a`, `b`, `c`, `d` must be supplied in CCW order as viewed from
;; *outside* the base; with that convention, the resulting triangles
;; (a,b,apex), (b,c,apex), (c,d,apex), (d,a,apex) all wind so their
;; geometric face normals point outward. Triangles are constructed
;; without :normals, so they take the flat geometric face normal —
;; appropriate for a faceted star where every side is meant to look
;; planar.
(def pyramid-sides
  (fn [a b c d apex surface]
    (group [(triangle {:vertices [a b apex] :surface surface})
            (triangle {:vertices [b c apex] :surface surface})
            (triangle {:vertices [c d apex] :surface surface})
            (triangle {:vertices [d a apex] :surface surface})])))

;; --------------------------------------------------------------------
;; Moravian star
;; --------------------------------------------------------------------

;; Six-pointed "spiked cube" star.
;;
;;   center  : [x y z]  centre of the star in world space.
;;   size    : number   distance from `center` to each spike apex
;;                      (along its cardinal axis).
;;   inner   : number   half-side of the inner cube. Smaller values
;;                      give longer, sharper spikes; values near
;;                      `size` give a blunt jack-like shape.
;;   surface : Surface  applied to all 24 triangles.
;;
;; Returns a Shape::Group of 24 triangles. Composes with `bounded`,
;; the transform constructors, and `group` like any other shape.
(def moravian-star
  (fn [center size inner surface]
    (let [s inner
          ;; Eight cube corners. Names encode signs on each axis:
          ;; 'p' = +, 'm' = -, in x-y-z order.
          cppp (p+ center (point    s     s     s))
          cppm (p+ center (point    s     s  (- s)))
          cpmp (p+ center (point    s  (- s)    s))
          cpmm (p+ center (point    s  (- s) (- s)))
          cmpp (p+ center (point (- s)    s     s))
          cmpm (p+ center (point (- s)    s  (- s)))
          cmmp (p+ center (point (- s) (- s)    s))
          cmmm (p+ center (point (- s) (- s) (- s)))
          ;; Six spike apexes, one per face direction.
          apx (p+ center (point    size      0      0))
          amx (p+ center (point (- size)     0      0))
          apy (p+ center (point       0   size      0))
          amy (p+ center (point       0  (- size)   0))
          apz (p+ center (point       0      0   size))
          amz (p+ center (point       0      0  (- size)))]
      ;; Six pyramids, one per cube face. The four base corners in each
      ;; call are listed CCW as viewed from outside the cube face, so
      ;; pyramid-sides ends up with every triangle's normal pointing
      ;; outward. Comments name the face by its outward normal.
      (group
        [(pyramid-sides cpmp cpmm cppm cppp apx surface)   ; +X face
         (pyramid-sides cmmm cmmp cmpp cmpm amx surface)   ; -X face
         (pyramid-sides cppm cmpm cmpp cppp apy surface)   ; +Y face
         (pyramid-sides cmmm cpmm cpmp cmmp amy surface)   ; -Y face
         (pyramid-sides cmmp cpmp cppp cmpp apz surface)   ; +Z face
         (pyramid-sides cmpm cppm cpmm cmmm amz surface)]))))   ; -Z face

;; --------------------------------------------------------------------
;; Demo scene
;; --------------------------------------------------------------------

;; A warm-gold surface in the spirit of a hanging Herrnhut star. A
;; touch of reflection picks up the checker ground and helps the
;; spikes read as solid faceted geometry.
(def surface-gold
  (surface {:color      [1.0 0.75 0.15]
            :ambient    ambient
            :specular   specular
            :light      light
            :checked    false
            :reflection 0.15}))

;; Custom camera: 3/4 view from above and to the side, so two or three
;; of the side spikes are visible alongside the top and front ones.
;; The default-camera from _common.lisp looks straight down -Y, which
;; would line three of the spikes up with each other.
(def moravian-star-camera
  (camera-looking-at [4 8 3] [0 0 0] [0 0 1] 1.0))

(def moravian-star-scene
  (scene
    {:name          "Moravian Star"
     :camera        moravian-star-camera
     :background    [0.0 0.0 0.0]
     :lights        [;; Key light: warm white, above and to the right.
                     (light-white [10 10 10])
                     ;; Fill light: cool blue from the opposite side,
                     ;; lower intensity. The blue tint plays nicely
                     ;; off the gold star, and the two lights give
                     ;; visible specular highlights on different spikes.
                     (light-point [-8 4 6] [0.5 0.6 1.0] 0.5)]
     :reflect-limit 2
     :oversample    2
     :objects
     [;; The star itself. Bounded so a ray that misses the world-space
      ;; AABB skips every per-triangle test — a small win at 24
      ;; triangles, but the right idiom for any mesh-shaped object
      ;; and the pattern teapot.lisp uses too.
      (bounded (moravian-star [0 0 0] 1.5 0.4 surface-gold))

      ;; Reflective checker ground, same as ball_on_plane et al.
      (plane {:normal [0 0 1] :p0 [0 0 -2] :surface surface-white-c})]}))
