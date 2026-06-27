(ns kami.backend.browser
  "L1 backend (cljs) — drives the `kami-clj-host` WASM module → `kami-render` →
  WebGPU in the browser. The real GPU path (ARCHITECTURE.md §4/§9). Implements
  `kami.gpu/IGpuBackend` by calling the `KamiCljHost` wasm-bindgen exports
  (register_mesh/register_material/register_shader/submit_frame/resize), which
  decode the KAMI columnar buffer (`kami.ipc/pack`) and render one instanced pass.

  `kami-clj-host` is the Rust crate `../../kami-clj-host` built with
  `wasm-pack build --target web --features host`; its JS glue exposes
  `KamiCljHost.create(canvas) -> Promise<host>`.

  Tint: the host decodes both the v1 (model-only) and v2 (model + per-draw tint)
  columnar layouts, so this backend forwards `:buffer`/`:meta` unchanged. To draw
  with per-instance tint, the caller packs v2 — `(kami.gpu/submit! be frame
  {:tint? true})` — and the bytes flow through here untouched."
  (:require [kami.gpu :as gpu]))

(defn- ->u8 [buffer]
  "Convert the packed :buffer (a vector of 0-255 ints) to a Uint8Array for the
  wasm boundary. `kami.ipc/pack` already produced GPU-aligned bytes."
  (js/Uint8Array. (into-array buffer)))

(defn- ->f32 [xs] (js/Float32Array. (into-array xs)))
(defn- ->u32 [xs] (js/Uint32Array. (into-array xs)))

;; A thin record wrapping the wasm `KamiCljHost` instance.
(defrecord BrowserBackend [host]
  gpu/IGpuBackend
  (register-mesh! [_ id vertices indices]
    (.register_mesh host id (->f32 vertices) (->u32 indices)))
  (register-material! [_ id params]
    (.register_material host id (->f32 (or params []))))
  (register-shader! [_ id wgsl layout]
    (.register_shader host id wgsl (or layout "")))
  (submit-frame! [_ packed]
    ;; packed = {:buffer :len :meta …}; meta travels as JSON, buffer as bytes.
    (.submit_frame host
                   (js/JSON.stringify (clj->js (:meta packed)))
                   (->u8 (:buffer packed))))
  (resize! [_ w h]
    (.resize host w h)))

(defn make
  "Create a browser GPU backend bound to canvas id `:canvas`. Returns a Promise
  that resolves to the backend once `KamiCljHost.create` settles (async
  adapter/device request) — await it with `.then`. `:host-ctor` lets callers
  inject the wasm class (default `js/KamiCljHost`)."
  [{:keys [canvas host-ctor]}]
  (let [el   (.getElementById js/document canvas)
        ctor (or host-ctor js/KamiCljHost)]
    (.then (.create ctor el) ->BrowserBackend)))
