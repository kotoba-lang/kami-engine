(ns kami.backend.host
  "L1 backend (JVM) — headless / server-side path. Not the chosen 'browser + Rust
  backend' critical path (ARCHITECTURE.md §4/§9), but a useful pure-clj
  `IGpuBackend` impl for deterministic CI verification: it does no GPU work —
  instead it decodes every submitted KAMI frame with `kami.ipc/unpack`, checks the
  columnar buffer is well-formed (magic, ncols, declared length, 16-byte column
  alignment), and records the registered assets + per-frame stats so tests / golden
  runs can assert on exactly the bytes the browser backend would DMA into wgpu.

  A future variant could embed kami-render via wasmtime / native wgpu offscreen for
  real thumbnails / golden images; this one is decode-and-verify only."
  (:require [kami.gpu :as gpu]
            [kami.ipc :as ipc]))

(defn- verify-packed
  "Decode `packed` (from `kami.ipc/pack`) with `ipc/unpack` and return
  `{:ok bool :errors [..] :n :ncols :len :draws}`. Pure; never throws — a corrupt
  buffer is caught and reported as an error rather than propagated."
  [packed]
  (try
    (let [back (ipc/unpack (:buffer packed))
          errs (cond-> []
                 (not= (:ncols packed) (:ncols back))
                 (conj (str "ncols mismatch: header=" (:ncols back)
                            " packed=" (:ncols packed)))

                 (and (:len packed) (not= (:len packed) (count (:buffer packed))))
                 (conj (str "len mismatch: :len=" (:len packed)
                            " buffer=" (count (:buffer packed))))

                 (not (every? #(zero? (mod (:offset %) 16)) (:columns back)))
                 (conj "column payload offset is not 16-byte aligned"))]
      {:ok      (empty? errs)
       :errors  errs
       :n       (:n back)
       :version (:version back)
       :ncols   (:ncols back)
       :len     (count (:buffer packed))
       :draws   (count (:draws (:meta packed)))})
    (catch Exception e
      {:ok false :errors [(str "unpack failed: " (.getMessage e))] :exception e})))

(defrecord HostBackend [state strict?]
  gpu/IGpuBackend
  (register-mesh! [_ id vertices indices]
    (swap! state assoc-in [:meshes id]
           {:vertices (count vertices) :indices (count indices)})
    id)
  (register-material! [_ id params]
    (swap! state assoc-in [:materials id] {:params (count params)})
    id)
  (register-shader! [_ id wgsl layout]
    (swap! state assoc-in [:shaders id]
           {:wgsl-bytes (count (or wgsl "")) :layout layout})
    id)
  (submit-frame! [_ packed]
    (let [v (verify-packed packed)]
      (when (and strict? (not (:ok v)))
        (throw (ex-info "kami.backend.host: frame failed verification"
                        {:errors (:errors v)})))
      (swap! state update :frames (fnil conj []) (dissoc v :exception))
      v))
  (resize! [_ w h]
    (swap! state assoc :size [w h])
    nil))

(defn make
  "Create a headless decode-and-verify backend implementing `kami.gpu/IGpuBackend`.
  `opts` may set `:strict?` (default false) to throw on the first frame that fails
  `ipc/unpack` verification instead of just recording it as `{:ok false}`."
  [opts]
  (->HostBackend (atom {:meshes {} :materials {} :shaders {} :frames [] :size nil})
                 (boolean (:strict? opts))))

(defn state
  "Snapshot the backend's recorded verification state (assets, frames, size)."
  [backend]
  @(:state backend))
