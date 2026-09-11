(ns kami.render.operator-fit-test
  (:require [clojure.test :refer [deftest is]]
            [kami.render.operator-fit :as fit]))

(deftest every-tier-resolves-a-valid-clear-two-hand-fit
  (doseq [tier [:hero :gameplay :crowd]
          :let [resolved (fit/resolve-fit {:tier tier :entity-id :operator/alpha})
                validation (:validation resolved)]]
    (is (:valid? validation))
    (is (empty? (:intersections validation)))
    (is (>= (:minimum-clearance validation)
            (get-in validation [:budgets :minimum-clearance])))
    (is (>= (:grip-separation validation)
            (get-in validation [:budgets :minimum-grip-separation])))
    (is (<= (:silhouette-occupancy validation)
            (get-in validation [:budgets :maximum-silhouette-occupancy])))))

(deftest rifle-is-in-front-and-does-not-cross-torso
  (let [resolved (fit/resolve-fit {:tier :gameplay})
        weapon (:weapon resolved)
        torso-errors (filter #(and (= :weapon-body (:kind %)) (= :torso (:body %)))
                             (get-in resolved [:validation :intersections]))]
    (is (empty? torso-errors))
    (is (< (get-in weapon [:transform :position 2]) -0.5))
    (is (= :combat/two-hand-aim (:pose-semantic weapon)))))

(deftest both-hands-contact-distinct-readable-sockets
  (let [resolved (fit/resolve-fit {})
        primary (get-in resolved [:weapon :sockets :primary-grip :world-position])
        support (get-in resolved [:weapon :sockets :support-grip :world-position])]
    (is (= primary (get-in resolved [:pose :joint-targets :hand-right :target])))
    (is (= support (get-in resolved [:pose :joint-targets :hand-left :target])))
    (is (not= primary support))
    (is (> (get-in resolved [:validation :grip-separation]) 0.34))))

(deftest shoulders-and-backpack-are-cleared-from-body-and-each-other
  (let [resolved (fit/resolve-fit {:tier :hero})
        constraints (get-in resolved [:validation :intersections])]
    (is (empty? constraints))
    (is (> (get-in resolved [:equipment :equipment/backpack :transform :position 2]) 0.4))
    (is (< (get-in resolved [:equipment :equipment/shoulder-left :transform :position 0]) -0.5))
    (is (> (get-in resolved [:equipment :equipment/shoulder-right :transform :position 0]) 0.5))))

(deftest validation-detects-regressions-geometrically
  (let [base (fit/resolve-fit {:tier :gameplay})
        crossing (assoc-in base [:weapon :volume]
                           {:kind :capsule :a [0.0 1.35 -0.5] :b [0.0 1.35 0.5] :radius 0.14})
        backpack-hit (assoc-in base [:equipment :equipment/backpack :volume :center]
                               [0.0 1.35 0.10])
        obscured (assoc-in base [:equipment :equipment/backpack :silhouette-area] 2.0)]
    (is (some #{:clearance} (:errors (fit/validate-fit crossing))))
    (is (some #{:clearance} (:errors (fit/validate-fit backpack-hit))))
    (is (some #{:silhouette-occupancy} (:errors (fit/validate-fit obscured))))))

(deftest deterministic-and-consumable-adjusted-attachment-data
  (let [a (fit/resolve-fit {:entity-id :operator/a})
        a2 (fit/resolve-fit {:entity-id :operator/a})
        b (fit/resolve-fit {:entity-id :operator/b})]
    (is (= (:weapon a) (:weapon a2)))
    (is (not= (get-in a [:weapon :transform]) (get-in b [:weapon :transform])))
    (doseq [[_ equipment] (:equipment a)]
      (is (map? (:transform equipment)))
      (is (map? (:source-mesh equipment))))))

(deftest photoreal-boundary-is-explicitly-unsupported
  (is (:same-api? fit/family-boundary))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (fit/resolve-fit {:family :photoreal}))))
