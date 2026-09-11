(ns kami.render.capture-lifecycle-test
  (:require [clojure.test :refer [deftest is]]
            [kami.render.capture-lifecycle :as lifecycle]))

(def local-bounds {:min [-0.62 0.0 -0.45] :max [0.62 2.08 0.45]})

(defn- requested []
  (lifecycle/request-freeze
   (lifecycle/initial-state)
   {:subject-id :operator/hero :subject-local-bounds local-bounds :orbit :front}))

(defn- settle [state transform]
  (reduce (fn [s tick]
            (lifecycle/snapshot-tick s {:tick tick :subject-id :operator/hero
                                        :subject-transform transform
                                        :submitted-render-frame-count (inc tick)
                                        :submitted-subject-presence
                                        {:schema lifecycle/capture-presence-schema
                                         :submitted? true :submit-sequence tick
                                         :entity-ids #{:operator/hero}
                                         :submitted-roles #{:subject/skinned}
                                         :draw-count 1
                                         :projected-screen-bounds
                                         {:min [0.35 0.28] :max [0.65 0.72]}
                                         :cache-evidence {:hit? false}
                                         :backend-health {:healthy? true}}}))
          state (range 3)))

(deftest production-capture-binds-camera-to-current-far-world-transform
  (let [transform {:translation [782.0 0.0 -213.0] :rotation-y-deg 37.0}
        frozen (settle (requested) transform)
        bounds (get-in frozen [:evidence :subject-world-bounds])]
    (is (= :frozen (:phase frozen)))
    (is (< 781.0 (get-in bounds [:min 0]) (get-in bounds [:max 0]) 783.0))
    (is (= 8 (count (get-in frozen [:evidence :projected-corners]))))
    (is (get-in frozen [:evidence :fully-inside-safe-frame?]))
    (is (get-in frozen [:evidence :meaningfully-intersects-safe-frame?]))
    (is (get-in frozen [:evidence :ground-contact?]))
    (is (zero? (get-in frozen [:evidence :stale-camera-delta])))
    (is (get-in frozen [:evidence :submitted-subject-presence-valid?]))
    (is (= 3 (get-in frozen [:evidence :submitted-render-frame-count])))
    (is (= #{:operator/hero}
           (get-in frozen [:render-selection :skinned :entity-ids])))
    (is (= :preserve-all (get-in frozen [:render-selection :world :mode])))))

(deftest arbitrary-translations-and-rotations-resolve-deterministically
  (doseq [transform [{:translation [-44.0 3.0 91.0] :rotation-y-deg -123.0}
                     {:translation [0.25 0.0 0.75] :rotation-y-deg 270.0}]]
    (let [a (settle (requested) transform)
          b (settle (requested) transform)]
      (is (= a b))
      (is (= (:translation transform)
             (get-in a [:evidence :subject-world-transform :translation]))))))

(deftest movement-or-absence-during-settle-fails-closed
  (let [state (lifecycle/snapshot-tick
               (requested) {:tick 10 :subject-id :operator/hero
                            :subject-transform {:translation [782.0 0.0 0.0]}})]
    (is (thrown-with-msg? #?(:clj Exception :cljs js/Error) #"moved during settle"
                          (lifecycle/snapshot-tick
                           state {:tick 11 :subject-id :operator/hero
                                  :subject-transform {:translation [782.01 0.0 0.0]}}))))
  (is (thrown-with-msg? #?(:clj Exception :cljs js/Error) #"subject absent"
                        (lifecycle/snapshot-tick
                         (requested) {:tick 0 :subject-id :operator/missing
                                      :subject-transform {:translation [0.0 0.0 0.0]}}))))

(deftest frozen-settle-and-release-reset-are-deterministic
  (let [frozen (settle (requested) {:position [12.0 0.0 -8.0] :yaw-deg 15.0})
        released-a (lifecycle/release frozen)
        released-b (lifecycle/release frozen)]
    (is (= released-a released-b))
    (is (= :released (:phase released-a)))
    (is (= {:contract lifecycle/contract :phase :idle :generation 1}
           (lifecycle/reset released-a)))))

(deftest missing-actual-queue-submission-and-timeout-fail-closed
  (let [state (requested)]
    (is (thrown-with-msg? #?(:clj Exception :cljs js/Error) #"presence failed closed"
                          (reduce (fn [s tick]
                                    (lifecycle/snapshot-tick
                                     s {:tick tick :subject-id :operator/hero
                                        :subject-transform {:position [0.0 0.0 0.0]}}))
                                  state (range 3))))
    (is (= :timeout (:state (lifecycle/timeout state 5000))))
    (is (false? (:frozen? (lifecycle/timeout state 5000))))))

(deftest non-authoritative-webgpu-presence-schema-fails-closed
  (let [bad-presence {:schema :kotoba.webgpu/submitted-subject-presence-v2
                      :submitted? true :entity-ids #{:operator/hero} :draw-count 1}]
    (is (= :kotoba.webgpu/capture-presence-evidence-v2
           lifecycle/capture-presence-schema))
    (is (thrown-with-msg?
         #?(:clj Exception :cljs js/Error) #"presence failed closed"
         (reduce (fn [state tick]
                   (lifecycle/snapshot-tick
                    state {:tick tick :subject-id :operator/hero
                           :subject-transform {:position [0.0 0.0 0.0]}
                           :submitted-render-frame-count (inc tick)
                           :submitted-subject-presence bad-presence}))
                 (requested) (range 3))))))
