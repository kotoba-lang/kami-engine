(ns kami.app-suji.ui
  "Mount, state and the event loop. Everything renderable lives in
  `kami.app-suji.core` as pure functions of a state map; this namespace holds the
  one atom, the fragment listener and the canvas lifecycle.

  Recomputation is synchronous and total: a slider move rebuilds the body, the
  pose, the loads, the tensions and the scene from scratch. That is affordable
  because `suji` is pure arithmetic over six segments — there is no solver to warm
  and no state to invalidate — and it means the picture and the numbers are always
  the same instant's answer."
  (:require [kami.app-suji.core :as core]
            [kami.app-suji.route :as route]
            [kami.app-suji.viewport :as viewport]
            [reagent.core :as r]
            [reagent.dom.client :as rdomc]))

(defonce app-state (r/atom core/initial-state))
(defonce root (atom nil))

(defn- apply-preset [state preset-name]
  ;; The table lives in `core/presets` — pure and JVM-testable — so the buttons
  ;; the panel renders and the postures this sets cannot drift apart. This used to
  ;; search `posture/reference-workstations` directly, which meant a preset added
  ;; to the panel would render as a button that did nothing when clicked.
  (if-let [p (core/preset-posture preset-name)]
    (assoc state :posture p)
    state))

(defn- set-path [state path v]
  (-> (assoc-in state path v)
      ;; a hand-moved slider is no longer any preset; saying it still is would be a
      ;; label that quietly stops being true
      (cond-> (= (take 1 path) [:posture]) (update :posture dissoc :preset))))

(defn- on-input [e]
  (let [t (.-target e)]
    (when-let [p (.getAttribute t "data-path")]
      (let [path (mapv keyword (.split p "/"))
            v (js/parseFloat (.-value t))]
        (when-not (js/isNaN v) (swap! app-state set-path path v))))))

(defn- on-click [e]
  (when-let [preset (some-> (.-target e) (.closest "[data-preset]")
                            (.getAttribute "data-preset"))]
    (swap! app-state apply-preset preset)))

(defn- sync-route! []
  (swap! app-state assoc :view (:id (route/fragment->view (.-hash js/location)))))

(defonce ^:private binding-canvas? (atom false))

(defn- ensure-canvas!
  "Bind the canvas if it is present and not bound yet.

  This runs after EVERY render rather than once at startup. React 18's
  `createRoot().render()` commits asynchronously, so the canvas does not exist
  yet when `init!` returns; a single `requestAnimationFrame` after mount raced it
  and lost, and because the attempt was never repeated the viewport stayed
  uninitialised for the life of the page — with no error anywhere, since
  `when-let` on a missing element simply does nothing. (Measured 2026-09-06: the
  canvas kept its default 300x150 backing store and the page reported no
  backend.) It also handles the canvas going away and coming back when the reader
  leaves the simulator view and returns.

  `binding-canvas?` guards against starting a second init while the first
  promise is still in flight — after-render fires again before it resolves.

  ⚠ IT DID NOT ACTUALLY HANDLE THE CANVAS COMING BACK, and the paragraph above
  said it did. The test was `(nil? (:viewport @viewport/state))`, which is false
  after the first init forever — so crossing to `#/spine` and returning left the
  GL context bound to the DESTROYED canvas element while a brand-new one rendered
  nothing.

  Measured 2026-09-09 in headless Chromium: after one round trip the new canvas'
  backing store was 300x150 — the HTML default — inside a CSS box of 708x531, and
  the picture was a single colour. `mesh/sync-canvas-size!` could not save it
  either: it returns early when the canvas' CSS box already matches the viewport
  map's `:width`/`:height`, which it does, because the new element is laid out at
  exactly the size the old one was. So nothing resized it and nothing drew on it.
  Nothing raised, nothing logged; the numbers went on updating beside a dead
  picture, which is this repo's oldest failure mode.

  The bound element is remembered and compared by IDENTITY. Comparing sizes would
  reproduce the bug — the new canvas is the same size as the old one, which is the
  whole reason the size test failed."
  []
  (when-let [canvas (.getElementById js/document "suji-canvas")]
    (when (and (not (identical? canvas (:canvas @viewport/state)))
               (not @binding-canvas?))
      (reset! binding-canvas? true)
      (viewport/init! canvas
                      (fn [backend]
                        (reset! binding-canvas? false)
                        (swap! app-state assoc :backend backend)
                        (viewport/render! (:scene (core/solved @app-state))))
                      (fn [msg]
                        (reset! binding-canvas? false)
                        (swap! app-state assoc :backend :unavailable :gpu-error msg)
                        (js/console.error "kami-app-suji: no GPU backend —" msg))))))

(defn root-view []
  (let [state @app-state]
    ;; Draw after React has committed this render, so the canvas exists and has
    ;; its laid-out size. Reading the scene here keeps the frame and the numbers
    ;; on the same state value.
    (r/after-render (fn []
                      (ensure-canvas!)
                      (viewport/render! (:scene (core/solved state)))))
    (core/app state)))

(defn ^:export init! []
  (sync-route!)
  (.addEventListener js/window "hashchange" sync-route!)
  (.addEventListener js/document "input" on-input)
  (.addEventListener js/document "click" on-click)
  (.addEventListener js/window "resize"
                     #(viewport/render! (:scene (core/solved @app-state))))
  (let [el (.getElementById js/document "app")
        rt (rdomc/create-root el)]
    (reset! root rt)
    (rdomc/render rt [root-view])))
