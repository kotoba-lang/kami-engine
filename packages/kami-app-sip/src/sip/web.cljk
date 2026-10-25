(ns sip.web
  "Browser runtime entry for Spirit in Physics.

  Boot order: fetch the portable snapshot → cook its procedural assets into real
  geometry → make the `kami.backend.browser` GPU backend (which drives the
  `kami-clj-host` wasm → kami-render → WebGPU) → load the snapshot into an ECS
  world and run the fixed-step loop with the ambient `sip.systems`.

  index.html instantiates the wasm and sets `window.KamiCljHost` before calling
  `boot`; `kami.backend.browser/make` uses that global by default and returns a
  core.async channel yielding the backend (async adapter/device request)."
  (:require [kami.sim :as sim]
            [kami.backend.browser :as browser]
            [sip.systems :as systems]          ; side-effect: registers defsystems
            [sip.assets :as assets]
            [cljs.reader :as reader]
            [cljs.core.async :refer [<!]])
  (:require-macros [cljs.core.async.macros :refer [go]]))

(defn- fetch-edn [url]
  (-> (js/fetch url)
      (.then #(.text %))
      (.then reader/read-string)))

(defn ^:export start
  "Boot with a parsed, already-cooked snapshot. `host-ctor` is the wasm
  `KamiCljHost` class (defaults to `js/KamiCljHost`). Returns a channel that
  yields the `sim/run!` handle once the GPU device is ready."
  [canvas-id snapshot host-ctor]
  (go
    (let [backend (<! (browser/make (cond-> {:canvas canvas-id}
                                      host-ctor (assoc :host-ctor host-ctor))))]
      (sim/run! {:canvas   canvas-id
                 :snapshot snapshot
                 :backend  backend
                 :systems  systems/all
                 :hz       30}))))

(defn ^:export boot
  "One-call boot from index.html: load + cook /snapshot.edn, then `start` using
  the global `window.KamiCljHost` (set by index.html after wasm init)."
  [canvas-id snapshot-url]
  (-> (fetch-edn snapshot-url)
      (.then (fn [snap]
               (let [cooked (assets/cook-snapshot snap)]
                 (js/console.log "Spirit in Physics — snapshot loaded:"
                                 (count (:snapshot/entities cooked)) "entities,"
                                 (count (:snapshot/assets cooked)) "assets")
                 (start canvas-id cooked nil))))
      (.catch (fn [e] (js/console.error "boot failed" e) (throw e)))))
