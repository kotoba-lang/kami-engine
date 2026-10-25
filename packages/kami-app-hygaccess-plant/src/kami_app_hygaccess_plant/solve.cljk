(ns kami-app-hygaccess-plant.solve
  "Registers the mixing-tank scenario as a `cae.solver/solve` method, dispatched
  on `[:solver :kind] = :hygaccess-mixing-tank` — the shared CAE solver-dispatch
  contract from `kami-engine-cae-solver` (`cae.solver/solve`, a multimethod
  dispatching on `(get-in case [:solver :kind])`). Callers invoke it uniformly:

    (require '[cae.solver :as solver] '[kami-app-hygaccess-plant.solve])
    (solver/solve {:solver {:kind :hygaccess-mixing-tank}
                    :tank kami-app-hygaccess-plant.process/default-tank
                    :process kami-app-hygaccess-plant.process/default-process})

  This mirrors how `cae.high-fidelity` / `cae.verification` register their own
  `:kind`s in that repo — SOLVER DISPATCH only; the actual physics lives in
  `mixing.cljc`."
  (:require [cae.solver :as solver]
            [kami-app-hygaccess-plant.mixing :as mixing]
            [kami-app-hygaccess-plant.process :as process]))

(defmethod solver/solve :hygaccess-mixing-tank
  [solver-case]
  (mixing/run-mixing-scenario
   {:tank         (or (:tank solver-case) (process/default-tank))
    :process      (or (:process solver-case) process/default-process)
    :flow-opts    (or (:flow-opts solver-case) {})
    :scalar-steps (or (:scalar-steps solver-case) 40)}))
