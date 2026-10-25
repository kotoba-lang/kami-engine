(ns kotoba.amenominaka.extension
  "M3 (ADR-2607100100): the Kit extension model (IExt + `extension.toml`
  parser) — ported forward from `kotoba-lang/kami-nv-compat`'s
  `kotoba.lang.kami-nv-compat.amenominaka.extension` (itself a clean-room
  JVM/CLJC mirror of `omni.ext.IExt`, ADR-2605261800 D6/D10.4, Wave 3 of
  the kami-nv-compat TS->CLJC port) into THIS repo — the canonical
  implementation, not the nv-compat facade. ADR-2605261800's own N10
  ('nv-compat namespace内で canonical 機能拡張... 禁止') means substantive
  extension-lifecycle logic belongs here, in `kami-app-amenominaka`
  itself; `kami-nv-compat` should stay a thin import-alias facade over
  this. Logic is unchanged from the ported source (Kahn topo-sort in
  [[kotoba.amenominaka.application]], this namespace's IExt protocol +
  TOML-subset parser) — only the namespace and this docstring differ.

  `extension.edn` (this app's native manifest format, per ADR-2607100100
  D7) needs NO custom reader — it's a plain EDN map with the same shape
  [[parse-extension-toml]] produces (`:dependencies` etc.), parseable
  with `clojure.edn/read-string` directly. [[parse-extension-toml]]
  exists for the literal-`.toml`-compat case only (reading an actual Kit
  `extension.toml` file, e.g. one ported from upstream Omniverse Kit) —
  `kotoba-lang/toml` was considered for this but is EDN->TOML *only* (no
  parser, confirmed by reading its source), so it cannot serve this
  direction; this hand-rolled TOML-subset parser is the real answer.

  NOTE on \"magatama Pregel cell\": `kami-app-amenominaka`'s own
  pre-M0 README draft claimed extensions map onto a 'magatama Pregel
  cell' and attributed this to ADR-2605261800. Re-reading that ADR in
  full (565 lines) finds ZERO occurrences of either word — the
  attribution does not survive the source. \"magatama\" independently
  names three unrelated systems elsewhere in this monorepo (gftdcojp's
  MagatamaApp Cloudflare-Worker convention, an etzhayyim capital-flow
  actor, gftdcojp's pymagatama/keiei daemon) and has no Pregel-flavoured
  technical definition anywhere. The org's real Pregel-cell system is
  `kotodama`'s persistent-daemon catalog (ADR-2605192415) — unrelated to
  extension loading. This namespace and [[kotoba.amenominaka.application]]
  therefore implement extension lifecycle WITHOUT any Pregel-cell
  mapping — that mapping is retired as a concept, not deferred as an
  implementation gap, per ADR-2607100100's M3 addendum."
  (:require [kotoba.lang.text :as str]))

;; ── IExt lifecycle (mirrors omni.ext.IExt) ────────────────────────────────

(defprotocol IExt
  (on-startup [this ext-id] "Called when the Application loads this extension.")
  (on-update  [this dt]     "Called once per app tick (seconds).")
  (on-shutdown [this]       "Called when the Application unloads this extension."))

(defn default-ext
  "A no-op IExt — mirrors `new IExt()` with the abstract class's default hooks."
  []
  (reify IExt
    (on-startup  [_ _])
    (on-update   [_ _])
    (on-shutdown [_])))

;; ── TOML subset parser (literal `extension.toml` compat only — see NOTE above) ──

