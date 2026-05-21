//! Stage geometry — the physical layout of the venue.
//!
//! A [`Stage`] is a small bag of zones (where the performer stands,
//! where fans crowd, where lighting trusses hang) plus fixture mount
//! points. Geometry is venue-agnostic: pure `glam::Vec3` so any wgpu
//! renderer can place meshes by zone.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::lighting::LightingFixture;

/// Named regions on the show floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StageZone {
    /// Performer footprint (centre of the riser).
    Performer,
    /// Audience pit — densest fans, jumps the hardest.
    Pit,
    /// General floor — looser audience.
    Floor,
    /// Side wings (back-line, monitors, off-stage).
    Wings,
    /// VIP balcony (camera POV often perches here).
    Balcony,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneBox {
    pub centre: Vec3,
    /// Half-extents on each axis in metres.
    pub half_size: Vec3,
}

impl ZoneBox {
    pub fn contains(&self, p: Vec3) -> bool {
        let d = (p - self.centre).abs();
        d.x <= self.half_size.x && d.y <= self.half_size.y && d.z <= self.half_size.z
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureMount {
    pub fixture: LightingFixture,
    pub position: Vec3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    pub zones: Vec<(StageZone, ZoneBox)>,
    pub fixtures: Vec<FixtureMount>,
    /// LED wall (back-stage) anchor + half-size. Used by the VJ deck.
    pub led_wall: ZoneBox,
}

impl Stage {
    pub fn zone(&self, kind: StageZone) -> Option<&ZoneBox> {
        self.zones.iter().find(|(k, _)| *k == kind).map(|(_, b)| b)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StagePreset {
    /// Small club: 6m wide stage, 80-fan pit, low ceiling.
    Club,
    /// Mid-size hall: 12m, 600-fan pit, hanging trusses.
    Hall,
    /// Festival main stage: 24m, ~3000 fans, tower trusses + side LED wings.
    Festival,
}

impl StagePreset {
    /// Build a default [`Stage`]. Y is up. -Z is "into the audience".
    pub fn build(self) -> Stage {
        let (stage_w, stage_d, ceil, floor_w, floor_d, pit_d) = match self {
            StagePreset::Club => (6.0, 4.0, 4.0, 8.0, 10.0, 4.0),
            StagePreset::Hall => (12.0, 6.0, 8.0, 16.0, 20.0, 8.0),
            StagePreset::Festival => (24.0, 10.0, 14.0, 40.0, 60.0, 20.0),
        };
        let stage_y = match self {
            StagePreset::Club => 0.6,
            StagePreset::Hall => 1.2,
            StagePreset::Festival => 1.8,
        };
        let perf_centre = Vec3::new(0.0, stage_y, 0.0);
        let zones = vec![
            (
                StageZone::Performer,
                ZoneBox {
                    centre: perf_centre,
                    half_size: Vec3::new(stage_w * 0.5, 0.5, stage_d * 0.5),
                },
            ),
            (
                StageZone::Pit,
                ZoneBox {
                    centre: Vec3::new(0.0, 0.0, -(pit_d * 0.5 + stage_d * 0.5)),
                    half_size: Vec3::new(floor_w * 0.5, 1.0, pit_d * 0.5),
                },
            ),
            (
                StageZone::Floor,
                ZoneBox {
                    centre: Vec3::new(0.0, 0.0, -(floor_d * 0.5 + stage_d * 0.5)),
                    half_size: Vec3::new(floor_w * 0.5, 1.0, floor_d * 0.5),
                },
            ),
            (
                StageZone::Wings,
                ZoneBox {
                    centre: Vec3::new(stage_w * 0.5 + 1.5, stage_y, 0.0),
                    half_size: Vec3::new(2.0, 2.5, stage_d * 0.5),
                },
            ),
            (
                StageZone::Balcony,
                ZoneBox {
                    centre: Vec3::new(0.0, ceil * 0.5, -(floor_d + stage_d * 0.5)),
                    half_size: Vec3::new(floor_w * 0.4, 1.0, 4.0),
                },
            ),
        ];

        // Fixtures arrange along a back-line and front truss.
        let mut fixtures = Vec::new();
        let truss_y = ceil - 0.5;
        for x in [-stage_w * 0.4, 0.0, stage_w * 0.4] {
            fixtures.push(FixtureMount {
                fixture: LightingFixture::FrontPar,
                position: Vec3::new(x, truss_y, -0.5),
            });
            fixtures.push(FixtureMount {
                fixture: LightingFixture::BackPar,
                position: Vec3::new(x, truss_y, stage_d * 0.5 - 0.2),
            });
        }
        fixtures.push(FixtureMount {
            fixture: LightingFixture::Spot,
            position: Vec3::new(0.0, truss_y, 0.0),
        });
        fixtures.push(FixtureMount {
            fixture: LightingFixture::Strobe,
            position: Vec3::new(0.0, truss_y, -1.0),
        });
        for x in [-stage_w * 0.45, stage_w * 0.45] {
            fixtures.push(FixtureMount {
                fixture: LightingFixture::Blinder,
                position: Vec3::new(x, stage_y + 1.2, -0.2),
            });
            fixtures.push(FixtureMount {
                fixture: LightingFixture::Laser,
                position: Vec3::new(x, truss_y - 0.3, 0.0),
            });
        }

        let led_wall = ZoneBox {
            centre: Vec3::new(0.0, stage_y + 2.5, stage_d * 0.5 + 0.05),
            half_size: Vec3::new(stage_w * 0.45, 2.0, 0.05),
        };

        Stage {
            zones,
            fixtures,
            led_wall,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn club_has_all_zones() {
        let s = StagePreset::Club.build();
        for z in [
            StageZone::Performer,
            StageZone::Pit,
            StageZone::Floor,
            StageZone::Wings,
            StageZone::Balcony,
        ] {
            assert!(s.zone(z).is_some(), "missing zone {z:?}");
        }
    }

    #[test]
    fn pit_is_in_front_of_stage() {
        let s = StagePreset::Hall.build();
        let perf = s.zone(StageZone::Performer).unwrap();
        let pit = s.zone(StageZone::Pit).unwrap();
        assert!(pit.centre.z < perf.centre.z, "pit should be downstream (-Z)");
    }

    #[test]
    fn fixtures_present_on_each_truss() {
        let s = StagePreset::Festival.build();
        let kinds: Vec<_> = s.fixtures.iter().map(|f| f.fixture).collect();
        for k in [
            LightingFixture::FrontPar,
            LightingFixture::BackPar,
            LightingFixture::Spot,
            LightingFixture::Strobe,
            LightingFixture::Blinder,
            LightingFixture::Laser,
        ] {
            assert!(kinds.contains(&k), "missing {k:?}");
        }
    }

    #[test]
    fn zone_box_contains_works() {
        let b = ZoneBox {
            centre: Vec3::ZERO,
            half_size: Vec3::splat(1.0),
        };
        assert!(b.contains(Vec3::new(0.5, -0.5, 0.5)));
        assert!(!b.contains(Vec3::new(2.0, 0.0, 0.0)));
    }
}
