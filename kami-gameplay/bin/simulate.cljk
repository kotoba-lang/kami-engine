;; Headless match harness.
;;
;;   npx --yes nbb bin/simulate.cljs [--seed N] [--bots N] [--ticks N] [--edn]
;;
;; Plays a full battle royale with no renderer, no browser and no GPU, and
;; prints what happened. This is the artifact the repository has been missing:
;; `battle_royale_weapons.edn`, `battle_royale_storm.edn` and
;; `battle_royale_consumables.edn` each claim in their header that a Rust
;; function is their parity oracle, and that Rust left with the workspace. A
;; table nothing can execute is a description of a game rather than a game.
;;
;; It is also how you tell a mechanic is real rather than declared. Every number
;; below is produced by the same code path the browser host would run.
(ns simulate
  (:require [clojure.edn :as edn]
            [kotoba.lang.text :as str]
            [kami.gameplay.world :as world]
            [kami.gameplay.match :as match]
            [kami.gameplay.zone :as zone]
            [kami.gameplay.weapon :as weapon]
            [kami.gameplay.perception :as perc]
            [kami.gameplay.vec3 :as v]
            [kami.gameplay.attributes :as attr]
            ["fs" :as fs]))

(def argv (vec (drop 2 (js->clj (.-argv js/process)))))
(defn- flag [name default]
  (if-let [i (first (keep-indexed #(when (= %2 (str "--" name)) %1) argv))]
    (js/parseInt (nth argv (inc i)))
    default))
(def edn-out? (some #{"--edn"} argv))

(def data-dir "../kami-game-scene/data/")
(defn- read-data [f] (edn/read-string (.readFileSync fs (str data-dir f) "utf8")))

(def weapons (read-data "battle_royale_weapons.edn"))
(def storm (read-data "battle_royale_storm.edn"))
(def scene (edn/read-string
             (.readFileSync fs "../kami-clj-play3d/games/royale-tps/scene.edn" "utf8")))

(def seed (flag "seed" 20260822))
(def bot-count (flag "bots" 23))
(def max-ticks (flag "ticks" 20000))
(def dt 0.1)

(def bot-pool (get-in scene [:loadout :bot-pool] [0 5 10 13 16]))
(def spawn-radius (get-in scene [:match :bot-spawn-radius] 420.0))

(defn- ring-pos [i n r]
  (let [a (* 2.0 Math/PI (/ (double i) (double n)))]
    [(* r (Math/cos a)) 0.0 (* r (Math/sin a))]))

(def entities
  (into {0 (world/make-entity
             {:tag "player" :pos [0.0 0.0 0.0]
              :attrs {:yaw 0.0 :health 100.0 :move-speed 6.4 :eye-height 1.62
                      :weapon 16.0 :ammo 16.0 :reserve-ammo 120.0}})}
        (for [i (range 1 (inc bot-count))]
          [i (world/make-entity
               {:tag "bot" :pos (ring-pos i bot-count spawn-radius)
                :attrs {:health 100.0 :move-speed 5.4 :eye-height 1.62
                        :weapon (double (nth bot-pool (mod i (count bot-pool))))
                        :ammo 30.0 :reserve-ammo 240.0}})])))

(def w0 (world/make-world {:scene scene :weapons weapons :storm storm
                           :seed seed :entities entities}))

;; A scripted player: rotate with the storm, turn toward whatever is visible,
;; sight the weapon and shoot at it. Deliberately simple — it is not a
;; demonstration of skill — but it goes through the same perception, aim,
;; weapon and ballistics path a human would, so the player column in the
;; standings means something rather than reporting a bot that fires into empty
;; space forever.
(defn- input-fn [w _t]
  (let [p (get-in w [:entities 0])
        z (world/zone-now w)]
    (if-not (attr/alive? p)
      {}
      (let [inside? (zone/inside? z (:pos p))
            seen (first (perc/sense-targets w 0 (:senses w) {}))
            target (when seen (get-in w [:entities (:id seen)]))
            ;; turn toward the target: a look delta, exactly as a mouse would
            ;; deliver it, rather than writing yaw directly
            dyaw (when target
                   (let [d (v/sub (:pos target) (:pos p))
                         want (Math/atan2 (v/x d) (- (v/z d)))
                         err (attr/wrap-angle (- want (attr/get p :yaw)))]
                     (/ (max -0.35 (min 0.35 err))
                        (get-in w [:scene :input/look-sensitivity] 0.0032))))
            ;; Only shoot inside the weapon's full-damage envelope. Firing at
            ;; whatever is merely *visible* means firing a 100-unit pistol at a
            ;; target 210 units away, which lands nothing and reports a player
            ;; who spent the whole match missing — a fact about this script,
            ;; not about the engine.
            in-range? (when target
                        (<= (v/dist-xz (:pos p) (:pos target))
                            (double (:damage-falloff (weapon/equipped (:weapons w) p) 0.0))))]
        {0 {:fire? (boolean in-range?)
            :ads? (some? target)
            :look-dx (or dyaw 0.0)
            :move-y (cond (not inside?) 1.0 in-range? 0.0 target 1.0 :else 0.25)}}))))

(def result (world/run w0 {:dt dt :max-ticks max-ticks :input-fn input-fn}))

(def events (:events result))
(def by-kind (frequencies (map :kind events)))
(def kills (filter #(= :kill (:kind %)) events))
(def shots (filter #(= :shot (:kind %)) events))
(def connected (filter :target shots))
(def headshots (filter #(= :head (:zone %)) kills))
;; :hit is emitted by the damage pipeline for every source, so the raw count
;; is dominated by storm ticks. Split them, or the report reads as though bots
;; connect five hundred times more often than they do.
(def bullet-hits (filter #(and (= :hit (:kind %)) (not= :storm (:source %))) events))
(def storm-ticks (filter #(and (= :hit (:kind %)) (= :storm (:source %))) events))

(defn- pct [a b] (if (pos? b) (str (.toFixed (* 100.0 (/ (double a) b)) 1) "%") "n/a"))

(if edn-out?
  (println (pr-str {:seed seed :ticks (:ticks result) :ended? (:ended? result)
                    :standings (:standings result) :events by-kind}))
  (do
    (println "── KAMI Royale TPS — headless match ──────────────────────────────")
    (println (str "  seed " seed "  ·  " (inc bot-count) " entrants  ·  dt " dt "s"))
    (println (str "  storm: " (count (zone/load-phases storm)) " phases, "
                  (.toFixed (:total-seconds (:zone-plan w0)) 0) "s to close"))
    (println (str "  weapons: " (count (weapon/load-table weapons)) " rows from "
                  "battle_royale_weapons.edn"))
    (println)
    (println (str "  ran " (:ticks result) " ticks ("
                  (.toFixed (* dt (:ticks result)) 0) "s of match time)"))
    (println (str "  ended: " (:ended? result)
                  (when-let [win (get-in result [:world :match :winner])]
                    (str " — winner #" win))))
    (println)
    (println "  shooting")
    (println (str "    shots fired      " (count shots)))
    (println (str "    connected        " (count connected)
                  "  (" (pct (count connected) (count shots)) ")"))
    (println (str "    damage events    " (count bullet-hits)
                  "  (storm ticks excluded: " (count storm-ticks) ")"))
    (println (str "    kills            " (count kills)))
    (println (str "    headshot kills   " (count headshots)))
    (println)
    (println "  deaths by cause")
    (doseq [[src n] (sort-by (comp - val) (frequencies (map :source kills)))]
      (println (str "    " (str/join (repeat (max 0 (- 17 (count (str src)))) " "))
                    src "  " n)))
    (println)
    (println "  final standings (top 8)")
    (println "    place  id  tag     kills  damage")
    (doseq [r (take 8 (sort-by :place (remove #(zero? (:place %)) (:standings result))))]
      (println (str "    " (.padStart (str (:place r)) 5) "  "
                    (.padStart (str (:id r)) 2) "  "
                    (.padEnd (:tag r) 7) " "
                    (.padStart (str (:kills r)) 5) "  "
                    (.padStart (.toFixed (:damage r) 0) 6))))
    (println)
    (println (str "  event stream: " (pr-str by-kind)))))

;; Evidence floor. A harness that simulated nothing must not print a tidy
;; report full of zeroes and exit 0.
(when (or (< (:ticks result) 10)
          (zero? (count shots))
          (zero? (count kills)))
  (println "\nREFUSING TO REPORT A MATCH: the simulation produced no shots or no deaths.")
  (js/process.exit 3))
