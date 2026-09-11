(ns kami.gameplay.aim-test
  (:require [clojure.test :refer [deftest is testing]]
            [kami.gameplay.aim :as aim]
            [kami.gameplay.vec3 :as v]
            [kami.gameplay.attributes :as attr]))

(def ^:private eps 1e-6)
(defn- close? [a b] (< (Math/abs (- (double a) (double b))) 1e-4))

(deftest look-direction-conventions
  (testing "yaw 0 looks down -Z"
    (let [d (aim/look-direction 0.0 0.0)]
      (is (close? (v/x d) 0.0)) (is (close? (v/y d) 0.0)) (is (close? (v/z d) -1.0))))
  (testing "yaw +pi/2 looks toward +X"
    (let [d (aim/look-direction (/ Math/PI 2) 0.0)]
      (is (close? (v/x d) 1.0)) (is (close? (v/z d) 0.0))))
  (testing "positive pitch looks up"
    (is (pos? (v/y (aim/look-direction 0.0 0.5)))))
  (testing "always unit length"
    (doseq [y [0.0 1.0 -2.5 3.0] p [-1.0 0.0 0.7]]
      (is (close? 1.0 (v/length (aim/look-direction y p)))))))

(deftest movement-is-camera-relative
  (testing "this is the single change that stops the game being top-down"
    (let [fwd (aim/move-vector 0.0 0.0 1.0)]
      (is (close? (v/z fwd) -1.0) "forward at yaw 0 is -Z")
      (is (close? (v/y fwd) 0.0) "movement stays on the ground plane"))
    (let [fwd (aim/move-vector (/ Math/PI 2) 0.0 1.0)]
      (is (close? (v/x fwd) 1.0) "turn the camera and forward turns with it")))
  (testing "strafe is perpendicular to forward"
    (let [f (aim/move-vector 0.7 0.0 1.0)
          r (aim/move-vector 0.7 1.0 0.0)]
      (is (close? 0.0 (v/dot f r))))))

(deftest diagonals-are-not-faster
  (let [d (aim/move-vector 0.0 1.0 1.0)]
    (is (close? 1.0 (v/length d))
        "unclamped diagonal input is how every player ends up running at 45 degrees")))

(deftest zero-input-is-zero-movement
  (is (close? 0.0 (v/length (aim/move-vector 1.23 0.0 0.0)))))

(deftest ads-rig-narrows-fov-and-shortens-the-boom
  (let [rigs (aim/rig {})
        hip (aim/blend-rig rigs 0.0)
        ads (aim/blend-rig rigs 1.0)
        mid (aim/blend-rig rigs 0.5)]
    (is (< (:fov ads) (:fov hip)) "narrowing FOV is what reads as magnification")
    (is (< (:distance ads) (:distance hip)))
    (is (< (:shoulder ads) (:shoulder hip)))
    (is (and (< (:fov ads) (:fov mid)) (< (:fov mid) (:fov hip)))
        "blended, not snapped — otherwise right-click teleports the camera")))

(deftest camera-is-behind-the-pawn-not-above-it
  (let [rigs (aim/rig {})
        pose (aim/camera-pose [0.0 0.0 0.0] 0.0 0.0 (:hip rigs))]
    (is (pos? (v/z (:eye pose)))
        "at yaw 0 the pawn looks toward -Z, so the camera sits at +Z behind it")
    (is (< (v/dist-xz (:eye pose) [0.0 0.0 0.0]) 8.0)
        "a third-person boom is metres, not the 27-unit aerial rig the scene ships")
    (is (< (v/y (:eye pose)) 3.0)
        "and it is at shoulder height, not 15 units up")))

(deftest camera-pulls-in-when-blocked
  (let [rigs (aim/rig {})
        free (aim/camera-pose [0.0 0.0 0.0] 0.0 0.0 (:hip rigs))
        wall (aim/camera-pose [0.0 0.0 0.0] 0.0 0.0 (:hip rigs) (fn [_ _ _] 1.2))]
    (is (< (:distance wall) (:distance free)))
    (is (>= (:distance wall) (:min-distance (:hip rigs)))
        "never closer than the minimum, or the camera ends up inside the pawn")))

(deftest pitch-is-clamped-by-the-rig
  (let [r (:hip (aim/rig {}))]
    (is (= (:pitch-max r) (aim/clamp-pitch r 99.0)))
    (is (= (:pitch-min r) (aim/clamp-pitch r -99.0)))))

(deftest apply-look-accumulates
  (let [rigs (aim/rig {})
        e {:tag "player" :attrs {}}
        e' (aim/apply-look e rigs 100.0 0.0 0.01)]
    (is (close? 1.0 (attr/get e' :yaw)))
    (let [e'' (aim/apply-look e' rigs 100.0 0.0 0.01)]
      (is (close? 2.0 (attr/get e'' :yaw))))))

(deftest ads-blend-moves-toward-the-goal-over-time
  (let [e {:tag "p" :attrs {}}
        held (reduce (fn [x _] (aim/step-ads x true 0.1 6.0)) e (range 3))
        released (reduce (fn [x _] (aim/step-ads x false 0.1 6.0)) held (range 3))]
    (is (> (attr/get held :ads) 0.0))
    (is (<= (attr/get held :ads) 1.0))
    (is (< (attr/get released :ads) (attr/get held :ads)))
    (is (>= (attr/get released :ads) 0.0))))

(deftest eye-position-is-not-the-feet
  (let [p (aim/eye-position [1.0 0.0 2.0] 1.6)]
    (is (close? 1.6 (v/y p)) "shots trace from the eye; from the feet they hit the ground")
    (is (close? 1.0 (v/x p)))))
