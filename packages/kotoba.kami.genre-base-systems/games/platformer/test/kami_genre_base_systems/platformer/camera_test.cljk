(ns kami-genre-base-systems.platformer.camera-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami-genre-base-systems.platformer.camera :as camera]))

(def room {:x 0.0 :y 0.0 :width 1000.0 :height 600.0})
(def viewport {:width 400.0 :height 300.0})

(deftest clamp-camera-stays-in-room-bounds
  (testing "centers on the player when far from every edge"
    (is (= {:x 300.0 :y 150.0}
           (camera/clamp-camera room {:x 500.0 :y 300.0} viewport))))
  (testing "clamps to the left/top edge instead of showing outside the room"
    (is (= {:x 0.0 :y 0.0}
           (camera/clamp-camera room {:x 0.0 :y 0.0} viewport))))
  (testing "clamps to the right/bottom edge instead of showing outside the room"
    (is (= {:x 600.0 :y 300.0}
           (camera/clamp-camera room {:x 999.0 :y 599.0} viewport))))
  (testing "centers on a room axis smaller than the viewport"
    (let [small-room {:x 0.0 :y 0.0 :width 200.0 :height 100.0}]
      (is (= {:x -100.0 :y -100.0}
             (camera/clamp-camera small-room {:x 100.0 :y 50.0} viewport))))))

(deftest step-camera-hard-locks-when-idle
  (let [state (camera/camera-idle room {:x 500.0 :y 300.0} viewport)
        next-state (camera/step-camera state {:x 520.0 :y 300.0} viewport 0.016)]
    (is (false? (:scrolling? next-state)))
    (is (= {:x 320.0 :y 150.0} (camera/clamp-camera room {:x 520.0 :y 300.0} viewport)))
    (is (= 320.0 (:camera-x next-state)))
    (is (= 150.0 (:camera-y next-state)))))

(deftest step-camera-room-transition-lerps-then-arrives
  (let [room-b {:x 1000.0 :y 0.0 :width 1000.0 :height 600.0}
        start (camera/camera-idle room {:x 500.0 :y 300.0} viewport)
        transitioning (camera/start-room-transition start room-b {:x 1000.0 :y 300.0} viewport)]
    (is (true? (:scrolling? transitioning)))
    (is (= room-b (:room transitioning)))
    (let [after-one-tick (camera/step-camera transitioning {:x 1000.0 :y 300.0} viewport 0.016)]
      (is (true? (:scrolling? after-one-tick)))
      (is (not= (:camera-x transitioning) (:camera-x after-one-tick))
          "camera should have moved partway toward the target, not jumped or stayed put"))
    (let [arrived (camera/step-camera transitioning {:x 1000.0 :y 300.0} viewport 100.0)]
      (is (false? (:scrolling? arrived)) "a huge dt should be enough to finish the transition")
      (is (= (:scroll-target transitioning) {:x (:camera-x arrived) :y (:camera-y arrived)})))))
