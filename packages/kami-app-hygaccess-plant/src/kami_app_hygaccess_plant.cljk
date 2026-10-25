(ns kami-app-hygaccess-plant
  "kami-app-hygaccess-plant — ONE illustrative product-visualization piece of
  `cloud-itonami/cloud-itonami-hygiene-access`: the sodium-hypochlorite
  dilution/mixing step for `int.hygaccess.water-purification-drops`, simulated
  with a real (if coarse, illustrative) finite-volume CFD solve rather than a
  hand-waved number. See README.md for full scope, what's tested vs.
  wired-but-unverified, and where every figure comes from.

  | Namespace | Contents |
  |---|---|
  | `kami-app-hygaccess-plant.tank` | Vessel/tank domain model (`:tank/...`) |
  | `kami-app-hygaccess-plant.process` | Batch/process parameters (`:process/...`), incl. `default-tank` / `default-process` |
  | `kami-app-hygaccess-plant.mixing` | The nagare-based CFD solve (flow + transient scalar transport + homogeneity CoV) |
  | `kami-app-hygaccess-plant.solve` | `cae.solver/solve :hygaccess-mixing-tank` registration |
  | `kami-app-hygaccess-plant.render` | render-IR construction (mirrors `kami.webgpu.ir`'s contract — see its docstring for why) |"
  (:require [kami-app-hygaccess-plant.tank]
            [kami-app-hygaccess-plant.process]
            [kami-app-hygaccess-plant.mixing]
            [kami-app-hygaccess-plant.solve]
            [kami-app-hygaccess-plant.render]))

(def scope
  "What this repo is and is not — see README.md 'Scope' for the full account."
  {:product-sku "int.hygaccess.water-purification-drops"
   :process :sodium-hypochlorite-dilution-mixing
   :fidelity :finite-volume-reference
   :status :screening-only
   :note "One representative illustrative process, not a general/validated/
          certified CFD tool. Other SKUs' mixing processes are structurally
          analogous (liquid-liquid dilution mixing in a stirred/jet-driven
          tank) but not separately simulated in this build."})
