(ns kami.gameplay.ai-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami.gameplay.ai :as ai]
            [kami.gameplay.perception :as perc]
            [kami.gameplay.vec3 :as v]
            [kami.gameplay.rng :as rng]
            [kami.gameplay.attributes :as attr]
            [kami.gameplay.weapon :as weapon]
            [kami.gameplay.support :as s]))

(def senses (perc/senses {}))
(def prof ai/default-profile)

(defn- bot-with [wi pos yaw]
  (s/actor 1 pos (merge {:tag "bot" :yaw yaw}
                        (when wi {:weapon wi :ammo 30}))))

(defn- decide [world & {:keys [zone] :or {zone nil}}]
  (ai/decide world s/table 1 {:zone zone :senses senses :prof prof :now-ms 0}))

(deftest state-codes-round-trip
  (doseq [st ai/states]
    (is (= st (ai/state-of (ai/set-state {:attrs {}} st))))))

(deftest an-unaware-bot-idles
  (is (= :idle (:state (decide (s/world-of (bot-with 0 [0.0 0.0 0.0] 0.0)))))
      "bots that are not hunting you are what makes a map feel populated"))

(deftest the-storm-outranks-a-visible-target
  (let [world (s/world-of (bot-with 0 [900.0 0.0 0.0] 0.0)
                          (s/actor 2 [880.0 0.0 -20.0] {:tag "player"}))
        d (decide world :zone {:center [0.0 0.0 0.0] :radius 100.0 :dps 5.0})]
    (is (= :rotate (:state d)) "a bot that fights while dying to the storm reads as broken")
    (is (< (v/dist-xz (:goal d) [0.0 0.0 0.0]) 100.0) "and it heads inside the circle")))

(deftest a-bot-inside-the-circle-is-free-to-fight
  (let [world (s/world-of (bot-with 0 [0.0 0.0 0.0] 0.0)
                          (s/actor 2 [0.0 0.0 -45.0] {:tag "player"}))
        d (decide world :zone {:center [0.0 0.0 0.0] :radius 500.0 :dps 5.0})]
    (is (not= :rotate (:state d)))))

(deftest range-preference-follows-the-weapon-it-drew
  (testing "each archetype fights inside its own full-damage envelope"
    ;; 0.9 x :damage-falloff — AR 50, shotgun 10, sniper 200
    (is (< (Math/abs (- (ai/preferred-range s/table (second (bot-with 0 [0.0 0.0 0.0] 0.0)) prof)
                        45.0)) 1e-6))
    (is (< (Math/abs (- (ai/preferred-range s/table (second (bot-with 1 [0.0 0.0 0.0] 0.0)) prof)
                        9.0)) 1e-6))
    (is (< (Math/abs (- (ai/preferred-range s/table (second (bot-with 2 [0.0 0.0 0.0] 0.0)) prof)
                        180.0)) 1e-6))
    (is (< (ai/preferred-range s/table (second (bot-with 1 [0.0 0.0 0.0] 0.0)) prof)
           (ai/preferred-range s/table (second (bot-with 0 [0.0 0.0 0.0] 0.0)) prof)
           (ai/preferred-range s/table (second (bot-with 2 [0.0 0.0 0.0] 0.0)) prof))
        "shotgun closest, sniper furthest, rifle between"))
  (testing "an unarmed bot wants to be close — it has to loot or melee"
    (is (= 6.0 (ai/preferred-range s/table (second (s/actor 1 [0.0 0.0 0.0])) prof)))))

(deftest a-shotgun-bot-closes-and-a-sniper-bot-backs-off
  ;; 60 units: past the shotgun bot's 9 and short of the sniper bot's 180.
  (let [player (s/actor 2 [0.0 0.0 -60.0] {:tag "player"})
        close (decide (s/world-of (bot-with 1 [0.0 0.0 0.0] 0.0) player))
        far (decide (s/world-of (bot-with 2 [0.0 0.0 0.0] 0.0) player))]
    (is (= :reposition (:state close)))
    (is (< (v/dist-xz (:goal close) [0.0 0.0 -60.0]) 60.0) "the shotgun bot moves toward")
    (is (= :reposition (:state far)))
    (is (> (v/dist-xz (:goal far) [0.0 0.0 -60.0]) 60.0) "the sniper bot backs off")))

