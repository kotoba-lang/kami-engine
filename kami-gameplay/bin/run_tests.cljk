;; nbb entry point for the kami-gameplay suite.
;;
;;   npx --yes nbb bin/run_tests.cljs
;;
;; Exits non-zero on any failure or error, and — the part that matters — refuses
;; to report a pass when it ran no tests. A suite that silently discovers
;; nothing returns the same green as a suite that verified everything, which is
;; the failure mode this workspace keeps finding in its own gates.
(ns run-tests
  (:require [cljs.test :as t]
            [kami.gameplay.rng-test]
            [kami.gameplay.attributes-test]
            [kami.gameplay.aim-test]
            [kami.gameplay.weapon-test]
            [kami.gameplay.ballistics-test]
            [kami.gameplay.damage-test]
            [kami.gameplay.zone-test]
            [kami.gameplay.perception-test]
            [kami.gameplay.ai-test]
            [kami.gameplay.match-test]
            [kami.gameplay.world-test]
            [kami.gameplay.data-test]))

(def ^:private minimum-tests
  "Evidence floor. Raise it when the suite grows; never lower it to make a run
  pass."
  60)

(defmethod t/report [:cljs.test/default :end-run-tests] [m]
  (let [ran (+ (:pass m) (:fail m) (:error m))]
    (println (str "\nRan " (:test m) " tests containing " ran " assertions."))
    (println (str (:fail m) " failures, " (:error m) " errors."))
    (cond
      (< (:test m) minimum-tests)
      (do (println (str "REFUSING TO REPORT A PASS: only " (:test m)
                        " tests ran, floor is " minimum-tests
                        " — the suite did not load."))
          (js/process.exit 3))
      (t/successful? m) (js/process.exit 0)
      :else (js/process.exit 1))))

(t/run-tests 'kami.gameplay.rng-test
             'kami.gameplay.attributes-test
             'kami.gameplay.aim-test
             'kami.gameplay.weapon-test
             'kami.gameplay.ballistics-test
             'kami.gameplay.damage-test
             'kami.gameplay.zone-test
             'kami.gameplay.perception-test
             'kami.gameplay.ai-test
             'kami.gameplay.match-test
             'kami.gameplay.world-test
             'kami.gameplay.data-test)
