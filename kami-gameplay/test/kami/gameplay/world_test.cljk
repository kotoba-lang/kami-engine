(ns kami.gameplay.world-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami.gameplay.world :as world]
            [kami.gameplay.match :as match]
            [kami.gameplay.attributes :as attr]
            [kami.gameplay.weapon :as weapon]
            [kami.gameplay.vec3 :as v]
            [kami.gameplay.support :as s]))

(defn- base
  ([] (base {}))
  ([{:keys [entities scene seed] :or {seed 20260822}}]
   (world/make-world {:scene (merge {:zone {:start-radius 1000.0 :center [0.0 0.0 0.0]}} scene)
                      :weapons s/weapons-raw
                      :storm s/storm-raw
                      :seed seed
                      :entities (or entities {})})))

(defn- duel
  "A player looking down -Z at a bot 20 units away, both holding weapon `wi`
  (an assault rifle by default)."
  ([] (duel 0))
  ([wi]
   (base {:entities (into {} [(s/actor 1 [0.0 0.0 0.0]
                                       {:tag "player" :yaw 0.0 :weapon wi :ammo 30 :reserve-ammo 90})
                              (s/actor 2 [0.0 0.0 -20.0]
                                       {:tag "bot" :yaw 3.14159 :weapon wi :ammo 30 :reserve-ammo 90})])})))

(deftest system-order-is-declared-not-implied
  (is (= [:weapon-upkeep :ai :player-intent :fire :integrate :zone :deaths :match]
         world/system-order))
  (is (= (count world/system-order) (count (distinct world/system-order)))))

(deftest a-step-starts-the-match-and-advances-the-clock
  (let [[w evs] (world/step (duel) (/ 1.0 30.0) {})]
    (is (= :live (get-in w [:match :state])))
    (is (= 2 (get-in w [:match :entrants])))
    (is (= 1 (get-in w [:clock :tick])))
    (is (< (Math/abs (- (get-in w [:clock :now-ms]) 33.333)) 0.01))
    (is (vector? evs))))

