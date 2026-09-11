(ns kami.render.operator-face-mesh-test
  (:require [clojure.test :refer [deftest is]]
            [kami.render.operator-face-mesh :as face]))

(deftest readable-face-parts-have-actual-indexed-curved-geometry
  (doseq [tier [:hero :gameplay :crowd]
          :let [resolved (face/resolve-face {:tier tier})]
          [id part] (:parts resolved)]
    (is (contains? #{:eye-left :eye-right :eyebrow-left :eyebrow-right :nose :mouth} id))
    (is (pos? (count (get-in part [:mesh :indices]))))
    (is (= :operator-bind-world (:space part)))
    (is (= (count (get-in part [:mesh :indices]))
           (get-in part [:material-ranges 0 :index-count])))
    (is (map? (:material part)))))

(deftest face-stays-on-head-and-never-masks-torso
  (let [resolved (face/resolve-face {:tier :hero})
        {:keys [min max]} (:bounds resolved)]
    (is (> (nth min 1) 1.64))
    (is (< (nth max 1) 1.94))
    (is (< (Math/abs (double (nth min 0))) 0.20))
    (is (< (Math/abs (double (nth max 0))) 0.20))
    (is (false? (get-in resolved [:occupancy :torso-mask?])))
    (is (< (get-in resolved [:occupancy :ratio]) 0.20))))

(deftest face-lod-decreases-and-photoreal-is-honest
  (let [counts (map #(get-in (face/resolve-face {:tier %}) [:budget :triangle-count])
                    [:hero :gameplay :crowd])]
    (is (apply > counts)))
  (is (thrown? #?(:clj Exception :cljs js/Error)
               (face/resolve-face {:family :photoreal}))))
