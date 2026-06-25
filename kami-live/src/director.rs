//! Director — EDN-declared reactions to show events.
//!
//! The choreography (poses, lighting, crowd) is data; so are the *reactions* a
//! scene triggers. Rather than a host hard-coding "confetti on the drop", the
//! scene author declares it in `:dance/triggers`, and the [`Director`] resolves,
//! per [`ShowEvent`], the action maps a host (native or web) should apply. Action
//! keys are free-form data — `:fx`, `:sound`, `:camera`, anything — so new
//! reactions need no engine code, only EDN.
//!
//! ```edn
//! :dance/triggers
//! [{:on :drop      :fx :confetti :sound :coin :camera :punch}
//!  {:on :breakdown :fx :dim      :sound :whoosh}
//!  {:on :callout   :tag "intro"  :camera :closeup}
//!  {:on :phrase    :vj-cut true}
//!  {:on :bar :every 8 :fx :pyro}]
//! ```

use std::collections::BTreeMap;

use kami_scene::{mget, EdnValue};

use crate::beat::BeatEvent;
use crate::setlist::CueKind;
use crate::show::ShowEvent;

/// What kind of show moment fires a trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerOn {
    /// Cue kinds (match `ShowEvent::Cue`).
    Drop,
    Breakdown,
    Callout,
    Custom,
    /// Beat-grid events (match `ShowEvent::Beat`).
    Beat,
    Bar,
    Phrase,
    /// A new track started (`ShowEvent::TrackChanged`).
    Track,
}

impl TriggerOn {
    fn by_name(name: &str) -> Option<TriggerOn> {
        Some(match name {
            "drop" => TriggerOn::Drop,
            "breakdown" => TriggerOn::Breakdown,
            "callout" => TriggerOn::Callout,
            "custom" => TriggerOn::Custom,
            "beat" => TriggerOn::Beat,
            "bar" => TriggerOn::Bar,
            "phrase" => TriggerOn::Phrase,
            "track" => TriggerOn::Track,
            _ => return None,
        })
    }
}

/// One authored reaction: a match condition + a free-form action map.
#[derive(Debug, Clone)]
pub struct Trigger {
    pub on: TriggerOn,
    /// Optional cue-tag filter (only fires for cues carrying this tag).
    pub tag: Option<String>,
    /// Optional periodicity for `bar` / `phrase` / `beat` (fires when the
    /// index is a multiple of `every`).
    pub every: Option<u32>,
    /// Action keys → values (`fx` → `:confetti`, `sound` → `:coin`, …). The
    /// host interprets these; the engine just carries the data.
    pub actions: BTreeMap<String, EdnValue>,
}

impl Trigger {
    /// Look up an action's value as an identifier (keyword/string name).
    pub fn action(&self, key: &str) -> Option<String> {
        self.actions.get(key).and_then(|v| {
            v.as_keyword()
                .map(|k| k.0.name.clone())
                .or_else(|| v.as_string().map(|s| s.to_string()))
        })
    }
}

/// The resolved set of authored reactions.
#[derive(Debug, Clone, Default)]
pub struct Director {
    pub triggers: Vec<Trigger>,
}

impl Director {
    /// Parse `:dance/triggers` from a scene root map. Missing → empty director.
    pub fn from_root(root: &BTreeMap<EdnValue, EdnValue>) -> Director {
        let triggers = mget(root, "dance/triggers")
            .and_then(|v| v.as_vector())
            .unwrap_or(&[])
            .iter()
            .filter_map(|t| t.as_map())
            .filter_map(parse_trigger)
            .collect();
        Director { triggers }
    }

    /// Resolve the action maps that fire for `event`, in author order.
    pub fn resolve(&self, event: &ShowEvent) -> Vec<&Trigger> {
        self.triggers
            .iter()
            .filter(|t| t.matches(event))
            .collect()
    }
}

impl Trigger {
    fn matches(&self, event: &ShowEvent) -> bool {
        match (self.on, event) {
            (TriggerOn::Drop, ShowEvent::Cue { cue, .. }) => {
                matches!(cue.kind, CueKind::Drop) && self.tag_ok(&cue.tag)
            }
            (TriggerOn::Breakdown, ShowEvent::Cue { cue, .. }) => {
                matches!(cue.kind, CueKind::Breakdown) && self.tag_ok(&cue.tag)
            }
            (TriggerOn::Callout, ShowEvent::Cue { cue, .. }) => {
                matches!(cue.kind, CueKind::Callout) && self.tag_ok(&cue.tag)
            }
            (TriggerOn::Custom, ShowEvent::Cue { cue, .. }) => {
                matches!(cue.kind, CueKind::Custom) && self.tag_ok(&cue.tag)
            }
            (TriggerOn::Beat, ShowEvent::Beat(BeatEvent::Beat { beat_index, .. })) => {
                self.every_ok(*beat_index)
            }
            (TriggerOn::Bar, ShowEvent::Beat(BeatEvent::Bar { bar_index, .. })) => {
                self.every_ok(*bar_index)
            }
            (TriggerOn::Phrase, ShowEvent::Beat(BeatEvent::Phrase { phrase_index, .. })) => {
                self.every_ok(*phrase_index)
            }
            (TriggerOn::Track, ShowEvent::TrackChanged { .. }) => true,
            _ => false,
        }
    }

