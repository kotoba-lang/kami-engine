(ns kami-app-hygaccess-plant.mixing
  "The mixing-tank CFD scenario: wires `tank.cljc` / `process.cljc` domain data
  through `nagare`'s finite-volume incompressible-NS solver (momentum + pressure,
  PISO) to get a converged circulating flow field, then advects/diffuses an
  initial NaOCl concentration charge through that frozen flow field with a
  transient scalar-transport loop, and reduces the result to a mixing-
  homogeneity coefficient-of-variation (CoV) metric.

  ## Why a closed recirculating cavity, not an open inlet+outlet pair

  This is modeled as the sealed BATCH-AGITATION phase of a small-batch mixing
  tank: the tank is charged (concentrated NaOCl stock poured in near the top,
  the rest already filled with water), then agitated/circulated with everything
  else closed, THEN drained to the filling line afterward (the outlet, per
  `tank.cljc`, is not simultaneously open during this simulated window). That
  maps cleanly onto nagare's own Ghia-validated lid-driven-cavity topology — all
  four walls no-slip except the top face, which carries a tangential
  fixed-value velocity representing the agitator/inlet-jet momentum — so the
  momentum solve never leaves nagare's tested BC combination space. Because the
  top face's imposed velocity is purely tangential (its wall-normal component is
  zero, same as a literal lid), the face flux through every boundary is zero:
  the cavity is exactly closed, so the scalar (NaOCl mass) is exactly conserved
  by construction — no hand-derived open-boundary mass balance needed.

  ## Why a hand-assembled transient scalar loop

  `nagare.transport/solve` (as pinned in deps.edn) solves the STEADY scalar
  transport equation for a fixed flow field directly — there is no landed
  transient (ddt) scalar solve in nagare yet. To show homogeneity CoV
  decreasing tick-by-tick over simulated time (what this build's golden test
  asserts), `transient-scalar-step` below assembles its own backward-Euler ddt
  term on top of `nagare.fvm/convection-diffusion`'s steady spatial operator —
  literally the same `V_cell/dt` on-diagonal / `V_cell/dt * value_old` in-source
  formula `nagare.fvm/momentum` already uses internally for the vector
  (velocity) ddt term, just applied here to a scalar field and composed at the
  call site from nagare's already-exposed `fvm`/`linsolve` primitives rather
  than reaching into nagare's internals. See docs/adr/0001-architecture.md
  Decision 3."
  (:require [nagare.mesh :as mesh]
            [nagare.field :as field]
            [nagare.fvm :as fvm]
            [nagare.linsolve :as ls]
            [nagare.solver :as solver]
            [nagare.diagnostics :as diagnostics]
            [kami-app-hygaccess-plant.tank :as tank]))

;; ---------------------------------------------------------------------------
;; Mesh + flow (incompressible NS, PISO)
;; ---------------------------------------------------------------------------

(defn build-mesh
  "The tank's 2-D cross-section as a nagare unstructured polyMesh."
  [tank-map]
  (mesh/block-mesh (tank/mesh-params tank-map)))

(defn- velocity-field
  "U=(0,0) everywhere, no-slip on three walls, tangential fixed-value
  `drive-v` on the top face (the agitator/inlet-jet drive — see ns docstring)."
  [m drive-v]
  (field/vol-vector m [0.0 0.0]
                    {:top    {:type :fixed-value :value [(double drive-v) 0.0]}
                     :left   {:type :no-slip}
                     :right  {:type :no-slip}
                     :bottom {:type :no-slip}}))

(defn- pressure-field
  "p=0 everywhere, fixedFluxPressure (zeroGradient-for-value) on all four
  walls — identical to nagare.demo's Ghia-validated cavity setup."
  [m]
  (field/vol-scalar m 0.0
                    {:top    {:type :fixed-flux-pressure}
                     :left   {:type :fixed-flux-pressure}
                     :right  {:type :fixed-flux-pressure}
                     :bottom {:type :fixed-flux-pressure}}))

