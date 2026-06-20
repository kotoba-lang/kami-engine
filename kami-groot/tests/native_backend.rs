//! The GR00T-shaped surface runs end-to-end on the KAMI-native backend with
//! zero NVIDIA assets: configure an embodiment, build a native policy, and run
//! `get_action` over a VLA observation.

use kami_groot::{Episode, EmbodimentConfig, Gr00tPolicy, Observation};

fn test_arm() -> EmbodimentConfig {
    EmbodimentConfig::from_robot(
        "test_arm",
        vec!["j0".into(), "j1".into(), "j2".into()],
        vec![[-1.0, 1.0], [-2.0, 2.0], [-0.5, 0.5]],
        vec!["wrist_cam".into()],
        4, // action horizon
    )
}

#[test]
fn native_policy_emits_chunked_action_within_limits() {
    let emb = test_arm();
    let limits = emb.dof_limits.clone();
    let policy = Gr00tPolicy::native(emb);

    let obs = Observation {
        state: vec![0.0; 3],
        video: vec![],
        language: Some("reach the cube".into()),
    };
    let action = policy.get_action(&obs);

    assert_eq!(action.horizon, 4);
    assert_eq!(action.n_dof, 3);
    assert_eq!(action.joint_targets.len(), 12);

    // A zeros policy outputs normalized 0 → the midpoint of each joint range,
    // and the same step is tiled across the whole horizon.
    for h in 0..action.horizon {
        let step = action.step(h);
        for (d, &[lo, hi]) in limits.iter().enumerate() {
            let mid = 0.5 * (lo + hi);
            assert!(
                (step[d] - mid).abs() < 1e-6,
                "chunk {h} dof {d} = {} (expected midpoint {mid})",
                step[d]
            );
        }
    }

    // The native seat reports no loaded checkpoint — honest, charter-clean.
    assert!(policy.checkpoint().is_none());
    assert_eq!(action.first(), action.step(0));
}

#[test]
fn episode_record_roundtrips() {
    let mut ep = Episode::new("test_arm");
    ep.push(vec![0.0, 0.1, 0.2], vec![0.0, 0.0, 0.0], Some("reach".into()));
    ep.push(vec![0.1, 0.1, 0.2], vec![0.1, 0.0, -0.1], Some("reach".into()));
    assert_eq!(ep.len(), 2);

    let json = serde_json::to_string(&ep).unwrap();
    let back: Episode = serde_json::from_str(&json).unwrap();
    assert_eq!(ep, back);
}
