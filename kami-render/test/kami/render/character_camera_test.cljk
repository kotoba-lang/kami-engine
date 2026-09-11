(ns kami.render.character-camera-test
  (:require [clojure.test :refer [deftest is]]
            [kami.render.character-camera :as camera]))

(def subject {:min [-0.62 0.0 -0.45] :max [0.62 2.08 0.45]})

(deftest production-framing-is-within-coverage-and-safe-margins
  (doseq [orbit [:front :three-quarter-right :three-quarter-left]
          :let [resolved (camera/resolve-camera {:subject-bounds subject :orbit orbit})
                evidence (:evidence resolved)]]
    (is (<= 0.28 (:coverage evidence) 0.42))
    (is (>= (:head-safe-margin evidence) 0.12))
    (is (>= (:feet-safe-margin evidence) 0.12))
    (is (:ground-visible? evidence))
    (is (:horizon-visible? evidence))
    (is (:environment-context-retained? evidence))))

(deftest renderer-and-studio-consume-explicit-camera-and-preserved-world-selection
  (let [resolved (camera/resolve-camera {:subject-id :operator/hero :subject-bounds subject})]
    (is (= #{:operator/hero} (get-in resolved [:render-selection :skinned :entity-ids])))
    (is (= :subject-only (get-in resolved [:render-selection :skinned :mode])))
    (is (= :preserve-all (get-in resolved [:render-selection :world :mode])))
    (is (false? (get-in resolved [:render-selection :world :removed?])))
    (doseq [field [:position :look-at :up :vertical-fov-deg :near :far :viewport]]
      (is (some? (get-in resolved [:camera field]))))))

(deftest occlusion-avoidance-settles-deterministically
  (let [obstacle {:min [-0.3 0.0 -2.5] :max [0.3 2.5 -0.5]}
        input {:subject-id :operator/a :subject-bounds subject :orbit :front :obstacles [obstacle]}
        a (camera/resolve-camera input) b (camera/resolve-camera input)]
    (is (= a b))
    (is (not= 0.0 (get-in a [:orbit :resolved-yaw-deg])))
    (is (pos? (get-in a [:settle :attempt-index])))
    (is (zero? (get-in a [:settle :position-delta])))
    (is (= 3 (get-in a [:settle :stable-frames])))))

(deftest collision-ground-clearance-and-fail-closed-evidence
  (let [resolved (camera/resolve-camera {:subject-bounds subject :ground-y 1.0})]
    (is (>= (get-in resolved [:evidence :ground-clearance]) 0.35)))
  (let [wall {:min [-100.0 -1.0 -100.0] :max [100.0 100.0 100.0]}]
    (is (thrown-with-msg? #?(:clj Exception :cljs js/Error) #"failed closed"
                          (camera/resolve-camera {:subject-bounds subject :obstacles [wall]})))))

(deftest photoreal-shares-boundary-but-is-not-claimed
  (is (:same-api? camera/family-boundary))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (camera/resolve-camera {:family :photoreal :subject-bounds subject}))))