(defn converge-flow
  "Run PISO (`nagare.solver/advance`) to a steady recirculating flow field for
  the tank's agitation-driven closed cavity.

  `nu-eff-m2-s` is an EFFECTIVE (turbulent, not molecular) kinematic viscosity:
  a real agitated tank's momentum Reynolds number (using water's molecular
  viscosity ~1e-6 m^2/s) is ~10^5-10^6 — fully turbulent, and nagare does not
  yet have a landed turbulence closure (its own roadmap marks RANS k-epsilon/
  k-omega-SST as pending S3). Running nagare's LAMINAR upwind solver at a
  literal molecular Reynolds number would silently misrepresent unresolved
  turbulence as a resolved laminar flow. Using a representative EFFECTIVE
  (eddy) viscosity — the standard reduced-order shortcut for exactly this
  situation — instead keeps the solve at a Reynolds number nagare's upwind
  scheme is actually validated at (its own Ghia cavity demo runs Re=100; this
  default of 5.0e-3 m^2/s against a 0.5 m/s / 0.6 m tank gives Re=60).

  Returns `{:mesh :U :p :phi :nu-eff-m2-s :dt-s :steps-run :converged}`."
  [tank-map {:keys [nu-eff-m2-s steps steady-tol]
             :or {nu-eff-m2-s 5.0e-3 steps 1500 steady-tol 1e-5}}]
  (let [m       (build-mesh tank-map)
        drive-v (tank/agitation-drive-velocity-m-s tank-map)
        dx      (:dx m)
        dt      (* 0.4 (/ dx (max drive-v 1.0e-6)))     ;; CFL ~0.4, mirrors nagare.demo
        U       (velocity-field m drive-v)
        p       (pressure-field m)
        params  {:nu nu-eff-m2-s :dt dt :n-correctors 2 :ref-cell 0
                  :p-tol 1e-7 :u-tol 1e-6 :max-iter 400}
        st      (solver/advance m params {:U U :p p :phi nil}
                                {:steps steps :steady-tol steady-tol})]
    {:mesh m :U (:U st) :p (:p st) :phi (:phi st)
     :nu-eff-m2-s nu-eff-m2-s :dt-s dt
     :steps-run (:steps-run st) :converged (boolean (:converged st))
     :max-courant (diagnostics/max-courant m (:phi st) dt)}))

;; ---------------------------------------------------------------------------
;; Scalar (concentration) transport — transient, hand-assembled ddt
;; ---------------------------------------------------------------------------

(defn- charge-region-cell-count
  "How many of the mesh's `n` cells the concentrated-stock charge occupies, so
  that the volume-average concentration comes out at the target dilution
  (`n / dilution-ratio` cells at stock concentration, the rest at 0, averages
  to `stock-concentration / dilution-ratio == target-concentration`)."
  [n dilution-ratio]
  (long (Math/round (/ (double n) (double dilution-ratio)))))

(defn initial-concentration-field
  "The just-charged, not-yet-mixed concentration field: the top
  `charge-region-cell-count` cells (nearest the inlet nozzle / agitator-drive
  face) at the stock-feed concentration, everything else (the pre-existing
  water fill) at 0. `:zero-gradient` on every wall — no diffusive flux through
  the tank shell (physically correct: NaOCl doesn't diffuse through steel/
  plastic), and (per the ns docstring) no advective flux through any wall
  either, since the closed-cavity flow field's boundary-normal velocity is
  zero everywhere."
  [m {:process/keys [stock-feed-concentration-pct]} dilution-ratio]
  (let [nx (:nx m) ny (:ny m) n (:n-cells m)
        charge-n (charge-region-cell-count n dilution-ratio)
        charge-rows (long (Math/ceil (/ (double charge-n) nx)))
        top-row-start (- ny charge-rows)
        charge? (fn [cell-id] (>= (quot cell-id nx) top-row-start))
        init (fn [cell-id] (if (charge? cell-id) (double stock-feed-concentration-pct) 0.0))]
    (field/vol-scalar m init
                      {:top    {:type :zero-gradient}
                       :left   {:type :zero-gradient}
                       :right  {:type :zero-gradient}
                       :bottom {:type :zero-gradient}})))

(defn- ddt-source
  "Add a backward-Euler ddt term to a steady convection-diffusion matrix `A` —
  see ns docstring for why this mirrors `nagare.fvm/momentum`'s inlined ddt."
  [m dt c-old A]
  (let [n (int (:n-cells m))
        ^doubles vol (:volumes m)
        ^doubles diag (aclone ^doubles (:diag A))
        ^doubles src  (aclone ^doubles (:source A))]
    (dotimes [c n]
      (let [r (/ (aget vol c) (double dt))]
        (aset diag c (+ (aget diag c) r))
        (aset src c (+ (aget src c) (* r (double (nth c-old c)))))))
    (assoc A :diag diag :source src)))

(defn transient-scalar-step
  "One backward-Euler transient scalar-transport tick of concentration field
  `c-field` on the frozen face-flux field `phi`, diffusivity `gamma`."
  [m gamma phi c-field dt]
  (let [A  (fvm/convection-diffusion m gamma phi c-field)
        A' (ddt-source m dt (:values c-field) A)
        x  (:x (ls/bicgstab A' (:source A') {:tol 1.0e-11 :max-iter 4000}))]
    (field/with-values c-field (vec x))))

