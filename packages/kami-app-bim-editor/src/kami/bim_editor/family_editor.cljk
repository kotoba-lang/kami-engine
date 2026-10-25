(ns kami.bim-editor.family-editor
  (:require [kotoba.lang.text :as string]
            [bim.integration :as family]))

(defn family-key [value]
  (-> (or value "") string/trim string/lower
      (string/replace #"[^a-z0-9]+" "-")
      (string/replace #"(^-|-$)" "")))

(defn box-family
  "Build an editable, IFC-renderable parametric box family definition."
  [{:keys [id name category width depth height material shared? types]}]
  (let [id (family-key id)]
    (when (string/blank? id)
      (throw (ex-info "family id is required" {})))
    (family/family-definition
     {:id id :name (or (not-empty (string/trim (or name ""))) id)
      :category (or category :furniture) :shared? shared?
      :parameters {:width {:type :length :scope :type :default width :min 0.01}
                   :depth {:type :length :scope :instance :default depth :min 0.01}
                   :height {:type :length :scope :instance :default height :min 0.01}
                   :finish {:type :material :scope :instance :default material}}
      :formulas {:volume [:* [:param :width] [:param :depth] [:param :height]]}
      :types (or types {:standard {:id (str id "-standard") :name "Standard"
                                   :parameters {:width width}}})
      :template {:kind (or category :furniture) :name (or name id)
                 :global-id nil :material [:material-param :finish]
                 :placement {:location [0 0 0]}
                 :geometry {:kind :extruded-area-solid
                            :profile {:kind :rectangle :x-dim [:param :width]
                                      :y-dim [:param :depth]}
                            :position {:location [0 0 0]} :direction [0 0 1]
                            :depth [:param :height]}
                 :quantities {:gross-volume-m3 [:param :volume]}
                 :psets {} :openings [] :connected-to []}})))

(defn upsert-family [catalog definition]
  (assoc-in catalog [:family-catalog/families (:family/id definition)] definition))

(defn apply-parametric-schema
  "Apply advanced family parameters, lookup tables, formulas, datums,
  constraints, sketches, and template.
  A trial instance is resolved before the definition may enter the catalog."
  [definition {:keys [parameters lookup-tables formulas reference-planes constraints
                      sketches template]}]
  (when-not (and (map? parameters) (map? lookup-tables) (map? formulas)
                 (map? reference-planes) (vector? constraints)
                 (map? sketches) (map? template))
    (throw (ex-info "advanced family schema has invalid collection types"
                    {:parameters parameters :lookup-tables lookup-tables
                     :formulas formulas :reference-planes reference-planes
                     :constraints constraints :sketches sketches :template template})))
  (let [definition (assoc definition
                          :family/parameters parameters
                          :family/lookup-tables lookup-tables
                          :family/formulas formulas
                          :family/reference-planes reference-planes
                          :family/constraints constraints
                          :family/sketches sketches
                          :family/template template)]
    (family/instantiate-family definition "family-schema-validation" {})
    definition))

(defn upsert-type [catalog family-id type-name width]
  (let [type-key (keyword (family-key type-name))]
    (when-not (get-in catalog [:family-catalog/families family-id])
      (throw (ex-info "family not found" {:family-id family-id})))
    (when (string/blank? (name type-key))
      (throw (ex-info "family type name is required" {})))
    (assoc-in catalog [:family-catalog/families family-id :family/types type-key]
              {:id (str family-id "-" (name type-key)) :name type-name
               :parameters {:width width}})))
