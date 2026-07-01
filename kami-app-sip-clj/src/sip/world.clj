(ns sip.world
  "Authoring (JVM): build the water-city world as datoms, then emit the portable
  snapshot the browser boots from. `clojure -M:datomic:build` runs `-main`.

  The world = a Datomic/datalevin transaction. The render half (water, sakura,
  light, camera) becomes the scene snapshot served to `kami-render`; the game
  half (8 areas, the player, the still-unnamed Ghost Agent) lives in the same
  store as the source of truth for `sip.store`.

  This ns is the generic engine glue — entity instantiation + transact +
  snapshot. The concrete water-city layout (assets, camera/light, tile/tree/
  lantern/house placement, environment grade, player/Ghost-Agent starting
  state) is SIP *content*: `resources/world-water-city.edn` under
  $SIP_IP_ROOT (see `sip.lore/ip-root`), not hardcoded here."
  (:require [sip.store :as store]
            [sip.lore :as lore]
            [kami.db :as kdb]
            [clojure.java.io :as io]
            [clojure.edn :as edn]))

(defn- read-edn [rel]
  (let [f (io/file lore/ip-root rel)]
    (when (.exists f) (edn/read-string (slurp f)))))

(def ^:private fallback-spec
  "Minimal inline fallback so the build stays runnable if the IP repo is
  absent — same degrade-gracefully contract as `sip.lore`."
  {:assets [{:asset/id "mesh/water" :asset/kind :mesh :asset/inline {:prim :plane :size 1}}
            {:asset/id "mat/water" :asset/kind :material :asset/inline {:albedo [0.55 0.74 0.86]}}]
   :camera {:translation [0.0 9.0 22.0] :rotation [-0.156 0.0 0.0 0.988]
            :fov 55.0 :near 0.1 :far 2000.0}
   :light  {:kind :dir :color [1.0 0.93 0.84] :intensity 1.3 :rotation [0.0 0.0 0.0 1.0]}
   :canal  {:x-range [-16 17 8] :z-range [-16 17 8]
            :mesh "mesh/water" :material "mat/water" :scale [4.0 1.0 4.0] :y 0.0}
   :sakura {:x-range [-16 17 8] :z-lines [-19 19]
            :mesh "mesh/water" :material "mat/water" :scale [1.3 1.4 1.3] :y 0.0}
   :lanterns {:positions [[0 0]]
              :mesh "mesh/water" :material "mat/water" :scale [0.5 0.9 0.5] :y 1.1}
   :houses {:positions [[0 -24]] :heights [3.0]
            :mesh "mesh/water" :material "mat/water" :scale-xz [2.2 2.0]}
   :env    {:clear [0.96 0.93 0.99 1.0] :sky :dawn
            :fog {:color [0.93 0.92 0.97] :density 0.015}}
   :agent  {:named? false :bond 0 :awakening 0 :voice :system-log}
   :player {:name "見習い" :starting-area :vol01-water-city
            :kokoro-value 0.6 :kokoro-tempo 60.0}})

(defn spec
  "The Vol.1 (Water City) world spec — content, not code."
  []
  (or (read-edn "resources/world-water-city.edn") fallback-spec))

;; --- entity instantiation (generic engine glue; no story/world literals) ----

(defn- asset-entity [{:keys [asset/id asset/kind asset/inline]}]
  {:asset/id id :asset/kind kind :asset/inline (pr-str inline)})

(defn- placed-entity [name [x z] y scale mesh material]
  {:kami/eid (random-uuid) :kami/name name
   :transform/translation [(double x) (double y) (double z)]
   :transform/rotation [0.0 0.0 0.0 1.0]
   :transform/scale scale
   :mesh/asset [:asset/id mesh] :material/asset [:asset/id material]})

(defn- tiled [{:keys [x-range z-range mesh material scale y]} name]
  (let [[x0 x1 xs] x-range [z0 z1 zs] z-range]
    (for [x (range x0 x1 xs) z (range z0 z1 zs)]
      (placed-entity name [x z] y scale mesh material))))

