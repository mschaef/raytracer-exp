; `defn` — sugar for `(def name (fn name [params] body...))`, expanded
; by the desugaring pass (src/sdl/desugar.rs) before evaluation.

; Basic definition.
(defn square [x] (* x x))
(assert= (square 5) 25)
(assert (fn? square))

; Equivalent to the long-hand def + fn form.
(def square-longhand (fn [x] (* x x)))
(assert= (square 7) (square-longhand 7))

; The function value carries the defn name.
(assert= (str square) "#<fn square>")

; Like def, defn evaluates to nil.
(assert= (defn ignored [] 1) nil)
(assert= (ignored) 1)

; Zero-arg function.
(defn forty-two [] 42)
(assert= (forty-two) 42)

; Multi-form body is an implicit do; the last form's value is returned.
(defn add-and-double [a b]
  (def __defn-side-effect (+ a b))
  (* 2 (+ a b)))
(assert= (add-and-double 3 4) 14)

; An optional docstring before the parameter vector is accepted and
; ignored.
(defn cube "Returns x cubed." [x] (* x x x))
(assert= (cube 3) 27)

; A string after the parameter vector is an ordinary body form.
(defn greeting [] "hello")
(assert= (greeting) "hello")

; Rest args and destructuring work exactly as they do in fn.
(defn variadic [a & xs] [a xs])
(assert= (variadic 1) [1 []])
(assert= (variadic 1 2 3) [1 [2 3]])
(defn dot [[ax ay az] [bx by bz]] (+ (* ax bx) (* ay by) (* az bz)))
(assert= (dot [1 2 3] [4 5 6]) 32)

; Self-recursion through the global binding, and recur.
(defn fact [n] (if (<= n 1) 1 (* n (fact (- n 1)))))
(assert= (fact 5) 120)
(defn sum-to [n acc] (if (= n 0) acc (recur (- n 1) (+ acc n))))
(assert= (sum-to 100 0) 5050)

; Closures capture the defining environment.
(defn make-adder [n] (fn [x] (+ x n)))
(def add5 (make-adder 5))
(assert= (add5 10) 15)

; defn is expanded anywhere in a form, not just at top level. Like
; def, a nested defn binds in the environment where it's evaluated.
(defn outer [x]
  (defn inner-helper [y] (* y 10))
  (inner-helper x))
(assert= (outer 4) 40)

; Sugar in a defn body is expanded as well.
(defn make-squarer []
  (defn local-square [x] (* x x))
  local-square)
(assert= ((make-squarer) 9) 81)

; Quoted data is not desugared: this is a list of symbols.
(def quoted '(defn not-defined [x] x))
(assert (vector? quoted))
(assert= (first quoted) 'defn)
(assert= (count quoted) 4)
