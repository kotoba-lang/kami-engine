//! steam — Valve Steamworks platform-services seam (ADR-0048).
//!
//! Steam is **not** a renderer or a language; it is a desktop *distribution
//! channel* plus a set of platform services (achievements / stats / rich
//! presence / cloud / overlay / Steam Input). This module is the host seam for
//! the *services* half. The packaging half (depot layout, `steam_appid.txt`)
//! lives in `platform` (`Target::steam_distributable`) and the `bb kami` tooling.
//!
//! ## Determinism (load-bearing)
//!
//! The guest reaches Steam only through the `kami:engine/steam` **output-only**
//! interface (see `wit/kami-game/world.wit`): `unlock-achievement`, `set-stat`,
//! `set-rich-presence`. Nothing flows *back* into the i64 sim, so a game runs
//! bit-identically whether or not Steam is connected — the wasmtime↔wasmi
//! golden-frame parity (ADR-0037) is untouched. The host buffers each call in a
//! `SteamEvent` queue that the engine drains after a tick and hands to a
//! `SteamBackend`.
//!
//! ## Backends
//!
//! - [`StubSteam`] — the default. No-op + `log`, linked everywhere. CI, web,
//!   non-Steam desktop, and headless golden-frame tests all use it, so the
//!   `kami:engine/steam` imports always resolve and the seam is exercised
//!   without the SDK.
//! - Real Steamworks — a `steamworks-rs`-backed impl behind a `steam-sdk` cargo
//!   feature (off by default). It needs the Steamworks SDK redistributable, a
//!   Steam App ID, and a running Steam client, so — exactly like the ADR-0037
//!   console GPU backend — the **seam ships now; the SDK binding is gated** and
//!   wired only on an actual Steam desktop build. Not included in this scaffold.

/// A platform-service effect emitted by the guest during a tick. Output-only:
/// the engine drains these after `tick` and forwards them to a [`SteamBackend`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SteamEvent {
    /// Unlock an achievement by its Steamworks API name.
    UnlockAchievement(String),
    /// Set an integer stat to an absolute value.
    SetStat(String, i64),
    /// Set a rich-presence key/value pair (empty value clears the key).
    SetRichPresence(String, String),
}

/// The platform-services sink. One method per `SteamEvent`; an impl forwards to
/// Steamworks (or no-ops). Intentionally infallible at this seam — platform
/// telemetry must never break gameplay, so impls swallow/log their own errors.
pub trait SteamBackend {
    fn unlock_achievement(&mut self, _id: &str) {}
    fn set_stat(&mut self, _name: &str, _value: i64) {}
    fn set_rich_presence(&mut self, _key: &str, _value: &str) {}

    /// Drain a batch produced by one tick. Default fans out per event; an impl
    /// can override to coalesce (e.g. one `StoreStats` flush per frame).
    fn apply(&mut self, events: Vec<SteamEvent>) {
        for e in events {
            match e {
                SteamEvent::UnlockAchievement(id) => self.unlock_achievement(&id),
                SteamEvent::SetStat(name, v) => self.set_stat(&name, v),
                SteamEvent::SetRichPresence(k, v) => self.set_rich_presence(&k, &v),
            }
        }
    }
}

/// The default backend: log + no-op. Linked on every target so the
/// `kami:engine/steam` imports always resolve and the sim stays deterministic
/// off-Steam. The real `steamworks-rs` impl (feature `steam-sdk`) is not in this
/// scaffold — see the module docs.
#[derive(Debug, Default)]
pub struct StubSteam;

impl SteamBackend for StubSteam {
    fn unlock_achievement(&mut self, id: &str) {
        log::debug!("[steam stub] unlock-achievement {id:?}");
    }
    fn set_stat(&mut self, name: &str, value: i64) {
        log::debug!("[steam stub] set-stat {name:?} = {value}");
    }
    fn set_rich_presence(&mut self, key: &str, value: &str) {
        log::debug!("[steam stub] rich-presence {key:?} = {value:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recording backend to prove `apply` fans out in order.
    #[derive(Default)]
    struct Recorder(Vec<SteamEvent>);
    impl SteamBackend for Recorder {
        fn unlock_achievement(&mut self, id: &str) {
            self.0.push(SteamEvent::UnlockAchievement(id.into()));
        }
        fn set_stat(&mut self, name: &str, value: i64) {
            self.0.push(SteamEvent::SetStat(name.into(), value));
        }
        fn set_rich_presence(&mut self, key: &str, value: &str) {
            self.0.push(SteamEvent::SetRichPresence(key.into(), value.into()));
        }
    }

    #[test]
    fn apply_fans_out_in_order() {
        let batch = vec![
            SteamEvent::UnlockAchievement("FIRST_WIN".into()),
            SteamEvent::SetStat("kills".into(), 42),
            SteamEvent::SetRichPresence("status".into(), "in_combat".into()),
        ];
        let mut rec = Recorder::default();
        rec.apply(batch.clone());
        assert_eq!(rec.0, batch);
    }

    #[test]
    fn stub_is_infallible_noop() {
        // Exercises the default backend's whole surface — must never panic.
        let mut s = StubSteam;
        s.apply(vec![
            SteamEvent::UnlockAchievement("A".into()),
            SteamEvent::SetStat("s".into(), -1),
            SteamEvent::SetRichPresence("k".into(), "".into()),
        ]);
    }
}
