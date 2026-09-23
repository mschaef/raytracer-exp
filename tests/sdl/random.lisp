; random and random-gaussian: stateless, counter-based random numbers.
;
; (random seed k1 k2 ...) hashes its integer arguments into a float in
; [0, 1); (random-gaussian seed k1 k2 ...) gives a normal deviate with
; mean 0 and standard deviation 1. The same arguments always give the
; same number, and nothing is mutated.

; Deterministic.
(assert= (random 700) (random 700))
(assert= (random 700 3 4) (random 700 3 4))
(assert= (random-gaussian 700 1) (random-gaussian 700 1))

; Every argument matters, including key order.
(assert (not= (random 700) (random 701)))
(assert (not= (random 700 1) (random 700 2)))
(assert (not= (random 700 1 2) (random 700 2 1)))
(assert (not= (random 700) (random 700 0)))

; Keys can be negative.
(assert (float? (random 700 -1)))

; In range.
(def samples (map (fn [i] (random 42 i)) (range 4000)))
(assert (= 0 (count (filter (fn [u] (or (< u 0) (>= u 1))) samples))))

(defn mean [xs] (/ (reduce + 0.0 xs) (count xs)))
(defn variance [xs]
  (let [m (mean xs)]
    (mean (map (fn [v] (* (- v m) (- v m))) xs))))

; Roughly uniform: mean near 1/2, variance near 1/12, and about a
; tenth of the samples in each tenth of the range.
(assert (< (abs (- (mean samples) 0.5)) 0.02))
(assert (< (abs (- (variance samples) (/ 1.0 12))) 0.005))
(def tenths (map (fn [d] (count (filter (fn [u] (= d (int (* 10 u)))) samples))) (range 10)))
(assert (= 0 (count (filter (fn [n] (or (< n 330) (> n 470))) tenths))))

; Roughly standard normal: mean near 0, variance near 1, and about
; 68% within one standard deviation.
(def normals (map (fn [i] (random-gaussian 42 i)) (range 4000)))
(assert (< (abs (mean normals)) 0.05))
(assert (< (abs (- (variance normals) 1.0)) 0.08))
(def within-one (count (filter (fn [v] (< (abs v) 1.0)) normals)))
(assert (and (> within-one 2600) (< within-one 2860)))

; Neighbouring keys aren't correlated: consecutive pairs have
; covariance near 0.
(def pairs-cov (mean (map (fn [i] (* (- (random 42 i) 0.5) (- (random 42 (+ i 1)) 0.5))) (range 4000))))
(assert (< (abs pairs-cov) 0.005))

; Choosing one of n items.
(defn pick [n & keys] (int (* n (apply random 700 keys))))
(assert (int? (pick 21 1 2)))
(assert (= 0 (count (filter (fn [i] (or (< (pick 21 i) 0) (> (pick 21 i) 20))) (range 1000)))))
