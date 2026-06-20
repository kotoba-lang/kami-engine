# ADR-0038: kami-cosmos clean-room world-model / synthetic-data compat — roadmap

- Status: Proposed (roadmap, not yet scheduled)
- Date: 2026-06-20
- Scope: new crate `kami-cosmos` (reserved); consumers `kami-replicator`,
  `kami-sensor-sim`, `kami-nerf`, `kami-genesis`, `kami-shugyo`, `kami-groot`
- Related: ADR-0037 (kami-groot), ADR-0034 (Isaac-compat maturation),
  ADR-2605261800 §2(b) N1..N9 (clean-room nv-compat charter)

## Context

After Isaac (ADR-0033/0034) and GR00T (ADR-0037), the remaining uncovered piece
of the NVIDIA embodied-AI 3D stack is **Cosmos** — the World Foundation Model
family used for synthetic data generation and policy evaluation (predict future
frames / rollouts conditioned on current observation + action). Repo-wide there
is **zero** `cosmos` reference today.

The nearest existing capability is `kami-replicator` (an `omni.replicator.core`-
shaped synthetic-data / domain-randomization generator) and `kami-nerf` /
`kami-sensor-sim` (neural reconstruction + sensor synthesis), but none of these
is a *predictive world model*: given `(observation, action)`, roll the world
forward in pixel/latent space.

## Decision (roadmap only)

Reserve a clean-room crate `kami-cosmos` mirroring the *public* Cosmos API
shape — tokenizer / world-model predict surface and the
`predict(obs, action_plan) -> future_frames` call — under the same charter
invariant as kami-genesis and kami-groot (ADR-2605261800 §2(b) N1..N9 NEVER: no
NVIDIA model, weight, or binary linked or vendored; names/shapes mirrored only).

As with kami-groot, the surface is **backend-pluggable** and the shipped default
is **KAMI-native and non-generative**: the existing `kami-genesis` physics
rollout (deterministic forward dynamics) rendered through `kami-render` /
`kami-sensor-sim` *is* the default "world model" — a ground-truth simulator
standing in for a learned generative one. This makes the surface real and
testable with no NVIDIA assets, and gives `kami-groot` / `kami-shugyo` a rollout
oracle for model-based evaluation. A learned generative backend (diffusion /
latent video) is explicitly out-of-tree and out-of-charter-core.

This ADR is a **roadmap reservation**, sequenced *after* kami-groot lands, and is
not yet scheduled. It exists so the path is named (cf. the kami-app-amenominaka
"R1.0 path reservation" pattern) and the `kami-replicator` / `kami-nerf` overlap
is acknowledged rather than rediscovered.

## Consequences

- The NVIDIA 3D / embodied stack coverage (Isaac · Omniverse · GR00T · Cosmos)
  becomes complete at the *surface* level, all clean-room, all on the native
  engine.
- Synthetic-data generation gains a predictive-rollout seat distinct from
  `kami-replicator`'s randomization-based generation.
- No NVIDIA asset is required to build or test the default backend.

## Honestly still open / explicitly deferred

- The default backend predicts via physics + render, not generative video — it
  cannot hallucinate unseen scene content the way a learned WFM does; that is the
  point (charter-clean, deterministic) and the limit.
- Whether `kami-cosmos` should subsume or sit beside `kami-replicator` is
  unresolved and is the first design question when this ADR is scheduled.
- A learned generative backend (weights, tokenizer, sampler) is out-of-tree.
- No timeline; gated on kami-groot (ADR-0037) shipping first.
