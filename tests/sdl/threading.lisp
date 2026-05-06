; Phase 4 — threading macros: -> (thread first) and ->> (thread last).
;
; Implemented as special forms in eval.rs (no macros in the SDL), but
; the script-visible behavior matches Clojure's `->` / `->>` macros.

;; --------------------------------------------------------------------
;; -> (thread first)
;; --------------------------------------------------------------------

; Trivial: a single value with no steps just returns the value.
(assert= (-> 5) 5)

; Bare-symbol step: (-> x f) ≡ (f x).
(def inc (fn [n] (+ n 1)))
(assert= (-> 5 inc) 6)

; Multi-step bare-symbol: each step calls the function on the previous
; result.
(assert= (-> 5 inc inc inc) 8)

; List step: (-> x (f a b)) ≡ (f x a b). The threaded value lands in
; the *first* slot.
(assert= (-> 10 (- 3)) 7)        ; (- 10 3)
(assert= (-> 10 (- 3) (- 2)) 5)  ; (- (- 10 3) 2)

; Mixing bare-symbol and list steps.
(assert= (-> 5 inc (* 3)) 18)    ; (* (inc 5) 3) = (* 6 3)

;; --------------------------------------------------------------------
;; ->> (thread last)
;; --------------------------------------------------------------------

(assert= (->> 5) 5)

; Bare-symbol step is identical to ->: (->> x f) ≡ (f x).
(assert= (->> 5 inc) 6)

; List step: (->> x (f a b)) ≡ (f a b x). The threaded value lands in
; the *last* slot.
(assert= (->> 10 (- 3)) -7)      ; (- 3 10)
(assert= (->> 10 (- 3) (- 2)) 9) ; (- 2 (- 3 10)) = (- 2 -7)

;; --------------------------------------------------------------------
;; Threading composes naturally with the standard HOFs (whose
;; "collection" argument is conventionally last, which is exactly why
;; ->> exists).
;; --------------------------------------------------------------------

(assert= (->> [1 2 3 4 5]
              (filter (fn [n] (> n 2)))
              (map    (fn [n] (* n 10))))
         [30 40 50])

(assert= (-> [1 2 3]
             (conj 4)
             (conj 5))
         [1 2 3 4 5])
