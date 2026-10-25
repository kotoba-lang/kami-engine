(ns kotoba.amenominaka.application
  "M3 (ADR-2607100100): the Kit Application (extension lifecycle host) —
  ported forward from `kotoba-lang/kami-nv-compat`'s
  `kotoba.lang.kami-nv-compat.amenominaka.application` (itself a
  clean-room mirror of `omni.kit.app.IApp`, ADR-2605261800 D6/D10.4, Wave
  4 of the kami-nv-compat TS->CLJC port) into THIS repo — see
  [[kotoba.amenominaka.extension]]'s docstring for why the canonical
  implementation belongs here rather than in the nv-compat facade, and
  for why no 'magatama Pregel cell' mapping exists (retired as a
  concept, not deferred).

  A shell that owns registered IExt instances and dispatches startup /
  update / shutdown in dependency-respecting order. Dependencies come
  from each extension's parsed manifest (`extension.edn` or `.toml`, via
  [[kotoba.amenominaka.extension/parse-extension-toml]]); startup is a
  Kahn topological sort (parents before children), shutdown is reverse;
  a cycle throws. Logic is unchanged from the ported source — only the
  namespace and this docstring differ."
  (:require [kotoba.amenominaka.extension :as ext]))

(defprotocol IApplication
  (register-extension! [this ext-id instance] [this ext-id instance manifest])
  (unregister-extension! [this ext-id])
  (get-extension [this ext-id])
  (get-extension-ids [this])
  (num-extensions [this])
  (num-started [this])
  (startup-all [this])
  (update! [this dt])
  (shutdown-all [this]))

(defn- registered-deps
  "The registered-extension ids this extension depends on (per its manifest)."
  [extensions {:keys [manifest]}]
  (if manifest
    (filterv #(contains? extensions %) (keys (:dependencies manifest)))
    []))

(defn- topological-order
  "Kahn topological order over the depends-on relation (only registered
  dependencies count). Throws on a cycle. Ties broken alphabetically."
  [extensions]
  (let [deps-map  (into {} (for [[ext-id ext] extensions]
                             [ext-id (registered-deps extensions ext)]))
        in-degree (atom (into {} (for [[eid deps] deps-map] [eid (count deps)])))
        ready     (atom (vec (sort (for [[eid d] @in-degree :when (zero? d)] eid))))
        order     (atom [])]
    (while (seq @ready)
      (let [eid (first @ready)]
        (swap! ready subvec 1)
        (swap! order conj eid)
        (doseq [[other deps] deps-map
                :when (some #{eid} deps)]
          (let [nd (dec (get @in-degree other))]
            (swap! in-degree assoc other nd)
            (when (zero? nd)
              (swap! ready #(vec (sort (conj % other)))))))))
    (let [result @order]
      (if (= (count result) (count extensions))
        result
        (throw (ex-info "Cyclic dependency in extensions; cannot order startup" {}))))))

(defn application
  "Returns a new IApplication (mirrors `new Application()`)."
  []
  (let [state (atom {:extensions {}})]
    (reify IApplication
      (register-extension! [_ ext-id instance]
        (swap! state assoc-in [:extensions ext-id] {:instance instance :manifest nil :started false}))
      (register-extension! [_ ext-id instance manifest]
        (swap! state assoc-in [:extensions ext-id] {:instance instance :manifest manifest :started false}))

      (unregister-extension! [_ ext-id]
        (let [extension (get-in @state [:extensions ext-id])]
          (when extension
            (when (:started extension)
              (ext/on-shutdown (:instance extension)))
            (swap! state update :extensions dissoc ext-id))))

      (get-extension [_ ext-id]
        (get-in @state [:extensions ext-id :instance]))

      (get-extension-ids [_]
        (vec (keys (:extensions @state))))

      (num-extensions [_]
        (count (:extensions @state)))

      (num-started [_]
        (count (for [[_ e] (:extensions @state) :when (:started e)] e)))

      (startup-all [_]
        (let [order (topological-order (:extensions @state))
              log   (atom [])]
          (doseq [eid order]
            (let [extension (get-in @state [:extensions eid])]
              (when-not (:started extension)
                (ext/on-startup (:instance extension) eid)
                (swap! state assoc-in [:extensions eid :started] true)
                (swap! log conj eid))))
          @log))

      (update! [_ dt]
        (doseq [[_ extension] (:extensions @state)]
          (when (:started extension)
            (ext/on-update (:instance extension) dt))))

      (shutdown-all [_]
        (let [order (try (topological-order (:extensions @state))
                         (catch #?(:clj Exception :cljs :default) _
                           (vec (keys (:extensions @state)))))
              log   (atom [])]
          (doseq [eid (reverse order)]
            (let [extension (get-in @state [:extensions eid])]
              (when (:started extension)
                (ext/on-shutdown (:instance extension))
                (swap! state assoc-in [:extensions eid :started] false)
                (swap! log conj eid))))
          @log)))))

;; ── global singleton (mirrors omni.kit.app.get_app()) ─────────────────────

(defonce ^:private global-app (atom nil))

(defn get-app
  "Return the global Application singleton (mirrors `omni.kit.app.get_app()`)."
  []
  (or @global-app (reset! global-app (application))))

(defn reset-app!
  "Reset the global app (test helper; not in upstream Kit)."
  []
  (reset! global-app nil))
