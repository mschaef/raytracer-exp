; if and do.

(assert= (if true 1 2) 1)
(assert= (if false 1 2) 2)
(assert= (if nil 1 2) 2)
(assert= (if 0 1 2) 1)        ; 0 is truthy

; if without else returns nil on false.
(assert= (if false 1) nil)
(assert= (if true 1) 1)

; do evaluates each form in order, returns the last.
(assert= (do 1 2 3) 3)
(assert= (do) nil)

; do is useful for side-effecting sequences in test setup; the
; intermediate values are discarded.
(assert= (do (+ 1 2) (+ 3 4)) 7)

; if branches are evaluated lazily.
(assert= (if true :ok (/ 1 0)) :ok)
(assert= (if false (/ 1 0) :ok) :ok)

; ----------------------------------------------------------------------
; Phase 4: when, when-not, cond
; ----------------------------------------------------------------------

; when returns the last body form when the test is truthy, nil
; otherwise.
(assert= (when true 1 2 3) 3)
(assert= (when false 1 2 3) nil)
(assert= (when nil :unreachable) nil)
(assert= (when 0 :truthy) :truthy)        ; 0 is truthy

; when with no body returns nil even when test is truthy.
(assert= (when true) nil)

; when-not is the mirror.
(assert= (when-not false 1 2 3) 3)
(assert= (when-not true :unreachable) nil)
(assert= (when-not nil :ok) :ok)
(assert= (when-not 0 :unreachable) nil)   ; 0 is truthy → no body

; when/when-not bodies are lazy: no evaluation when the gate is closed.
(assert= (when false (/ 1 0)) nil)
(assert= (when-not true (/ 1 0)) nil)

; cond: pairs of test/expr. First truthy test wins; nil if none match.
(assert= (cond) nil)
(assert= (cond true 1) 1)
(assert= (cond false 1 true 2 false 3) 2)
(assert= (cond false 1 nil 2) nil)

; :else is just a truthy keyword — nothing magic about the name.
(assert= (cond
            false :a
            false :b
            :else :default)
         :default)

; cond evaluates only the matching branch.
(assert= (cond true :ok false (/ 1 0)) :ok)
(assert= (cond
            false (/ 1 0)
            true  :ok
            :else (/ 1 0))
         :ok)