(defn- strip-comment
  "Drop a trailing `# ...` comment, but keep `#` that appears inside a
  double-quoted string. (An unescaped `\"` toggles in-string.)"
  [line]
  (let [n (count line)]
    (loop [i 0 in-string? false out ""]
      (if (>= i n)
        out
        (let [c    (.charAt line i)
              prev (if (pos? i) (.charAt line (dec i)) \space)
              in-string?' (if (and (= c \") (not= prev \\)) (not in-string?) in-string?)]
          (if (and (= c \#) (not in-string?))
            out
            (recur (inc i) in-string?' (str out c))))))))

(defn- strip-quotes
  "Trim, then strip one leading + trailing double-quote if both present."
  [s]
  (let [s (str/trim s)]
    (if (and (str/starts-with? s "\"") (str/ends-with? s "\"") (> (count s) 1))
      (subs s 1 (dec (count s)))
      s)))

(defn- split-top-level
  "Split `inner` on top-level commas, respecting nested [] / {} depth."
  [inner]
  (loop [chars (seq inner) depth 0 cur "" out []]
    (if-let [ch (first chars)]
      (cond
        (or (= ch \[) (= ch \{)) (recur (rest chars) (inc depth) (str cur ch) out)
        (or (= ch \]) (= ch \})) (recur (rest chars) (dec depth) (str cur ch) out)
        (and (= ch \,) (zero? depth)) (recur (rest chars) depth "" (conj out cur))
        :else (recur (rest chars) depth (str cur ch) out))
      (if (str/blank? cur) out (conj out cur)))))

(declare parse-value)

(defn- parse-num
  "Parse a TOML scalar number; returns nil if `s` is not a number. Integers
  (no '.') become long; decimals stay double — matching TS Number()/Math.trunc.
  (On cljs there is no int/long distinction — every number is a JS double.)"
  [s]
  #?(:clj  (try
             (let [n (Double/parseDouble s)]
               (if (str/includes? s ".") n (long n)))
             (catch Exception _ nil))
     :cljs (let [n (js/parseFloat s)]
             (when-not (js/isNaN n) n))))

(defn- parse-value
  "Parse a TOML scalar / array / inline-table value (string already trimmed)."
  [raw]
  (let [s (str/trim raw)]
    (cond
      (and (str/starts-with? s "\"") (str/ends-with? s "\"") (> (count s) 1))
      (subs s 1 (dec (count s)))

      (and (str/starts-with? s "[") (str/ends-with? s "]"))
      (let [inner (str/trim (subs s 1 (dec (count s))))]
        (if (str/blank? inner) [] (mapv parse-value (split-top-level inner))))

      (and (str/starts-with? s "{") (str/ends-with? s "}"))
      (let [inner (str/trim (subs s 1 (dec (count s))))]
        (if (str/blank? inner)
          {}
          (into {} (for [part (split-top-level inner)
                         :let [eq (str/index-of part "=")]
                         :when (>= eq 0)]
                     [(strip-quotes (subs part 0 eq)) (parse-value (subs part (inc eq)))]))))

      (= s "true")  true
      (= s "false") false
      :else         (or (parse-num s) s))))

(defn- ensure-table-in
  "Ensure the map at `path` (vector of string keys) in m exists and is a map;
  assoc-in creates intermediate levels as maps. Returns updated m."
  [m path]
  (let [p (vec path)]
    (cond-> m
      (and (seq p) (not (map? (get-in m p)))) (assoc-in p {}))))

(defn- ensure-array-of-tables-in
  "Ensure the array-of-tables at `path` exists (appending a fresh {}), and
  return [new-root new-current-path] where current-path includes the new
  element's index."
  [m path]
  (let [p      (vec path)
        parent (vec (butlast p))
        k      (last p)
        m      (if (seq parent) (ensure-table-in m parent) m)
        base   (if (seq parent) (get-in m parent) m)
        arr    (if (seq parent) (get base k) (get m k))
        arr'   (if (vector? arr) arr [])
        idx    (count arr')
        arr''  (conj arr' {})
        m'     (if (seq parent) (assoc-in m (conj parent k) arr'') (assoc m k arr''))]
    [m' (conj p idx)]))

(defn parse-extension-toml
  "Parse the subset of TOML used by Kit extension manifests. Returns a map with
  :title :version :description :category :keywords :authors :repository
  :dependencies :python-modules :raw-tables."
  [text]
  (let [[root _current-path]
        (loop [lines (str/split text #"\n") root {} current-path []]
          (if-let [raw-line (first lines)]
            (let [line (-> raw-line strip-comment str/trim)]
              (if (str/blank? line)
                (recur (rest lines) root current-path)
                (cond
                  (and (str/starts-with? line "[[") (str/includes? line "]]"))
                  (let [name (-> line (subs 2 (str/index-of line "]]")) str/trim)
                        path (str/split name #"\.")
                        [root' new-path] (ensure-array-of-tables-in root path)]
                    (recur (rest lines) root' new-path))

                  (and (str/starts-with? line "[") (str/includes? line "]"))
                  (let [name (-> line (subs 1 (str/index-of line "]")) str/trim)
                        path (str/split name #"\.")]
                    (recur (rest lines) (ensure-table-in root path) path))

                  :else
                  (let [eq (str/index-of line "=")]
                    (if (neg? eq)
                      (recur (rest lines) root current-path)
                      (let [k (strip-quotes (subs line 0 eq))
                            v (parse-value (subs line (inc eq)))]
                        (recur (rest lines)
                               (assoc-in root (conj current-path k) v)
                               current-path)))))))
            [root current-path]))
        pkg (or (get root "package") {})
        py  (get root "python")
        modules (if (vector? (get py "module")) (get py "module") [])]
    {:title          (str (get pkg "title" ""))
     :version        (str (get pkg "version" "0.1.0"))
     :description    (str (get pkg "description" ""))
     :category       (str (get pkg "category" ""))
     :keywords       (or (get pkg "keywords") [])
     :authors        (or (get pkg "authors") [])
     :repository     (str (get pkg "repository" ""))
     :dependencies   (or (get root "dependencies") {})
     :python-modules modules
     :raw-tables     root}))
