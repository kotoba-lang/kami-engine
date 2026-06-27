(ns kami.gpu
  "L1 — GPU frontend. Defines the `IGpuBackend` protocol (the clj ↔ kami-render
  boundary) and the resource/submit API the runtime calls. The actual GPU work is
  done by `kami-render` (Rust/wgpu); backends live in `kami.backend.*`.

  Two backends behind one protocol (ARCHITECTURE.md §9):
    - browser  : kami-render WASM → WebGPU (→ WebGL2 fallback). The real path.
    - host     : optional headless (wasmtime/FFI). Stub for now."
  (:require [kami.ipc :as ipc]))

(defprotocol IGpuBackend
  "The minimal surface `kami-render` must expose (additive WIT `kami:engine/frame`,
  §8b). Resources are uploaded once keyed by asset id; frames reference them."
  (register-mesh!     [backend id vertices indices]
    "Upload a mesh once under asset `id`. Idempotent on identical bytes.")
  (register-material! [backend id params]
    "Upload material params (f32 vector) under asset `id`.")
  (register-shader!   [backend id wgsl layout]
    "Register clj-authored WGSL (from `kami.wgsl/emit`) as a pipeline under `id`.")
  (submit-frame!      [backend packed]
    "Submit one packed KAMI columnar frame (from `kami.ipc/pack`) for this frame.")
  (resize!            [backend w h]
    "Notify the backend of a canvas/surface resize (depth target realloc, etc.)."))

(defn backend
  "Construct the platform-appropriate backend. To keep this ns dependency-light
  and avoid pulling the cljs-only browser ns onto the JVM classpath, callers
  normally invoke `kami.backend.browser/make` / `kami.backend.host/make` directly.
  `opts` may carry an explicit `:make` fn for injection/testing (e.g. a mock
  backend in a contract test)."
  [{:keys [make] :as opts}]
  (if make
    (make opts)
    (throw (ex-info "kami.gpu/backend: pass :make, or call kami.backend.*/make directly"
                    {:opts (keys opts)}))))

(defn ensure-assets!
  "Walk a scene-snapshot's :snapshot/assets and `register-*!` each one once,
  dispatching on :asset/kind. For procedural assets the bytes live inline
  (:asset/inline, edn); for B2/IPFS assets a `:resolve` fn in the asset map (or a
  backend-side fetch) supplies {:vertices :indices} / {:params} / {:wgsl :layout}.
  Returns the set of registered asset ids. Idempotent per backend."
  [backend snapshot]
  (reduce
   (fn [done {:keys [:asset/id :asset/kind] :as asset}]
     (let [data (or (:asset/data asset)
                    (when-let [r (:resolve asset)] (r asset))
                    {})]
       (case kind
         :mesh     (register-mesh! backend id (:vertices data) (:indices data))
         :material (register-material! backend id (:params data))
         :shader   (register-shader! backend id (:wgsl data) (:layout data))
         :texture  nil ; textures handled by material upload for now
         nil)
       (conj done id)))
   #{}
   (:snapshot/assets snapshot)))

(defn submit!
  "Convenience: `kami.ipc/pack` a render-IR frame and `submit-frame!` it.
  `pack-opts` is forwarded to `kami.ipc/pack` — e.g. `{:tint? true}` packs the
  version-2 layout with per-draw tint columns (the browser backend forwards the
  bytes version-agnostically; the Rust host renders v1 or v2 alike)."
  ([backend frame] (submit! backend frame nil))
  ([backend frame pack-opts]
   (submit-frame! backend (ipc/pack frame pack-opts))))
