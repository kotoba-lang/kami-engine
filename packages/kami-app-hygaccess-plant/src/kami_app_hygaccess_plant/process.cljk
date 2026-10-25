(ns kami-app-hygaccess-plant.process
  "Batch/process parameters for the dilution/mixing step — `:process/...`
  namespaced keys, mirroring `tank.cljc`'s convention. Concentration figures are
  NOT invented: `:process/target-concentration-pct` and the efficacy window are
  copied from `cloud-itonami/cloud-itonami-hygiene-access`'s own registered data
  for `int.hygaccess.water-purification-drops`
  (`products.edn` `:hygaccess.product/concentration-pct 1.0`, and
  `src/hygaccess/registry.cljc` `efficacy-window-pct`
  `[:sodium-hypochlorite :water-purification-drops] [0.5 1.5]`, percent w/v) — see
  README.md 'Where the numbers come from'. `:process/stock-feed-concentration-pct`
  (the concentrated feedstock BEFORE this plant's dilution step) is this build's
  own representative choice: commercial technical-grade NaOCl solution is commonly
  supplied around 10-15% w/v, so 12% w/v is used as an illustrative feedstock
  figure, documented as such (not sourced from a specific supplier spec)."
  )

(def default-process
  "The one illustrative process this build simulates (see README.md 'Scope')."
  #:process
   {:product-sku "int.hygaccess.water-purification-drops"
    :active :sodium-hypochlorite
    :stock-feed-concentration-pct 12.0     ;; representative technical-grade NaOCl feedstock
    :target-concentration-pct 1.0          ;; cloud-itonami-hygiene-access products.edn
    :efficacy-window-pct [0.5 1.5]         ;; cloud-itonami-hygiene-access registry.cljc
    :mixing-duration-s 90.0                ;; representative small-batch agitation window
    :turbulent-diffusivity-m2-s 3.0e-3     ;; representative agitated-tank eddy diffusivity
                                            ;; (orders of magnitude above NaOCl's molecular
                                            ;; diffusivity in water, ~1.5e-9 m^2/s — turbulent
                                            ;; mixing, not molecular diffusion, dominates)
    ;; The field name a QC / batch-record data model (e.g. the sibling
    ;; cloud-itonami-hygiene-access GMP/QA-data-model work) could cite this
    ;; simulation's headline output number under. Loose EDN-level naming
    ;; convention only — no code dependency on that repo.
    :qc-field-reference :mixing-homogeneity-cov-pct})

(defn dilution-ratio
  "Stock-feed : target dilution ratio (e.g. 12.0 for a 12% feed diluted to 1%
  target) — how many volumes of water dilute one volume of concentrated feed."
  [{:process/keys [stock-feed-concentration-pct target-concentration-pct]}]
  (/ (double stock-feed-concentration-pct) (double target-concentration-pct)))

(defn within-efficacy-window?
  "Is `concentration-pct` within this process's registered efficacy window?"
  [{:process/keys [efficacy-window-pct]} concentration-pct]
  (let [[lo hi] efficacy-window-pct]
    (<= (double lo) (double concentration-pct) (double hi))))

(defn default-tank
  "The one illustrative tank this build simulates — a small-batch cylindrical
  mixing tank approximated by its vertical 2-D cross-section (see tank.cljc
  docstring). Dimensions are representative of a small-batch (tens-of-liters)
  production tank, not a procurement spec."
  []
  #:tank
   {:id :hygaccess-mixing-tank-01
    :name "Water-purification-drops dilution/mixing tank (illustrative)"
    :product-sku "int.hygaccess.water-purification-drops"
    :cross-section-m [0.60 0.60]     ;; 60cm x 60cm vertical cross-section
    :depth-m 0.60                    ;; unit-depth assumption's real-world depth
    :mesh-resolution [24 24]
    :agitation #:agitation{:type :jet-shear-drive
                            :drive-velocity-m-s 0.5
                            :description
                            "Representative agitator-paddle-tip / inlet-jet
                            momentum driving bulk recirculation, modeled as a
                            shear boundary condition on the tank's top face
                            (see tank.cljc docstring)."}
    :inlet #:inlet{:description
                    "Concentrated NaOCl stock feed + diluent (water) charge
                    nozzle, at the tank's top face — the same face the
                    agitation shear BC drives from. Domain data only; see
                    tank.cljc docstring."}
    :outlet #:outlet{:description
                      "Drain valve to the filling line, opened AFTER the
                      simulated agitation window (not a simultaneously-open
                      CFD boundary — see tank.cljc docstring)."}})