(deftest firing-goes-through-the-weapon-gate-for-players-too
  (let [w0 (duel)
        [w1 evs1] (world/step w0 (/ 1.0 30.0) {1 {:fire? true}})
        [w2 evs2] (world/step w1 (/ 1.0 30.0) {1 {:fire? true}})]
    (is (some #(= :shot (:kind %)) evs1))
    (is (= 29.0 (attr/get (get-in w1 [:entities 1]) :ammo)))
    (testing "the second pull inside the 181ms interval is refused"
      (is (not-any? #(= :shot (:kind %)) evs2))
      (is (= 29.0 (attr/get (get-in w2 [:entities 1]) :ammo))))))

(deftest an-aimed-shot-damages-and-a-turned-away-shot-does-not
  ;; weapon 2 is the bolt-action sniper: spread 0, so this isolates aim from
  ;; dispersion. The assault rifle's cone is exercised separately below.
  (let [[hit-w evs] (world/step (duel 2) (/ 1.0 30.0) {1 {:fire? true}})
        turned (assoc-in (duel 2) [:entities 1 :attrs :yaw] (/ Math/PI 2))
        [miss-w _] (world/step turned (/ 1.0 30.0) {1 {:fire? true}})]
    (is (some #(contains? #{:hit :kill} (:kind %)) evs))
    (is (< (attr/get (get-in hit-w [:entities 2]) :health) 100.0))
    (is (= 100.0 (attr/get (get-in miss-w [:entities 2]) :health))
        "aim is a thing the game has now")))

(deftest a-sniper-headshot-is-lethal-outright
  ;; 105 base x 2.5 headshot = 262.5 against a 100-health target. The
  ;; multiplier has been sitting in battle_royale_weapons.edn since ADR-0046
  ;; with nothing reading it.
  (let [[w evs] (world/step (duel 2) (/ 1.0 30.0) {1 {:fire? true}})
        kill (first (filter #(= :kill (:kind %)) evs))]
    (is (some? kill))
    (is (= :head (:zone kill)))
    (is (not (attr/alive? (get-in w [:entities 2]))))))

(deftest hip-fire-spread-makes-some-shots-miss-and-aiming-fixes-that
  ;; The shipped game's host-side auto-fire cannot miss, which is most of why it
  ;; does not read as a shooter. Here the same perfectly-aimed burst lands
  ;; differently depending only on whether the player is sighted.
  ;;
  ;; The target is tagged "dummy" (so the AI never drives it) and unarmed with a
  ;; deep health pool, which keeps the burst running instead of ending the match
  ;; on the first connection.
  (let [range-test
        (fn [ads?]
          (let [w (base {:entities
                         (into {} [(s/actor 1 [0.0 0.0 0.0]
                                            {:tag "player" :yaw 0.0 :weapon 0
                                             :ammo 30 :reserve-ammo 9000})
                                   (s/actor 2 [0.0 0.0 -60.0]
                                            {:tag "dummy" :health-max 10000 :health 10000})])})
                r (world/run w {:dt (/ 1.0 30.0) :max-ticks 500
                                :input-fn (fn [_ _] {1 {:fire? true :ads? ads?}})})
                shots (filter #(= :shot (:kind %)) (:events r))]
            {:fired (count shots)
             :connected (count (filter :target shots))}))
        hip (range-test false)
        ads (range-test true)]
    (is (> (:fired hip) 20) (str "the burst actually fired: " (:fired hip)))
    (is (= (:fired hip) (:fired ads)) "same trigger discipline in both runs")
    (is (< (:connected hip) (:fired hip)) "hip fire misses — dispersion is real")
    (is (pos? (:connected hip)) "but not every shot")
    (is (> (:connected ads) (:connected hip))
        (str "aiming down sight must buy accuracy — that is what makes ADS a decision; "
             "hip " (:connected hip) "/" (:fired hip)
             " vs ads " (:connected ads) "/" (:fired ads)))))

(deftest movement-is-camera-relative-through-the-whole-step
  (let [w0 (duel)
        [w1 _] (world/step w0 0.5 {1 {:move-y 1.0}})
        [w2 _] (world/step (assoc-in w0 [:entities 1 :attrs :yaw] (/ Math/PI 2)) 0.5 {1 {:move-y 1.0}})]
    (is (neg? (v/z (get-in w1 [:entities 1 :pos]))) "forward at yaw 0 is -Z")
    (is (pos? (v/x (get-in w2 [:entities 1 :pos]))) "turn the camera and forward turns with it")))

(deftest aiming-down-sight-slows-the-player
  (let [w0 (assoc-in (duel) [:entities 1 :attrs :ads] 1.0)
        [hip _] (world/step (duel) 0.5 {1 {:move-y 1.0}})
        [ads _] (world/step w0 0.5 {1 {:move-y 1.0 :ads? true}})]
    (is (< (Math/abs (v/z (get-in ads [:entities 1 :pos])))
           (Math/abs (v/z (get-in hip [:entities 1 :pos])))))))

(deftest the-storm-damages-whoever-is-outside-it
  (let [w0 (base {:entities (into {} [(s/actor 1 [0.0 0.0 0.0] {:tag "player"})
                                      (s/actor 2 [5000.0 0.0 0.0] {:tag "player"})])})
        [w1 evs] (world/step w0 1.0 {})]
    (is (= 100.0 (attr/get (get-in w1 [:entities 1]) :health)))
    (is (< (attr/get (get-in w1 [:entities 2]) :health) 100.0))
    (is (some #(= :storm (:source %)) evs))))

(deftest a-projectile-weapon-does-not-resolve-on-the-frame-it-is-fired
  (let [w0 (assoc-in (duel) [:entities 1 :attrs :weapon] 3.0)
        w0 (assoc-in w0 [:entities 1 :attrs :ammo] 1.0)
        [w1 evs1] (world/step w0 (/ 1.0 60.0) {1 {:fire? true}})]
    (is (some #(= :shot-projectile (:kind %)) evs1))
    (is (= 100.0 (attr/get (get-in w1 [:entities 2]) :health))
        "a 100 u/s rocket has to cross 20 units first — that is what makes it dodgeable")
    (is (= 1 (count (:projectiles w1))))
    (let [[w2 _] (world/step w1 0.4 {})]
      (is (< (attr/get (get-in w2 [:entities 2]) :health) 100.0) "and then it arrives"))))

(deftest a-reload-completes-before-firing-is-tested-in-the-same-tick
  (testing "system order: :weapon-upkeep runs before :fire"
    (let [w0 (-> (duel)
                 (assoc-in [:entities 1 :attrs :ammo] 0.0)
                 (assoc-in [:entities 1 :attrs :reserve-ammo] 90.0)
                 (assoc-in [:entities 1 :attrs :reload-until] 30.0))
          [w1 evs] (world/step w0 (/ 1.0 30.0) {1 {:fire? true}})]
      (is (some #(= :shot (:kind %)) evs)
          "the reload that finishes at 30ms must arm the gun for this tick's 33ms")
      (is (= 29.0 (attr/get (get-in w1 [:entities 1]) :ammo))))))

(deftest replays-are-identical-for-a-seed
  (let [run #(world/run (duel) {:dt (/ 1.0 30.0) :max-ticks 400
                                :input-fn (fn [_ t] {1 {:fire? true :move-y (if (even? t) 1.0 -1.0)}})})
        a (run) b (run)]
    (is (= (:ticks a) (:ticks b)))
    (is (= (:standings a) (:standings b)))
    (is (= (map :kind (:events a)) (map :kind (:events b))))))

(deftest a-different-seed-produces-a-different-match
  (let [mk (fn [seed]
             (world/run (base {:seed seed
                               :entities (into {} [(s/actor 1 [0.0 0.0 0.0]
                                                             {:tag "player" :yaw 0.0 :weapon 0
                                                              :ammo 30 :reserve-ammo 900})
                                                   (s/actor 2 [0.0 0.0 -60.0]
                                                             {:tag "bot" :weapon 0 :ammo 30
                                                              :reserve-ammo 900})])})
                        {:dt (/ 1.0 30.0) :max-ticks 300
                         :input-fn (constantly {1 {:fire? true}})}))]
    (is (not= (map :amount (filter :amount (:events (mk 1))))
              (map :amount (filter :amount (:events (mk 2))))))))

(deftest a-full-match-terminates-with-a-winner
  ;; The full eight-phase schedule from battle_royale_storm.edn, which closes to
  ;; radius 0. This is the property the shipped royale does not have at all: it
  ;; spawns bots on a ring forever and has no state in which anyone has won.
  (let [ents (into {} (concat [(s/actor 0 [0.0 0.0 0.0]
                                        {:tag "player" :yaw 0.0 :weapon 0 :ammo 30 :reserve-ammo 9000})]
                              (for [i (range 1 7)]
                                (s/actor i [(* 12.0 (- i 3)) 0.0 -35.0]
                                         {:tag "bot" :weapon 0 :ammo 30 :reserve-ammo 9000
                                          :move-speed 5.0}))))
        r (world/run (world/make-world {:scene {:zone {:start-radius 1000.0 :center [0.0 0.0 0.0]}}
                                        :weapons s/weapons-raw
                                        :storm nil
                                        :seed 20260822
                                        :entities ents})
                     {:dt 0.1 :max-ticks 12000
                      :input-fn (constantly {0 {:fire? true}})})]
    (is (:ended? r) (str "match did not terminate in " (:ticks r) " ticks; "
                         (match/alive-count (:world r)) " still alive"))
    (is (<= (match/alive-count (:world r)) 1))
    (is (= 7 (count (:standings r))))
    (is (every? pos? (map :place (:standings r))) "everyone gets a placement")
    (is (= (set (range 1 8)) (set (map :place (:standings r))))
        "placements are a permutation of 1..7 — no ties, no gaps")))

(deftest bots-fight-back-without-any-scripted-input
  (let [ents (into {} [(s/actor 0 [0.0 0.0 0.0]
                                {:tag "player" :yaw 0.0 :weapon 0 :ammo 30 :reserve-ammo 9000})
                       (s/actor 1 [0.0 0.0 -30.0]
                                {:tag "bot" :yaw 0.0 :weapon 0 :ammo 30 :reserve-ammo 9000})])
        r (world/run (base {:entities ents})
                     {:dt (/ 1.0 20.0) :max-ticks 3000 :input-fn (constantly {})})
        player-damage (filter #(and (= :hit (:kind %)) (= 0 (:target %))) (:events r))]
    (is (seq player-damage)
        "the player takes fire from a bot that was never told to shoot")))

(deftest bots-reload-and-run-dry-like-players
  (let [ents (into {} [(s/actor 0 [0.0 0.0 0.0] {:tag "player" :yaw 0.0 :health-max 10000 :health 10000})
                       (s/actor 1 [0.0 0.0 -30.0] {:tag "bot" :yaw 0.0 :weapon 0
                                                   :ammo 30 :reserve-ammo 30})])
        r (world/run (base {:entities ents})
                     {:dt (/ 1.0 20.0) :max-ticks 1200 :input-fn (constantly {})})
        bot (get-in r [:world :entities 1])]
    (is (= 0.0 (attr/get bot :reserve-ammo)) "it spent its reserve")
    (is (<= (attr/get bot :ammo) 30.0))
    (is (attr/alive? (get-in r [:world :entities 0])) "and could not kill a 10000-health player")))
