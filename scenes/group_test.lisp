; SDL port of scenes.rs::scene_group_test.
;
; Demonstrates `Shape::Group` as a hierarchical container — visually
; identical to a flat scene with the same primitives (grouping has no
; rendering effect on its own), but sets up the tree structure that
; transforms hang off of. The nested-group case (a sphere alongside
; an inner group of two smaller spheres) confirms recursion through
; Group::hit_test works to arbitrary depth.

(load "_common.lisp")

(def group-test-scene
  (scene
    {:name          "Group Test"
     :camera        default-camera
     :background    [0.0 0.0 0.0]
     :reflect-limit 2
     :oversample    2
     :objects
     [(light-white [10 10 10])
      ;; A "snowman": three stacked spheres treated as a single
      ;; child of the scene.
      (group [(sphere {:center [-2 0 -1]   :r 0.6 :surface surface-white})
              (sphere {:center [-2 0  0]   :r 0.5 :surface surface-white})
              (sphere {:center [-2 0  0.8] :r 0.4 :surface surface-white})])

      ;; A row of three cubes, also grouped, to confirm the same
      ;; mechanism works for boxes and that nearest-hit is correct
      ;; when groups contain different primitive types.
      (group [(cuboid {:center [1 0 -1] :size [0.6 0.6 0.6] :surface surface-red})
              (cuboid {:center [2 0 -1] :size [0.6 0.6 0.6] :surface surface-green})
              (cuboid {:center [3 0 -1] :size [0.6 0.6 0.6] :surface surface-blue})])

      ;; A nested group: a sphere alongside an inner group of two
      ;; smaller spheres. Confirms recursion through Group::hit_test
      ;; works to arbitrary depth.
      (group [(sphere {:center [0 -3 -1] :r 0.7 :surface surface-purple})
              (group [(sphere {:center [-0.7 -3 0.2] :r 0.3 :surface surface-orange})
                      (sphere {:center [ 0.7 -3 0.2] :r 0.3 :surface surface-yellow})])])

      ;; A ground plane outside any group, to confirm flat and
      ;; grouped objects coexist correctly in the same scene.
      (plane {:normal [0 0 1] :p0 [0 0 -2] :surface surface-white-c})]}))