;; ---------------------------------------------------------------------------
;; Homogeneity metric
;; ---------------------------------------------------------------------------

(defn coefficient-of-variation-pct
  "std-dev / mean, as a percentage, of a field's cell values — the mixing-
  homogeneity metric (`:mixing-homogeneity-cov-pct` in the result map).
  0% = perfectly uniform (fully mixed); larger = less homogeneous."
  [values]
  (let [n (count values)
        mean (/ (reduce + values) n)
        variance (/ (reduce + (map (fn [v] (let [d (- (double v) mean)] (* d d))) values)) n)
        sd (Math/sqrt variance)]
    (if (zero? mean) 0.0 (* 100.0 (/ sd (Math/abs (double mean)))))))

;; ---------------------------------------------------------------------------
;; Orchestration
;; ---------------------------------------------------------------------------

;; ---------------------------------------------------------------------------
;; Stepwise API — a genuine closed-loop control surface alongside the
;; monolithic `run-mixing-scenario` below (that entrypoint is UNCHANGED and
;; still the one CI's render-verification job and mixing_test.cljc's original
;; golden assertions depend on).
;;
;; `run-mixing-scenario` converges the agitation-driven flow field EXACTLY
;; ONCE (`converge-flow`), then holds the resulting face-flux field `phi`
;; frozen across all `scalar-steps` transient scalar ticks — a quasi-steady-
;; flow assumption (the momentum field re-equilibrates fast relative to the
;; scalar-mixing timescale) that was already implicit in that function, just
;; never exercised by a changing boundary condition. The stepwise API below
;; makes that assumption explicit and genuinely closes it: `step` re-solves
;; PISO — warm-started from the previous flow state, never from rest — ONLY
;; when the commanded agitator RPM actually changes the drive-velocity
;; boundary condition from the previous tick's; a held setpoint reuses the
;; already-converged flow untouched. This is what makes higher RPM show up as
;; a REAL, recomputed higher face-flux magnitude (stronger convective
;; stirring => a genuinely different, faster-homogenizing CoV trajectory —
;; see mixing_test.cljc `higher-rpm-mixes-faster-than-lower-rpm`), and what
;; makes a CONSTANT-RPM stepwise run numerically reproduce
;; `run-mixing-scenario`'s own single-converge-then-hold-phi path bit-for-bit
;; (see mixing_test.cljc `stepwise-with-constant-rpm-matches-monolithic-scenario`).
;; ---------------------------------------------------------------------------

