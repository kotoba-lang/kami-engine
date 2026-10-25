(ns kami.cad.dimensional-sketch (:require [kami.cad :as cad]))

(def default-dimensions {:width 4.0 :height 2.0})

(defn valid-dimensions? [{:keys [width height]}]
  (and (number? width) (pos? width) (number? height) (pos? height)))

(defn section-sketch
  "Build the dimensional construction sketch used by every loft section."
  [{:keys [width height] :as dimensions}]
  (when-not (valid-dimensions? dimensions)
    (throw (ex-info "Sketch dimensions must be positive" {:dimensions dimensions})))
  (let [half (/ width 2.0)]
    (cad/sketch [(cad/sketch-point :origin 0 0 true)
                 (cad/sketch-point :left (- half 0.17) 0.12)
                 (cad/sketch-point :right (+ half 0.23) -0.15)
                 (cad/sketch-point :crown 0.11 (+ height 0.19))]
                [(cad/sketch-line :left-axis :origin :left)
                 (cad/sketch-line :right-axis :origin :right)
                 (cad/sketch-line :height-axis :origin :crown)]
                [(cad/horizontal :left-horizontal :left-axis)
                 (cad/horizontal :right-horizontal :right-axis)
                 (cad/vertical :crown-vertical :height-axis)
                 (cad/distance-constraint :left-radius :origin :left half)
                 (cad/distance-constraint :right-radius :origin :right half)
                 (cad/distance-constraint :crown-height :origin :crown height)])))

(defn solve-dimensions [dimensions]
  (cad/solve-sketch (section-sketch dimensions)))

(defn apply-to-section [section solved]
  (let [[left-x _] (get-in solved [:sketch/points :left :sketch.point/position])
        [crown-x crown-y] (get-in solved [:sketch/points :crown :sketch.point/position])
        [right-x _] (get-in solved [:sketch/points :right :sketch.point/position])
        points (:cad/control-points section)]
    (when-not (= 3 (count points))
      (throw (ex-info "Dimensional section requires three control points" {:point-count (count points)})))
    (assoc section :cad/control-points
           (mapv (fn [[x y] [_ _ z]] [x y z])
                 [[(- (#?(:clj Math/abs :cljs js/Math.abs) left-x)) 0]
                  [crown-x crown-y]
                  [(#?(:clj Math/abs :cljs js/Math.abs) right-x) 0]] points))))

(defn solve-sections [sections dimensions]
  (let [solved (solve-dimensions dimensions)]
    {:sections (mapv #(apply-to-section % solved) sections)
     :solver (:sketch/solver solved)
     :sketch solved}))
