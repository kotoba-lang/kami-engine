(ns kami.wgsl
  "L2 — WGSL shaders authored as Clojure data → WGSL source string.

  Keeps shader authoring in clj (composable, testable, diffable) WITHOUT
  re-implementing a GPU: the emitted WGSL is registered with `kami-render` via the
  additive `kami:engine/frame.register-shader` WIT call (ARCHITECTURE.md §8).
  Built-in pipelines need no WGSL — they reuse kami-render's shipped shaders.

  Shader-as-data shape (see §8a):
    {:wgsl/name \"ripple\"
     :wgsl/bindings [{:group 0 :binding 0 :var :uniform :name \"u\" :type :Globals}]
     :wgsl/structs  {:Globals [[:time :f32] [:mvp :mat4x4<f32>]]}
     :wgsl/vertex   {:in [[:pos :vec3<f32> {:location 0}]]
                     :out [[:clip :vec4<f32> :builtin/position]]
                     :body '[(set! out.clip (* u.mvp (vec4 in.pos 1.0)))]}
     :wgsl/fragment {:out [[:color :vec4<f32> {:location 0}]]
                     :body '[(set! out.color (vec4 0.3 0.6 1.0 1.0))]}}

  The body sub-language is a small s-expression subset; unsupported forms throw
  so the author can fall back to a raw WGSL string (scope: ADR-CLJ §13.2)."
  (:require [clojure.string :as str]))

(def builtin-pipelines
  "Pipeline keywords that map directly to kami-render::scene_pipelines — no WGSL
  emission needed; `:draw/pipeline` just names one."
  #{:pbr :sky :terrain :vegetation :character :water :voxel :particle :atlas})

(defn builtin?
  "True if `pipeline` is a built-in kami-render pipeline (skip WGSL emission)."
  [pipeline]
  (contains? builtin-pipelines pipeline))

;; ---------------------------------------------------------------------------
;; Expression + statement lowering (small subset)
;; ---------------------------------------------------------------------------

(defn- emit-num [n]
  (let [d (double n)]
    (if (== d (Math/floor d))
      (str (long d) ".0")
      (str d))))

(def ^:private binops {'+ "+" '- "-" '* "*" '/ "/"})

(declare emit-expr)

(defn- emit-call [form]
  (let [[op & args] form]
    (cond
      (contains? binops op)
      (str "(" (str/join (str " " (binops op) " ") (map emit-expr args)) ")")

      ('#{vec2 vec3 vec4 mat3 mat4} op)
      (let [ty ({'vec2 "vec2<f32>" 'vec3 "vec3<f32>" 'vec4 "vec4<f32>"
                 'mat3 "mat3x3<f32>" 'mat4 "mat4x4<f32>"} op)]
        (str ty "(" (str/join ", " (map emit-expr args)) ")"))

      :else ; generic WGSL function call: (normalize n) → normalize(n)
      (str (name op) "(" (str/join ", " (map emit-expr args)) ")"))))

(defn- emit-expr [form]
  (cond
    (number? form) (emit-num form)
    (symbol? form) (name form)            ; dotted access (in.pos, u.mvp) preserved
    (string? form) form                    ; raw WGSL escape hatch
    (seq? form)    (emit-call form)
    (list? form)   (emit-call form)
    :else (throw (ex-info "wgsl/emit-expr: unsupported form" {:form form :type (type form)}))))

(defn- emit-stmt [form]
  (cond
    (and (seq? form) (= 'set! (first form)))
    (str "  " (emit-expr (nth form 1)) " = " (emit-expr (nth form 2)) ";")

    (and (seq? form) (= 'let (first form)))
    (let [[_ bindings & body] form]
      (str (->> (partition 2 bindings)
                (map (fn [[s v]] (str "  let " (name s) " = " (emit-expr v) ";")))
                (str/join "\n"))
           "\n"
           (str/join "\n" (map emit-stmt body))))

    :else (str "  " (emit-expr form) ";")))

;; ---------------------------------------------------------------------------
;; Struct + binding + stage emission
;; ---------------------------------------------------------------------------

(defn emit-struct
  "Emit one `struct Name { … }` block from [[:field :type] …]."
  [sname fields]
  (str "struct " (name sname) " {\n"
       (->> fields
            (map (fn [[f t]] (str "  " (name f) ": " (name t) ",")))
            (str/join "\n"))
       "\n};\n"))

(defn- emit-binding [{:keys [group binding var name type]}]
  (let [kind (clojure.core/name (clojure.core/or var :uniform))]
    (str "@group(" group ") @binding(" binding ") var<" kind "> "
         name ": " (clojure.core/name type) ";\n")))

(defn- attr-of [decoration]
  (cond
    (= decoration :builtin/position) "@builtin(position) "
    (and (map? decoration) (:location decoration)) (str "@location(" (:location decoration) ") ")
    (and (map? decoration) (:builtin decoration)) (str "@builtin(" (name (:builtin decoration)) ") ")
    :else ""))

(defn- emit-io-struct [sname io]
  (str "struct " sname " {\n"
       (->> io
            (map (fn [[fname ftype deco]]
                   (str "  " (attr-of deco) (name fname) ": " (name ftype) ",")))
            (str/join "\n"))
       "\n};\n"))

(defn emit-stage
  "Emit a @vertex or @fragment entry point. Returns a string with the In/Out
  structs and the entry fn. `stage-kind` is :vertex or :fragment."
  [stage-kind {:keys [in out body]}]
  (let [tag    (name stage-kind)
        In     (str (str/capitalize tag) "In")
        Out    (str (str/capitalize tag) "Out")
        in-s   (when (seq in) (emit-io-struct In in))
        out-s  (emit-io-struct Out out)]
    (str in-s out-s
         "@" tag "\n"
         "fn " tag "_main(" (when (seq in) (str "in: " In)) ") -> " Out " {\n"
         "  var out: " Out ";\n"
         (str/join "\n" (map emit-stmt body)) "\n"
         "  return out;\n"
         "}\n")))

(defn emit
  "Compile a shader-as-data map to a complete WGSL source string. Pure. The output
  + a bind-group `layout` descriptor go to `kami.gpu/register-shader!`."
  [{:keys [:wgsl/name :wgsl/bindings :wgsl/structs :wgsl/vertex :wgsl/fragment]}]
  (str "// kami.wgsl emitted shader: " name "\n"
       (apply str (map (fn [[n fs]] (emit-struct n fs)) structs))
       (apply str (map emit-binding bindings))
       (when vertex (emit-stage :vertex vertex))
       (when fragment (emit-stage :fragment fragment))))
