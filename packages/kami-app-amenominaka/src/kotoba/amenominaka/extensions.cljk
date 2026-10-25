(ns kotoba.amenominaka.extensions
  "M3 (ADR-2607100100): registers M0's scene composition
  ([[kotoba.amenominaka.scene]]) and M2's render-IR bridge
  ([[kotoba.amenominaka.render-ir]]) as `extension.edn`-manifested,
  IExt-lifecycle extensions loaded through
  [[kotoba.amenominaka.application]] — this is the actual new work M3
  needs (the loader/lifecycle machinery itself is ported forward, see
  that namespace's docstring).

  `extension.edn` manifests are authored as plain EDN data below, not as
  separate files on disk — this repo is a single app, not a multi-
  package plugin directory a loader scans at runtime, so there's no real
  filesystem-discovery use case yet to build for (see the namespace
  docstring on [[kotoba.amenominaka.extension]] for why the format needs
  no custom reader either way — it's the same shape
  `parse-extension-toml` produces, plain EDN maps read with
  `clojure.edn/read-string`).

  Each extension's `on-startup`/`on-shutdown` only logs — there's no
  meaningful runtime state to initialize for a pure-function library
  (`compose` / `scene->render-ir` take all their input as arguments, not
  from extension-startup side effects). The real, tested value M3 adds
  is the depends_on-ordered startup/shutdown lifecycle and the
  manifests themselves, not fabricated side effects — consumers still
  `:require` [[kotoba.amenominaka.scene]] / [[kotoba.amenominaka.render-ir]]
  directly to actually call `compose`/`scene->render-ir`; this namespace
  doesn't route capability access through the extension registry (that
  would be scope beyond what M3 asks for)."
  (:require [kotoba.amenominaka.application :as app]
            [kotoba.amenominaka.extension :as ext]))

(def scene-manifest
  {:title "kotoba.amenominaka.scene"
   :version "0.1.0"
   :description "Compose a bim building + kami-*-scene environment presets into one scene EDN (M0)."
   :dependencies {}})

(def render-ir-manifest
  {:title "kotoba.amenominaka.render-ir"
   :version "0.1.0"
   :description "Bridge a composed scene into kami.webgpu.ir render-IR for a browser walkthrough (M2)."
   :dependencies {"scene" {}}})

(defn- logging-ext
  "An IExt that only records its lifecycle transitions into `log` (an
  atom holding a vector) — see the namespace docstring for why there's
  no further state to initialize. `id` is closed over rather than read
  from `on-shutdown`'s args, since (unlike `on-startup`) `IExt/on-shutdown`
  doesn't receive the extension id."
  [log id]
  (reify ext/IExt
    (on-startup [_ ext-id] (swap! log conj [:startup ext-id]))
    (on-update [_ _dt])
    (on-shutdown [_] (swap! log conj [:shutdown id]))))

(defn load!
  "Register the scene (M0) and render-ir (M2) extensions on a fresh
  [[kotoba.amenominaka.application/application]] and start them in
  dependency order (scene before render-ir, since render-ir's manifest
  depends on scene's). Returns `{:app :order :log}` — `:order` is
  `startup-all`'s own return (the ids in the order they started),
  `:log` a vector of `[:startup id]`/`[:shutdown id]` lifecycle events."
  []
  (let [log (atom [])
        application (app/application)]
    (app/register-extension! application "scene" (logging-ext log "scene") scene-manifest)
    (app/register-extension! application "render-ir" (logging-ext log "render-ir") render-ir-manifest)
    {:app application :order (app/startup-all application) :log @log}))
