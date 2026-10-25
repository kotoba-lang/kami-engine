(ns kami.modeler.vehicle
  "Render/import a shared vehicle document without owning physics math."
  (:require [kami.modeler.project :as project]
            [kami.modeling :as modeling]
            [kotoba.physics.vehicle :as vehicle]))

(defn- position [node]
  (mapv #(double (get (:position node) % 0.0)) [:x :y :z]))

(defn- box-mesh [a b thickness]
  (let [lo (mapv #(- (min %1 %2) thickness) a b)
        hi (mapv #(+ (max %1 %2) thickness) a b)
        [x0 y0 z0] lo [x1 y1 z1] hi]
    (modeling/mesh [[x0 y0 z0] [x1 y0 z0] [x1 y1 z0] [x0 y1 z0]
                    [x0 y0 z1] [x1 y0 z1] [x1 y1 z1] [x0 y1 z1]]
                   [[0 3 2 1] [4 5 6 7] [0 1 5 4]
                    [3 7 6 2] [0 4 7 3] [1 2 6 5]])))

(defn- node-object [idx node]
  (-> (modeling/object idx (str "Physics node " (:id node)) (modeling/cube 0.07)
                       {:translation (position node)
                        :material (merge project/default-material
                                         {:material/base-color [0.2 0.75 1.0 1.0]})})
      (assoc-in [:object/mesh :mesh/type] :physics-node)
      (assoc-in [:object/mesh :node/id] (:id node))))

(defn- beam-object [node-by-id base idx beam]
  (let [a (position (get node-by-id (:n1 beam)))
        b (position (get node-by-id (:n2 beam)))
        object (modeling/object (+ base idx) (str "Physics beam " (:id beam))
                                (box-mesh a b 0.018)
                                {:material (merge project/default-material
                                                  {:material/base-color [1.0 0.55 0.15 1.0]})})]
    (-> object
        (assoc-in [:object/mesh :mesh/type] :physics-beam)
        (assoc-in [:object/mesh :beam/id] (:id beam))
        (assoc-in [:object/mesh :node-a] (:n1 beam))
        (assoc-in [:object/mesh :node-b] (:n2 beam))
        (assoc-in [:object/mesh :broken] (boolean (:broken beam))))))

(defn open-document [doc]
  (when-not (vehicle/document? doc)
    (throw (ex-info "not a shared vehicle document" {:document doc})))
  (let [nodes (vec (get-in doc [:vehicle/structure :nodes]))
        beams (vec (get-in doc [:vehicle/structure :beams]))
        node-by-id (into {} (map (juxt :id identity) nodes))
        objects (into (mapv node-object (range) nodes)
                      (map-indexed (partial beam-object node-by-id (count nodes)) beams))]
    (project/open
     (project/document {:id (str (:vehicle/id doc)) :name (:vehicle/name doc)
                        :scene {:scene/objects objects}
                        :selection {:object-id (some-> objects first :object/id) :face-id 0 :mode :object}
                        :camera {:azimuth 0.7 :elevation 0.45}
                        :interaction {:profile :vehicle-physics}}))))

(defn sample-document []
  (let [points [[-1.0 0.2 -1.8] [1.0 0.2 -1.8] [-1.0 0.2 1.8] [1.0 0.2 1.8]
                [-0.8 1.0 -1.2] [0.8 1.0 -1.2] [-0.8 1.0 1.2] [0.8 1.0 1.2]]
        nodes (mapv (fn [id [x y z]] {:id id :position {:x x :y y :z z}}) (range) points)
        pairs [[0 1] [0 2] [1 3] [2 3] [4 5] [4 6] [5 7] [6 7]
               [0 4] [1 5] [2 6] [3 7] [0 5] [1 4] [2 7] [3 6]]
        beams (mapv (fn [id [a b]] {:id id :n1 a :n2 b}) (range) pairs)]
    (vehicle/document {:id :studio/vehicle-physics
                       :name "Vehicle Physics Workspace"
                       :preset :sports
                       :structure {:nodes nodes :beams beams}
                       :provenance {:authority :kotoba/kami-studio}})))