(defn- reconverge-flow
  "Re-solve PISO (`nagare.solver/advance`) to the steady flow field for a NEW
  `new-drive-v-m-s` top-face BC, WARM-STARTED from `prev` (`{:U :p :phi}`)
  rather than from rest. Warm-starting is not just an optimization here — it
  is the physically correct behaviour: a real agitated fluid that already has
  momentum doesn't reset to stationary just because the setpoint moved, it
  relaxes from wherever it already is. It is also far cheaper than a from-
  rest `converge-flow` call: only the DELTA between the old and new BC needs
  to relax, so a setpoint change typically re-converges in a handful of PISO
  steps rather than the ~300 a from-rest solve needs. `flow-opts` (`:steps`/
  `:steady-tol`) means the same as `converge-flow`'s."
  [mesh {:keys [U p phi]} new-drive-v-m-s nu-eff-m2-s
   {:keys [steps steady-tol] :or {steps 1500 steady-tol 1e-5}}]
  (let [dx (:dx mesh)
        dt (* 0.4 (/ dx (max (double new-drive-v-m-s) 1.0e-6)))
        U' (assoc-in U [:boundary :top :value] [(double new-drive-v-m-s) 0.0])
        params {:nu nu-eff-m2-s :dt dt :n-correctors 2 :ref-cell 0
                 :p-tol 1e-7 :u-tol 1e-6 :max-iter 400}
        st (solver/advance mesh params {:U U' :p p :phi phi}
                            {:steps steps :steady-tol steady-tol})]
    {:U (:U st) :p (:p st) :phi (:phi st)
     :flow-diagnostics {:dt-s dt :steps-run (:steps-run st)
                         :converged (boolean (:converged st))
                         :max-courant (diagnostics/max-courant mesh (:phi st) dt)}}))

(defn init-state
  "Stepwise-API entry point. Same physical scenario as `run-mixing-scenario`
  (`tank`/`process`/`flow-opts`/`scalar-steps` mean exactly the same thing,
  including the SAME defaults, and this fn performs the SAME `converge-flow`
  + `initial-concentration-field` calls that fn's own `let` block does), but
  returns the INITIAL state for driving the scenario forward tick-by-tick via
  `step` instead of monolithically.

  Returned state: `{:mesh :tank :process :U :p :phi :nu-eff-m2-s
  :drive-velocity-m-s :flow-opts :flow-diagnostics :c :gamma :scalar-dt-s
  :scalar-steps :tick :mixing-homogeneity-cov-pct-history}` — `:tick` starts
  at 0, `:mixing-homogeneity-cov-pct-history` starts as the just-charged,
  not-yet-mixed CoV (matching `run-mixing-scenario`'s own history's first
  entry)."
  [{:keys [tank process flow-opts scalar-steps]
    :or {flow-opts {} scalar-steps 40}}]
  (let [{:keys [mesh U p phi nu-eff-m2-s dt-s steps-run converged max-courant]}
        (converge-flow tank flow-opts)
        dilution-ratio (/ (double (:process/stock-feed-concentration-pct process))
                          (double (:process/target-concentration-pct process)))
        c0 (initial-concentration-field mesh process dilution-ratio)
        gamma (:process/turbulent-diffusivity-m2-s process)
        scalar-dt (/ (double (:process/mixing-duration-s process)) (double scalar-steps))
        drive-v (tank/agitation-drive-velocity-m-s tank)]
    {:mesh mesh :tank tank :process process
     :U U :p p :phi phi
     :nu-eff-m2-s nu-eff-m2-s :drive-velocity-m-s drive-v
     :flow-opts flow-opts
     :flow-diagnostics {:dt-s dt-s :steps-run steps-run :converged converged
                         :max-courant max-courant}
     :c c0 :gamma gamma :scalar-dt-s scalar-dt :scalar-steps scalar-steps
     :tick 0
     :mixing-homogeneity-cov-pct-history [(coefficient-of-variation-pct (:values c0))]}))

(defn step
  "Advance a stepwise-API `state` (from `init-state`, or a previous `step`)
  by ONE control tick, given a `control-command` map
  `{:agitator-rpm-setpoint <RPM number>}`.

  Converts the RPM setpoint to a drive velocity
  (`tank/agitator-rpm->drive-velocity-m-s`) and — ONLY IF that velocity
  differs (beyond floating-point round-trip noise, 1e-9 m/s) from the state's
  current converged drive velocity — re-solves PISO to the new steady flow
  field (`reconverge-flow`, warm-started, never from rest) before advecting
  the concentration field one backward-Euler tick (`transient-scalar-step`)
  through the (possibly-updated) flow's face-flux field `phi`. A held
  setpoint reuses the already-converged flow untouched — see ns docstring for
  why that is both physically correct and what makes a constant-RPM run
  reproduce `run-mixing-scenario`.

  Returns the updated state (same shape as `init-state`'s return, `:tick`
  incremented, `:mixing-homogeneity-cov-pct-history` grown by one entry, plus
  `:control-command` recording what was actually applied this tick)."
  [{:keys [mesh tank U p phi drive-velocity-m-s nu-eff-m2-s flow-opts
           c gamma scalar-dt-s tick mixing-homogeneity-cov-pct-history]
    :as state}
   {:keys [agitator-rpm-setpoint] :as control-command}]
  (let [new-drive-v (tank/agitator-rpm->drive-velocity-m-s tank agitator-rpm-setpoint)
        bc-changed? (> (Math/abs (- new-drive-v (double drive-velocity-m-s))) 1.0e-9)
        {:keys [U p phi flow-diagnostics]}
        (if bc-changed?
          (reconverge-flow mesh {:U U :p p :phi phi} new-drive-v nu-eff-m2-s flow-opts)
          {:U U :p p :phi phi :flow-diagnostics (:flow-diagnostics state)})
        c' (transient-scalar-step mesh gamma phi c scalar-dt-s)
        cov' (coefficient-of-variation-pct (:values c'))]
    (assoc state
           :U U :p p :phi phi
           :drive-velocity-m-s new-drive-v
           :flow-diagnostics flow-diagnostics
           :c c'
           :tick (inc tick)
           :control-command control-command
           :mixing-homogeneity-cov-pct-history (conj mixing-homogeneity-cov-pct-history cov')
           :mixing-homogeneity-cov-pct cov')))

(defn sensor-reading
  "Extract a plausible sensor-style summary from a stepwise `state`: the
  current homogeneity CoV, and a FLOW-MAGNITUDE-DERIVED 'temperature proxy'.

  Be honest about what this is: this model has NO energy equation anywhere
  (no thermal field, no heat source, no thermal BC — `mixing.cljc` solves
  momentum + scalar-concentration transport only). `:temperature-proxy-c` is
  NOT a simulated temperature reading; it is a monotonic stand-in derived
  from the flow field's mean velocity magnitude (representative ambient +
  viscous/agitation-heating band, documented constants below, not measured or
  solved for), spelled `-proxy-` rather than `:temperature-c` specifically so
  a downstream consumer (e.g. `cloud-itonami/cloud-itonami-hygiene-access`'s
  control-loop reader) can't mistake it for a real thermal simulation result."
  [{:keys [mesh U mixing-homogeneity-cov-pct-history tick]}]
  (let [values (:values U)
        n (count values)
        mean-speed (/ (reduce + (map (fn [[vx vy]] (Math/sqrt (+ (* vx vx) (* vy vy)))) values))
                      (double n))
        ;; Representative mapping only, NOT a solved energy equation: ambient
        ;; water temperature (20 C) plus up to a few degrees of viscous/
        ;; agitation heating that scales with flow speed. The 10.0 C-per-(m/s)
        ;; coefficient is this build's own illustrative choice, documented
        ;; here (not measured, not derived from a thermal simulation).
        temperature-proxy-c (+ 20.0 (* 10.0 mean-speed))]
    {:tick tick
     :mixing-homogeneity-cov-pct (last mixing-homogeneity-cov-pct-history)
     :mean-flow-speed-m-s mean-speed
     :temperature-proxy-c temperature-proxy-c
     :mesh-n-cells (:n-cells mesh)}))

(defn run-mixing-scenario
  "Run the full scenario: converge the agitation-driven flow, charge the tank
  with a concentration blob sized to the process's dilution ratio, step scalar
  transport forward `scalar-steps` ticks over `:process/mixing-duration-s`
  simulated seconds, and reduce to the homogeneity-CoV result record.

  `tank`/`process` are REQUIRED (a `:tank/...` map and a `:process/...` map —
  see `process/default-tank` / `process/default-process` for ready-made ones).
  This fn does not default them itself and has no dependency on `process.cljc`
  at all, keeping the dependency direction domain-data -> solve one-way;
  `solve.cljc` is where the `process/default-*` fallback lives."
  [{:keys [tank process flow-opts scalar-steps]
    :or {flow-opts {} scalar-steps 40}}]
  (let [{:keys [mesh phi nu-eff-m2-s dt-s steps-run converged max-courant]}
        (converge-flow tank flow-opts)
        dilution-ratio (/ (double (:process/stock-feed-concentration-pct process))
                          (double (:process/target-concentration-pct process)))
        c0 (initial-concentration-field mesh process dilution-ratio)
        gamma (:process/turbulent-diffusivity-m2-s process)
        scalar-dt (/ (double (:process/mixing-duration-s process)) (double scalar-steps))
        run (loop [k 0 c c0 hist [(coefficient-of-variation-pct (:values c0))]]
              (if (>= k scalar-steps)
                {:field c :history hist}
                (let [c' (transient-scalar-step mesh gamma phi c scalar-dt)]
                  (recur (inc k) c' (conj hist (coefficient-of-variation-pct (:values c')))))))
        final-values (:values (:field run))
        final-mean (/ (reduce + final-values) (count final-values))
        final-mass (reduce + (map * final-values (seq (:volumes mesh))))
        initial-mass (reduce + (map * (:values c0) (seq (:volumes mesh))))]
    {:solver :hygaccess-mixing-tank
     :mesh {:nx (:nx mesh) :ny (:ny mesh) :lx (:lx mesh) :ly (:ly mesh)
            :dx (:dx mesh) :dy (:dy mesh) :n-cells (:n-cells mesh)}
     :flow {:nu-eff-m2-s nu-eff-m2-s :dt-s dt-s :steps-run steps-run
            :converged converged :max-courant max-courant}
     :scalar {:diffusivity-m2-s gamma :dt-s scalar-dt :steps scalar-steps
              :dilution-ratio dilution-ratio}
     :process process
     :concentration-field-pct final-values
     :concentration-mean-pct final-mean
     :concentration-mass-conservation-defect (Math/abs (- final-mass initial-mass))
     :mixing-homogeneity-cov-pct-history (:history run)
     :mixing-homogeneity-cov-pct (last (:history run))
     :fidelity :finite-volume-reference
     :status :screening-only}))
