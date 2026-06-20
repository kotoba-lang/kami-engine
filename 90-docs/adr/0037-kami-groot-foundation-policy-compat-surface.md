# ADR-0037: kami-groot clean-room GR00T N1 foundation-policy compat surface

- Status: Proposed
- Date: 2026-06-20
- Scope: new crate `kami-groot`; consumers `kami-shugyo`, `kami-genesis`,
  `kami-sensor-sim`, `kami-character` / `kami-skeleton` / `kami-vrm`
- Related: ADR-0033 (kami-genesis `isaacsim.core.api` surface), ADR-0034 (Isaac-
  compat stack maturation), ADR-2605261800 §2(b) N1..N9 (clean-room nv-compat
  charter), ADR-2605261600 §G5 (quantitative quality gate)

## Context

The Isaac-compat stack (ADR-0033/0034) gives us, on a fully KAMI-native solver,
the four things a humanoid foundation policy needs around it: a multi-env RL
environment (`kami-shugyo`, `isaaclab.envs.ManagerBasedRLEnv`-shaped), articulated
dynamics + IK (`kami-genesis`, `isaacsim.core.api`-shaped), perception sensors
(`kami-sensor-sim`: Camera / Lidar / IMU / Contact), and humanoid body/skeleton
(`kami-character` / `kami-skeleton` / `kami-vrm`).

What is missing is the **policy seat itself**: there is no crate that mirrors the
NVIDIA Isaac GR00T (`gr00t` / `Isaac-GR00T` N1.x) public surface — model load,
the Vision-Language-Action (VLA) `obs → action` inference call, the embodiment /
modality config that maps a robot's joints + cameras onto the policy's I/O
heads, and the teleop / imitation episode format used to fine-tune. Today the
only repo-wide hit for `gr00t`/`groot` is the trademark list in
`CHARTER-RIDER.md`.

Without this seat, application code written against GR00T cannot run on the
engine, and the humanoid RL work in `kami-shugyo` has no foundation-policy
baseline to compare its from-scratch `LinearPolicy` against.

## Decision

Add a **clean-room** crate `kami-groot` that mirrors the *public, documented*
GR00T N1.x API names and tensor shapes only. The charter invariant
(ADR-2605261800 §2(b) N1..N9 NEVER — no NVIDIA library, header, binary, or model
weight linked, vendored, or referenced) is preserved exactly as in kami-genesis:
**method-name/shape mirroring only**, every action executed by the existing
KAMI-native stack.

The crate is **policy-pluggable**: the GR00T-shaped surface is a trait seat, and
the shipped default backend is a KAMI-native policy (the `kami-shugyo`
`LinearPolicy` / `random_search` learner generalized to the VLA I/O shape), not a
re-hosted foundation model. A real GR00T checkpoint is never required to build,
test, or run; loading one is an optional, user-supplied, out-of-tree backend.

### Surface to mirror (R1.x, all clean-room)

1. **Policy lifecycle** — `Gr00tPolicy::from_pretrained(path, embodiment_cfg)` /
   `reset()` / `get_action(obs) -> Action`. `from_pretrained` on a path with no
   weights instantiates the native default backend (honest: logs "native
   backend, no checkpoint loaded"), so the surface is exercisable with zero
   NVIDIA assets.

2. **Embodiment / modality config** — the `[state] / [action] / [video] /
   [language]` modality map (à la GR00T's `ModalityConfig` / data-config) that
   binds a robot's DOF order (from `kami-genesis` `dof_names`), camera streams
   (from `kami-sensor-sim` `Camera`), and a language-instruction slot onto the
   policy's typed I/O heads. Built from URDF + sensor rig, not hardcoded.

3. **Observation / Action tensors** — `Observation { state: [n_dof], video:
   Vec<Frame>, language: Option<String> }` and `Action { joint_targets:
   [horizon, n_dof] }` with the GR00T action-horizon (action-chunking) shape, so
   a policy emitting an H-step plan plugs straight into the `kami-shugyo`
   `VecRLEnv` step loop.

4. **Embodiment-head abstraction** — the GR00T "new-embodiment head" idea as a
   `EmbodimentHead` trait: a small adapter mapping the shared policy latent to a
   specific robot's action space, fine-tuned by the native learner. This is what
   makes the seat *foundation-policy-shaped* rather than a single hardcoded
   controller.

5. **Episode / teleop record format** — a clean-room `LeRobot`-shaped episode
   schema (`Episode { steps: Vec<{obs, action}> }`) for imitation data, readable
   by the learner. No NVIDIA / HuggingFace dataset binary is required; the format
   is mirrored, the codec is ours.

### Integration

- `kami-shugyo`: a `Gr00tPolicy` satisfies a `VecRLEnv`-driving policy trait, so
  the existing trainer evaluates a foundation-shaped policy on `VectorizedEeReachEnv`
  with the same DR / obs-noise rig — giving the from-scratch baseline a
  same-harness comparison.
- `kami-genesis`: obs `state` + `Action.joint_targets` ride the existing
  `ArticulationControllerView` (PD + feedforward), with action-chunking unrolled
  through the controller.
- `kami-sensor-sim`: `Camera` frames fill the `video` modality; an end-to-end
  `tests/groot_on_genesis.rs` rig (mirroring the `*_on_genesis.rs` sensor rigs)
  runs a moving humanoid through obs → policy → action → step.
- `kami-character` / `kami-skeleton` / `kami-vrm`: supply the humanoid
  embodiment (DOF map, retarget) the modality config binds against.

## Consequences

- GR00T-shaped application code (load policy, configure embodiment, run
  `get_action` in a control loop) runs unchanged on the native stack, with **no
  NVIDIA asset** required to build or test.
- `kami-shugyo` gains a foundation-policy baseline in the same evaluation harness
  as its from-scratch learner.
- The clean-room charter is preserved end-to-end; only public API names/shapes
  are mirrored, and the default backend is KAMI-native.
- Crate `description`, README, and this ADR become the synchronized source of
  truth, as in ADR-0034.

## Honestly still open

- The native default backend is a *small* learned policy, not a pretrained
  generalist — it will not match a real GR00T checkpoint's zero-shot breadth; it
  exists to make the seat real and testable, and to baseline.
- Loading an actual GR00T checkpoint (optional out-of-tree backend) needs a
  weight-format reader + tokenizer that we do not ship and that is explicitly
  outside the charter-clean core.
- VLA language grounding is a slot, not a trained language model; the `language`
  modality is plumbed but the native backend treats it as a conditioning id, not
  free-form comprehension.
- G5 validation (ADR-2605261600 §G5) against captured GR00T ground-truth action
  traces is deferred to an analytic/behavioral baseline until such traces exist.
- Action-horizon / chunking interplay with sub-stepped contact (the ADR-0034
  open bounce-energy item) is unverified for long horizons.
