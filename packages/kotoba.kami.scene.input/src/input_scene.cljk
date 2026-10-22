(ns input-scene
  "KAMI Input Scene — EDN authoring surface for `input`'s default
  device→action input-binding maps (fps/graph). Restored from the
  legacy kami-engine/kami-input-scene Rust crate (deleted in
  kotoba-lang/kami-engine PR #82 'Remove Rust workspace from
  kami-engine') as part of the clj-wgsl migration (ADR-2607010930,
  com-junkawasaki/root).

  This is the data-tier counterpart of `kami-input` for the input
  system, the same way `kami-scene`-authored `scene.edn` is the
  data-tier counterpart of a whole `kami-clj` game: it turns canonical
  `:input/maps` EDN (named, *ordered* tables of `[key-code action]`
  pairs) into the real `input/input-map` binding-map data structure,
  the same way `input`'s hardcoded `default-fps-map`/`default-graph-map`
  presets build it. It re-uses `scene`'s tolerant EDN accessors
  (`kw-key`/`mget`/`root-map`) the same way games parse `scene.edn`
  (namespaced keywords match on `ns/name`, malformed entries are
  skipped).

  ## Why this is safe (ADR-0038)

  Hot per-frame input resolution (`input/resolve-action`, gesture
  detection, focus routing — all in `kotoba-lang/input`) stays pure
  CLJC as-is. A default binding map is init-time CONFIG — the
  device→action table read once when an app sets up its input handler
  — so it is safe to author as EDN. `kotoba-lang/input` itself stays
  untouched; the EDN dependency lives only here. The compiled-in
  `input/default-fps-map` / `input/default-graph-map` builders remain
  the `builtin-input-map` fallback and are parity-tested against the
  shipped EDN (`input-edn`).

  ## EDN shape (see `resources/input.edn`)

      {:input/maps
       {:fps   [[\"KeyW\" :move-up] [\"ArrowUp\" :move-up] ... [\"Escape\" :pause]]
        :graph [[\"KeyW\" :move-up] ... [\"Minus\" :zoom-out] [\"NumpadSubtract\" :zoom-out]]}}

  Each map is an **ordered** vector of `[key-code action-keyword]`
  pairs (order matters — `resolve-action` is first-match). `key-code`
  is the W3C `KeyboardEvent.code` string; `action` is a hyphenated
  action keyword id (`:move-up` / `:zoom-in` / ...) drawn from
  `input/actions`.

  Unlike the original (`include_str!` embedding a file at Rust compile
  time), this namespace inlines the EDN as a plain string constant
  (`input-edn`) — the CLJC-portable equivalent, working identically
  under both Clojure and ClojureScript with no filesystem/classpath
  resource loading. The byte-identical source also ships at
  `resources/input.edn` for provenance/tooling.

  Depends on `kotoba-lang/scene` (tolerant EDN accessors) and
  `kotoba-lang/input` (the `actions` vocabulary and the
  `default-fps-map`/`default-graph-map` parity oracle). Zero
  additional runtime dependencies beyond those two sibling CLJC
  crates."
  (:require [scene :as scene]
            [input :as input]))

;; ── Shipped EDN config ───────────────────────────────────────────

(def input-edn
  "The canonical default input-binding CONFIG shipped with this crate.
  This is the source of truth; the compiled-in preset fns
  (`input/default-fps-map`, `input/default-graph-map`) are the
  parity-tested mirror. CLJC-portable equivalent of the original's
  `include_str!(\"../data/input.edn\")`."
  ";; Canonical default input-binding maps for kami-input (ADR-0040 data tier:\n;; \"input: device→action maps as EDN\").\n;;\n;; Each entry mirrors one `kami_input::InputMap::{default_fps,default_graph}()`\n;; builder. A map is an ORDERED vector of `[key-code action]` pairs — order\n;; matters, because `InputMap::resolve` is first-match. These are parity-tested\n;; == the real Rust (every (key, action), in order) in tests/input_parity.rs.\n;;\n;; key-code is the W3C KeyboardEvent.code string (e.g. \"KeyW\", \"ArrowUp\",\n;; \"Escape\"). action is an Action keyword id (hyphenated): :move-up /\n;; :move-down / :move-left / :move-right / :zoom-in / :zoom-out / :pan-start /\n;; :pan-end / :pan-move / :primary / :secondary / :cancel / :confirm / :jump /\n;; :sprint / :interact / :attack / :pause / :reset / :menu / :fullscreen.\n\n{:input/maps\n {:fps   [[\"KeyW\" :move-up]      [\"ArrowUp\" :move-up]\n          [\"KeyS\" :move-down]    [\"ArrowDown\" :move-down]\n          [\"KeyA\" :move-left]    [\"ArrowLeft\" :move-left]\n          [\"KeyD\" :move-right]   [\"ArrowRight\" :move-right]\n          [\"Space\" :jump]        [\"ShiftLeft\" :sprint]\n          [\"KeyE\" :interact]     [\"Escape\" :pause]]\n\n  :graph [[\"KeyW\" :move-up]      [\"ArrowUp\" :move-up]\n          [\"KeyS\" :move-down]    [\"ArrowDown\" :move-down]\n          [\"KeyA\" :move-left]    [\"ArrowLeft\" :move-left]\n          [\"KeyD\" :move-right]   [\"ArrowRight\" :move-right]\n          [\"Equal\" :zoom-in]     [\"NumpadAdd\" :zoom-in]\n          [\"Minus\" :zoom-out]    [\"NumpadSubtract\" :zoom-out]]}}\n")

(def all-map-names
  "Names of the input maps shipped as the compiled-in oracle (iteration
  source for `builtin-input-map`/parity). Kept here (not in `input`)
  to keep the domain namespace untouched. Order mirrors the original
  `impl InputMap` declaration order."
  ["fps" "graph"])

;; ── Errors ───────────────────────────────────────────────────────
;; Rust's `#[derive(thiserror::Error)]` `Error` enum, ported as
;; `ex-info` constructors distinguished by `:type` in ex-data.

(defn ex-not-a-map
  "The EDN source did not parse to a top-level map."
  []
  (ex-info "input EDN root is not a map" {:type ::not-a-map}))

(defn ex-no-table
  "The `:input/maps` table was missing or not a map."
  []
  (ex-info "`:input/maps` missing or not a map" {:type ::no-table}))

(defn ex-map-not-found
  "The requested map id was missing under `:input/maps`."
  [name]
  (ex-info (str "input map `" name "` not found under `:input/maps`")
           {:type ::map-not-found :name name}))

(defn ex-unknown-action
  "A binding referenced an unknown action keyword id."
  [id]
  (ex-info (str "unknown action `" id "`") {:type ::unknown-action :action id}))

;; ── Action id <-> keyword ────────────────────────────────────────

(defn id-from-action
  "The hyphenated keyword id string for an action keyword `a` (e.g.
  `:move-up` -> \"move-up\"), or nil if `a` isn't a known
  `input/actions` member. Inverse of `action-from-id`."
  [a]
  (when (contains? input/actions a)
    (name a)))

(defn action-from-id
  "Parse an action keyword from its hyphenated keyword id string `id`.
  Unknown ids yield nil (the loader turns that into
  `ex-unknown-action`) — unlike the tolerant scalar fallbacks
  elsewhere, an unrecognised action is a hard error so a typo never
  silently drops a binding."
  [id]
  (let [k (keyword id)]
    (when (contains? input/actions k)
      k)))

;; ── Binding-map construction ─────────────────────────────────────

(defn input-map-from-pairs
  "Build one real `input/input-map` from an ordered vector of
  `[key-code action]` pairs (the EDN value of one `:input/maps`
  entry).

  The bindings are rebuilt **in order** — `input/resolve-action` is
  first-match, so order is load-bearing. The key-code is read as a
  string; the action is read as a keyword and resolved via
  `action-from-id` (an unknown action id is a hard
  `ex-unknown-action`). A pair that is malformed in *shape* (not a
  2-element vector / non-string key / non-keyword action) is skipped,
  matching how the rest of the data tier degrades on shape errors."
  [pairs]
  (input/input-map
   (into []
         (keep (fn [pair]
                 (when (and (vector? pair) (= 2 (count pair)))
                   (let [[code act] pair]
                     (when (string? code)
                       ;; action: a keyword id. Shape error -> skip;
                       ;; unknown id -> hard error.
                       (when-let [id (scene/kw-key act)]
                         (if-let [action (action-from-id id)]
                           [code action]
                           (throw (ex-unknown-action id)))))))))
         pairs)))

(defn builtin-input-map
  "The compiled-in fallback / parity oracle: the real
  `input/default-fps-map` / `input/default-graph-map`. Returns nil for
  an unknown name. This is what the shipped EDN is parity-tested
  against."
  [name]
  (case name
    "fps" (input/default-fps-map)
    "graph" (input/default-graph-map)
    nil))

(defn input-maps-from-edn
  "Parse the whole `:input/maps` table from EDN `src` into a map keyed
  by the map id, each value the rebuilt `input/input-map` (bindings in
  order)."
  [src]
  (let [root (or (scene/root-map src) (throw (ex-not-a-map)))
        table (scene/mget root "input/maps")]
    (when-not (map? table) (throw (ex-no-table)))
    (into {}
          (keep (fn [[k v]]
                  (when-let [id (scene/kw-key k)]
                    (when (vector? v)
                      [id (input-map-from-pairs v)]))))
          table)))

(defn input-map-from-edn
  "Look up & rebuild a single input map by `name` from EDN `src`.
  Throws if the table or the named map is absent (or a binding
  references an unknown action)."
  [src name]
  (let [root (or (scene/root-map src) (throw (ex-not-a-map)))
        table (scene/mget root "input/maps")]
    (when-not (map? table) (throw (ex-no-table)))
    (let [pairs (some (fn [[k v]] (when (= (scene/kw-key k) name) v)) table)]
      (when-not (vector? pairs) (throw (ex-map-not-found name)))
      (input-map-from-pairs pairs))))

(defn shipped-input-maps
  "Convenience: load & rebuild all input maps from the crate-shipped
  `input-edn`."
  []
  (input-maps-from-edn input-edn))

(defn shipped-input-map
  "Convenience: load & rebuild one input map from the shipped EDN."
  [name]
  (input-map-from-edn input-edn name))