(defn- lined [{:keys [x-range z-lines mesh material scale y]} name]
  (let [[x0 x1 xs] x-range]
    (for [z z-lines x (range x0 x1 xs)]
      (placed-entity name [x z] y scale mesh material))))

(defn- dotted [{:keys [positions mesh material scale y]} name]
  (map #(placed-entity name % y scale mesh material) positions))

(defn- housed [{:keys [positions heights mesh material scale-xz]} name]
  (let [[sx sz] scale-xz]
    (map (fn [pos h] (placed-entity name pos (/ h 2.0) [sx (double h) sz] mesh material))
         positions heights)))

(defn scene-tx
  "Render half: a contiguous canal plaza framed by cherry trees, waterside
  lanterns and wood townhouses, from `spec` — a calm water-city vignette under
  a warm key light, viewed from a gently downward camera."
  []
  (let [{:keys [assets camera light canal sakura lanterns houses]} (spec)]
    (concat
     (map asset-entity assets)
     [{:kami/eid (random-uuid) :kami/name "main-cam"
       :camera/fov (:fov camera) :camera/near (:near camera) :camera/far (:far camera)
       :camera/active? true
       :transform/translation (:translation camera)
       :transform/rotation (:rotation camera)}
      {:kami/eid (random-uuid) :kami/name "morning-light"
       :light/kind (:kind light) :light/color (:color light) :light/intensity (:intensity light)
       :transform/rotation (:rotation light)}]
     (tiled canal "canal")
     (lined sakura "sakura")
     (dotted lanterns "lantern")
     (housed houses "house"))))

(defn game-tx
  "Game half: the 8 learning-areas (from the story-bible), the player, and their
  Ghost Agent — unnamed, awakening at stage 0. Asking its name is move one."
  []
  (let [{:keys [agent player]} (spec)
        areas (for [{:keys [id title volume season theme motifs]} (lore/volumes)]
                {:sip.area/id id :sip.area/title title :sip.area/volume volume
                 :sip.area/season season :sip.area/theme theme
                 :sip.area/motif (vec motifs)
                 :sip.area/open? (= volume 1)}) ; only Vol.1 (Water City) open at start
        agent-e {:sip.agent/named? (:named? agent) :sip.agent/bond (:bond agent)
                 :sip.agent/awakening (:awakening agent) :sip.agent/voice (:voice agent)}]
    (concat areas
            [agent-e
             {:sip.player/id (random-uuid)
              :sip.player/name (:name player)
              :sip.player/area [:sip.area/id (:starting-area player)]
              :sip.kokoro/value (:kokoro-value player) :sip.kokoro/tempo (:kokoro-tempo player)}])))

(defn build!
  "Transact schema + world into a fresh store at `dir`; return the render
  snapshot (game state stays in the store for `sip.store`)."
  [dir]
  (let [conn (store/connect dir)]
    ;; datalevin's transact! returns the TxReport directly (not a future).
    (store/transact! conn (vec (scene-tx)))
    (store/transact! conn (vec (game-tx)))
    (kdb/snapshot (store/db conn)
                  {:scene "spirit-in-physics/water-city"
                   :env (pr-str (:env (spec)))})))

(defn -main
  "Build the snapshot and write it where the browser bundle can fetch it
  (public/snapshot.edn). Title: Spirit in Physics → https://sip.etzhayyim.com"
  [& [dir]]
  (let [dir (or dir (str (System/getProperty "java.io.tmpdir") "/sip-world"))
        snap (build! dir)
        out  (io/file "public" "snapshot.edn")]
    (io/make-parents out)
    (spit out (pr-str snap))
    (println "Spirit in Physics — wrote" (.getPath out)
             "(" (count (:snapshot/entities snap)) "entities,"
             (count (:snapshot/assets snap)) "assets )")
    ;; sanity: the snapshot the browser will load must be structurally valid
    (require 'kami.scene)
    (let [valid? (resolve 'kami.scene/valid?)]
      (when valid? (valid? snap)))
    (System/exit 0)))