(deftest at-preferred-range-it-engages
  (let [world (s/world-of (bot-with 0 [0.0 0.0 0.0] 0.0)
                          (s/actor 2 [0.0 0.0 -45.0] {:tag "player"}))
        d (decide world)]
    (is (= :engage (:state d)))
    (is (= 2 (:target d)))
    (is (nil? (:goal d)) "engaging means holding the line, not walking into contact")))

(deftest a-heard-contact-is-investigated-not-engaged
  (let [world (s/world-of (bot-with 0 [0.0 0.0 0.0] 0.0)
                          (s/actor 2 [0.0 0.0 20.0] {:tag "player"}))
        d (decide world)]
    (is (= :investigate (:state d)))
    (is (some? (:goal d)))))

(deftest memory-keeps-a-bot-searching-after-line-of-sight-breaks
  (let [remembered (-> (second (bot-with 0 [0.0 0.0 0.0] 0.0))
                       (perc/remember 2 0 senses))
        world {:entities {1 remembered
                          2 (second (s/actor 2 [0.0 0.0 900.0] {:tag "player"}))}}
        d (ai/decide world s/table 1 {:senses senses :prof prof :now-ms 1000})]
    (is (= :investigate (:state d)))
    (is (= 2 (:target d)))))

(deftest aim-error-is-bounded-and-deterministic
  (let [e (second (bot-with 0 [0.0 0.0 0.0] 0.0))
        [a _] (ai/aim-at (rng/seed 3) e [0.0 0.0 -50.0] prof)
        [b _] (ai/aim-at (rng/seed 3) e [0.0 0.0 -50.0] prof)
        err (* (:aim-error-degrees prof) (/ Math/PI 180.0))]
    (is (= (:attrs a) (:attrs b)) "replays identically")
    (is (<= (Math/abs (attr/get a :yaw)) (+ err 1e-9))
        "a bot aiming at a target dead ahead ends up within its error cone, not beyond it")))

(deftest aim-turns-toward-the-target
  (let [e (second (bot-with 0 [0.0 0.0 0.0] 0.0))
        [right _] (ai/aim-at (rng/seed 1) e [50.0 0.0 0.0] {:aim-error-degrees 0.0})
        [up _] (ai/aim-at (rng/seed 1) e [0.0 20.0 -5.0] {:aim-error-degrees 0.0})]
    (is (< (Math/abs (- (attr/get right :yaw) (/ Math/PI 2))) 1e-6))
    (is (pos? (attr/get up :pitch)) "and looks up at a target above it")))