    fn tag_ok(&self, tag: &str) -> bool {
        self.tag.as_deref().map_or(true, |t| t == tag)
    }

    fn every_ok(&self, index: u32) -> bool {
        match self.every {
            Some(n) if n > 0 => index % n == 0,
            _ => true,
        }
    }
}

fn parse_trigger(m: &BTreeMap<EdnValue, EdnValue>) -> Option<Trigger> {
    let on = mget(m, "on")
        .and_then(|v| v.as_keyword().map(|k| k.0.name.clone()))
        .and_then(|n| TriggerOn::by_name(&n))?;
    let tag = mget(m, "tag").and_then(|v| v.as_string()).map(|s| s.to_string());
    let every = mget(m, "every").and_then(|v| {
        v.as_integer()
            .or_else(|| v.as_float().map(|f| f as i64))
            .map(|i| i.max(0) as u32)
    });
    // every non-control key becomes an action.
    let mut actions = BTreeMap::new();
    for (k, v) in m {
        if let Some(kw) = k.as_keyword() {
            let name = kw.0.name.clone();
            if !matches!(name.as_str(), "on" | "tag" | "every") {
                actions.insert(name, v.clone());
            }
        }
    }
    Some(Trigger {
        on,
        tag,
        every,
        actions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::DanceScene;
    use crate::show::ShowEvent;

    const SCENE: &str = r#"
    {:dance/show {:bpm 140.0 :stage :festival}
     :dance/triggers
     [{:on :drop      :fx :confetti :sound :coin :camera :punch}
      {:on :callout   :tag "intro" :camera :closeup}
      {:on :phrase    :vj-cut true}
      {:on :bar :every 8 :fx :pyro}]
     :dance/setlist
     [{:title "A" :bpm 140.0 :bars 16 :dance :wota
       :cues [{:beat 0 :kind :callout :tag "intro"}
              {:beat 16 :kind :drop :tag "hook"}]}]}
    "#;

    #[test]
    fn parses_triggers_with_freeform_actions() {
        let sc = DanceScene::from_edn(SCENE).expect("scene");
        assert_eq!(sc.director.triggers.len(), 4);
        let drop = &sc.director.triggers[0];
        assert_eq!(drop.on, TriggerOn::Drop);
        assert_eq!(drop.action("fx").as_deref(), Some("confetti"));
        assert_eq!(drop.action("sound").as_deref(), Some("coin"));
        assert_eq!(drop.action("camera").as_deref(), Some("punch"));
    }

    #[test]
    fn resolves_drop_event_to_confetti() {
        let mut sc = DanceScene::from_edn(SCENE).expect("scene");
        sc.show.start();
        let mut fired_confetti = false;
        for _ in 0..600 {
            for ev in sc.show.tick(1.0 / 30.0) {
                for t in sc.director.resolve(&ev) {
                    if t.action("fx").as_deref() == Some("confetti") {
                        fired_confetti = true;
                    }
                }
            }
            if fired_confetti {
                break;
            }
        }
        assert!(fired_confetti, "drop cue resolved to the confetti trigger");
    }

    #[test]
    fn tag_filter_scopes_callout() {
        let sc = DanceScene::from_edn(SCENE).expect("scene");
        // a callout with the matching tag fires; a different tag does not.
        let mk = |tag: &str| ShowEvent::Cue {
            track_index: 0,
            cue: crate::setlist::CuePoint {
                at_beat: 0,
                kind: CueKind::Callout,
                tag: tag.to_string(),
            },
        };
        assert_eq!(sc.director.resolve(&mk("intro")).len(), 1);
        assert_eq!(sc.director.resolve(&mk("other")).len(), 0);
    }

    #[test]
    fn every_filter_gates_periodic_bars() {
        let sc = DanceScene::from_edn(SCENE).expect("scene");
        let bar = |i: u32| ShowEvent::Beat(BeatEvent::Bar {
            time: 0.0,
            bar_index: i,
        });
        // :every 8 → fires on bar 8/16, not 7.
        assert_eq!(sc.director.resolve(&bar(8)).len(), 1);
        assert_eq!(sc.director.resolve(&bar(7)).len(), 0);
    }
}
