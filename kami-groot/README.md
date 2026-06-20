# kami-groot

Clean-room **NVIDIA Isaac GR00T N1.x** foundation-policy compat surface — the
"policy seat" of the KAMI embodied-AI stack, completing
Isaac (`kami-genesis`) + Isaac Lab (`kami-shugyo`) + sensors (`kami-sensor-sim`).

**Status**: R1.0 — native seat. See
`90-docs/adr/0037-kami-groot-foundation-policy-compat-surface.md`.

## Charter invariant

Mirrors the *public, documented* GR00T Vision-Language-Action API **by name and
shape only**. No NVIDIA library, header, binary, or model weight is linked,
vendored, or referenced (ADR-2605261800 §2(b) N1..N9 NEVER). The seat is
`EmbodimentHead`-pluggable and the **shipped default backend is KAMI-native**
(the `kami-shugyo` gradient-free policy generalized to the VLA I/O shape), so the
whole crate **builds, tests, and runs with zero NVIDIA assets**.

## Surface (mirrored)

| GR00T N1.x | kami-groot | Notes |
|---|---|---|
| `Gr00tPolicy.from_pretrained(path, embodiment)` | `Gr00tPolicy::from_pretrained` | no weights at `path` → native backend, logged honestly |
| `policy.reset()` / `policy.get_action(obs)` | `reset` / `get_action` | native backend is stateless |
| `ModalityConfig` / embodiment data-config | `EmbodimentConfig` / `ModalityConfig` | built from URDF `dof_names`/limits + camera rig |
| obs `{state, video, language}` | `Observation` | proprioception + frames + instruction slot |
| action chunk `[horizon, n_dof]` | `Action` | action-chunking shape; native plan is myopic-tiled |
| new-embodiment head | `EmbodimentHead` trait / `NativeHead` | swap native ↔ out-of-tree checkpoint |
| LeRobot episode dataset | `Episode` / `EpisodeStep` | clean-room schema, our codec |

## Example

```rust
use kami_groot::{EmbodimentConfig, Gr00tPolicy, Observation};

let emb = EmbodimentConfig::from_robot(
    "panda",
    vec!["j1".into(), /* … */ "j7".into()],
    vec![[-2.9, 2.9]; 7],          // per-DOF limits (from kami-genesis get_dof_limits)
    vec!["wrist_cam".into()],
    16,                            // action-chunk horizon
);
let policy = Gr00tPolicy::native(emb);         // zero NVIDIA assets

let action = policy.get_action(&Observation {
    state: vec![0.0; 7],
    video: vec![],
    language: Some("pick up the cube".into()),
});
let next = action.first();                     // [n_dof] joint targets to execute
```

## Build & test

```bash
cargo test -p kami-groot
```

## Honestly still open

- The native default backend is a *small* learned policy, not a pretrained
  generalist — it exists to make the seat real, testable, and baseline-able.
- Loading an actual GR00T checkpoint is an optional **out-of-tree** backend
  (weight reader + tokenizer) explicitly outside the charter-clean core.
- The `language` modality is a conditioning slot, not a trained LM.