(deftest apply-decision-writes-velocity-toward-the-goal
  (let [world (s/world-of (bot-with 0 [0.0 0.0 0.0] 0.0))
        e (get-in world [:entities 1])
        [e' _] (ai/apply-decision (rng/seed 1) world e
                                  {:state :rotate :goal [0.0 0.0 -100.0] :target nil}
                                  {:senses senses :prof prof :now-ms 0})]
    (is (neg? (v/z (:vel e'))))
    (is (< (Math/abs (- (v/length (:vel e')) (attr/get e' :move-speed))) 1e-6))
    (is (= :rotate (ai/state-of e'))))
  (testing "engaging sights the weapon; anything else lowers it"
    (let [world (s/world-of (bot-with 0 [0.0 0.0 0.0] 0.0))
          e (get-in world [:entities 1])
          [eng _] (ai/apply-decision (rng/seed 1) world e
                                     {:state :engage :goal nil :target nil}
                                     {:senses senses :prof prof :now-ms 0})
          [rot _] (ai/apply-decision (rng/seed 1) world e
                                     {:state :rotate :goal [0.0 0.0 -9.0] :target nil}
                                     {:senses senses :prof prof :now-ms 0})]
      (is (= 1.0 (attr/get eng :ads)))
      (is (= 0.0 (attr/get rot :ads)))))
  (testing "and stops when it has arrived"
    (let [world (s/world-of (bot-with 0 [0.0 0.0 0.0] 0.0))
          e (get-in world [:entities 1])
          [e' _] (ai/apply-decision (rng/seed 1) world e
                                    {:state :rotate :goal [0.0 0.0 0.1] :target nil}
                                    {:senses senses :prof prof :now-ms 0})]
      (is (< (v/length (:vel e')) 1e-9)))))

(deftest repositioning-is-slower-than-rotating
  (let [world (s/world-of (bot-with 0 [0.0 0.0 0.0] 0.0))
        e (get-in world [:entities 1])
        [fast _] (ai/apply-decision (rng/seed 1) world e
                                    {:state :rotate :goal [0.0 0.0 -99.0]} {:senses senses :prof prof :now-ms 0})
        [slow _] (ai/apply-decision (rng/seed 1) world e
                                    {:state :reposition :goal [0.0 0.0 -99.0]} {:senses senses :prof prof :now-ms 0})]
    (is (< (v/length (:vel slow)) (v/length (:vel fast))) "strafing, not sprinting into contact")))

(deftest an-idle-bot-sweeps-its-view-cone
  ;; Sight is a cone. Without this a bot can only ever notice what walks into
  ;; the facing it spawned with — and bots dropped on a ring all spawn facing
  ;; the same way, so in a measured 24-entrant match not one of them ever
  ;; acquired the player.
  (let [world (s/world-of (bot-with 0 [0.0 0.0 0.0] 0.0))
        e (get-in world [:entities 1])
        [e1 rs] (ai/apply-decision (rng/seed 1) world e {:state :idle} {:prof prof :dt 0.1})
        [e2 _] (ai/apply-decision rs world e1 {:state :idle} {:prof prof :dt 0.1})]
    (is (not= (attr/get e :yaw) (attr/get e1 :yaw)) "an idle bot turns")
    (is (not= (attr/get e1 :yaw) (attr/get e2 :yaw)))
    (is (= (attr/get e1 :ai-scan) (attr/get e2 :ai-scan))
        "and keeps sweeping the same way rather than jittering")
    (is (< (Math/abs (- (attr/get e1 :yaw) (attr/get e :yaw)))
           (+ 1e-9 (* 0.1 (:scan-rate-rad-s prof))))
        "at no more than the configured rate")))

(deftest sweeping-eventually-acquires-a-target-outside-the-spawn-cone
  (let [;; directly behind the bot: outside a 110-degree cone by a wide margin,
        ;; and with hearing turned down so sight is the only way to find it
        quiet (assoc senses :hearing-range 1.0 :gunshot-hearing-range 1.0)
        world (s/world-of (bot-with 0 [0.0 0.0 0.0] 0.0)
                          (s/actor 2 [0.0 0.0 60.0] {:tag "player"}))
        step (fn [w t]
               (let [e (get-in w [:entities 1])
                     d (ai/decide w s/table 1 {:senses quiet :prof prof :now-ms (* t 100)})
                     [e' _] (ai/apply-decision (rng/seed 1) w e d
                                               {:prof prof :dt 0.1 :senses quiet :now-ms (* t 100)})]
                 [(assoc-in w [:entities 1] e') d]))
        acquired-at (loop [w world t 0]
                      (if (> t 200)
                        nil
                        (let [[w' d] (step w t)]
                          (if (:target d) t (recur w' (inc t))))))]
    (is (nil? (:target (ai/decide world s/table 1
                                  {:senses quiet :prof prof :now-ms 0})))
        "it starts unaware")
    (is (some? acquired-at)
        "a sweeping bot never found a target standing 60 units behind it in 20 seconds")
    (is (< acquired-at 120)
        (str "acquisition took " acquired-at " ticks; a full sweep at "
             (:scan-rate-rad-s prof) " rad/s should need far fewer"))))
