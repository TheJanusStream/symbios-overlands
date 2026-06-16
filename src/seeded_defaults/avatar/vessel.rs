//! Steampunk / scifi catamaran design knobs for the default
//! hover-boat avatar.
//!
//! Sister to [`super::body`]: where `AvatarBody` carries
//! humanoid-relevant proportions kept on a tight band, `VesselDesign`
//! carries vessel-specific knobs (hull radius, mast height, smokestack
//! count) with deliberately wider continuous ranges so two
//! same-archetype avatars still feel visibly distinct.
//!
//! Two enums anchor the design space: [`VesselArchetype`] picks the
//! ornamental kit (Steam / Solar / Hybrid) and [`BowStyle`] picks the
//! prow ornament. The continuous knobs are sampled per-archetype where
//! it matters (e.g. only Steam/Hybrid vessels actually use
//! `smokestack_count`); other consumers read whatever value is in the
//! struct so future spawners stay free of "is this field meaningful"
//! branching.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::hash::fnv1a_64;
use crate::seeded_defaults::scene::{pick, range_f32};

const AVATAR_VESSEL_SALT: u64 = 0xCA7A_C0DE_CA7A_C0DE;

/// Vessel ornamental archetype. Drives which decorative pieces show
/// up (smokestacks vs solar panels vs both) without changing the core
/// hull / deck / mast skeleton.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VesselArchetype {
    /// Brass-pipe steampunk: smokestacks rising from the stern.
    Steam,
    /// Clean scifi: a tilted solar panel above the deck, slim antenna
    /// crowning the mast.
    Solar,
    /// One of each — a hybrid steampunk/scifi look.
    Hybrid,
}

impl VesselArchetype {
    pub const ALL: [Self; 3] = [Self::Steam, Self::Solar, Self::Hybrid];

    /// Whether this archetype mounts one or more smokestacks at the
    /// stern (Steam, Hybrid).
    pub fn has_smokestacks(self) -> bool {
        matches!(self, Self::Steam | Self::Hybrid)
    }

    /// Whether this archetype carries a tilted solar panel above the
    /// deck (Solar, Hybrid).
    pub fn has_solar_panel(self) -> bool {
        matches!(self, Self::Solar | Self::Hybrid)
    }

    /// Whether this archetype crowns its mast with a slim antenna
    /// (Solar, Hybrid). Steam vessels keep the bare finial.
    pub fn has_antenna(self) -> bool {
        matches!(self, Self::Solar | Self::Hybrid)
    }
}

/// Hull skeleton family. Unlike [`VesselArchetype`] (which only swaps
/// ornaments), the hull form changes the actual silhouette: how many
/// hulls there are, how wide the deck sits, and how the mass is
/// distributed. Two boats with different hull forms read as different
/// vessels from across a room, not as the same boat re-trimmed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HullForm {
    /// Single fat centre hull under a narrow deck — sleek launch.
    Monohull,
    /// Two symmetric hulls — the classic wide catamaran.
    Catamaran,
    /// Fat centre hull plus two short outriggers — busiest waterline.
    Trimaran,
    /// No capsule hulls at all: one wide, shallow, tapered slab under
    /// an oversized deck. Reads as an industrial flat-top.
    Barge,
}

impl HullForm {
    pub const ALL: [Self; 4] = [Self::Monohull, Self::Catamaran, Self::Trimaran, Self::Barge];
}

/// Prow ornament family. `None` is a valid pick — some avatars skip
/// the bow piece entirely and let the asymmetric stern (smokestacks /
/// solar panel + flag) carry the directional cue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BowStyle {
    /// Vertical spike — a tall thin cone standing on the deck prow.
    Spike,
    /// Small forward jewel sphere — quietest of the four.
    Sphere,
    /// Long horizontal cone pointing forward — the "ramming prow".
    Beak,
    /// Skip the bow ornament. Vessel reads as flat-fronted.
    None,
}

impl BowStyle {
    pub const ALL: [Self; 4] = [Self::Spike, Self::Sphere, Self::Beak, Self::None];
}

/// All seeded vessel knobs. Every dimension scale is a multiplier
/// against the nominal value the spawner uses (`1.0` = nominal); the
/// spawner reads them as `base × scale`.
#[derive(Clone, Copy, Debug)]
pub struct VesselDesign {
    pub archetype: VesselArchetype,
    pub hull_form: HullForm,
    pub bow_style: BowStyle,

    /// Catamaran hull capsule radius scale.
    pub hull_radius_scale: f32,
    /// Catamaran hull capsule length scale (fore-aft extent).
    pub hull_length_scale: f32,
    /// Lateral spread between the two hulls (multiplier on nominal
    /// `±0.85` X). Wider values give a more stable, plump catamaran;
    /// narrower values give a sleeker form.
    pub hull_spread_scale: f32,
    /// How far below the deck the hulls sit (Y offset multiplier).
    pub hull_drop_scale: f32,

    /// Mast height scale (vertical extent of the central cylinder).
    pub mast_height_scale: f32,
    /// Mast cross-section scale.
    pub mast_radius_scale: f32,

    /// Bow ornament dimension scale. Unused when `bow_style ==
    /// BowStyle::None`.
    pub bow_scale: f32,

    /// Number of stern smokestacks. `0` when the archetype skips
    /// stacks (Solar). Otherwise `1..=3`.
    pub smokestack_count: u32,
    /// Smokestack height scale.
    pub smokestack_height_scale: f32,

    /// Solar panel tilt angle (radians, around X). Unused when the
    /// archetype skips the panel (Steam).
    pub solar_panel_tilt_rad: f32,

    /// Upward rake of the hull prow, in metres of vertex-torture bend
    /// applied to each hull capsule's bow end. `0.0` is a straight
    /// log; higher values read as a sled / gondola sweep.
    pub prow_rake: f32,
    /// Mast taper fraction (`1 - taper` top width). Tapered masts read
    /// as ship spars instead of scaffold pipes.
    pub mast_taper: f32,
    /// Smokestack flare — *negative* taper so the stack opens outward
    /// at the crown like a Victorian funnel.
    pub stack_flare: f32,
}

impl VesselDesign {
    pub fn for_did(did: &str) -> Self {
        Self::for_seed(fnv1a_64(did))
    }

    /// Derive from a pre-computed seed — the manual re-roll path.
    /// `for_did(did)` is exactly `for_seed(fnv1a_64(did))`.
    pub fn for_seed(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_VESSEL_SALT);

        let archetype = pick(&VesselArchetype::ALL, &mut rng);
        let hull_form = pick(&HullForm::ALL, &mut rng);
        let bow_style = pick(&BowStyle::ALL, &mut rng);

        // Wider continuous ranges than `AvatarBody` carries — vessels
        // are decorative and can drift further from a "nominal hull"
        // without reading as broken.
        let hull_radius_scale = range_f32(&mut rng, 0.85, 1.55);
        let hull_length_scale = range_f32(&mut rng, 0.95, 1.35);
        let hull_spread_scale = range_f32(&mut rng, 0.85, 1.30);
        let hull_drop_scale = range_f32(&mut rng, 0.85, 1.30);
        let mast_height_scale = range_f32(&mut rng, 0.95, 1.55);
        let mast_radius_scale = range_f32(&mut rng, 0.75, 1.35);
        let bow_scale = range_f32(&mut rng, 0.85, 1.50);

        // Smokestack count is archetype-gated so a Solar vessel never
        // sprouts a stray smokestack.
        let smokestack_count = if archetype.has_smokestacks() {
            match archetype {
                VesselArchetype::Steam => sample_u32(&mut rng, 1, 3),
                _ => 1, // Hybrid: always exactly one.
            }
        } else {
            0
        };
        let smokestack_height_scale = range_f32(&mut rng, 0.85, 1.40);

        // Tilt the solar panel between ~5° and ~30° off horizontal.
        // Negative tilts are fine — a panel angled toward the stern
        // reads as a "rear-mounted" deck plate vs a "forward-mounted"
        // dashboard plate.
        let solar_panel_tilt_rad =
            range_f32(&mut rng, -30.0_f32.to_radians(), 30.0_f32.to_radians());

        // Hull-rake band depends on the hull form: barges stay nearly
        // flat (a swept slab reads as melted, not raked), monohulls
        // take the strongest gondola sweep.
        let prow_rake = match hull_form {
            HullForm::Barge => range_f32(&mut rng, 0.0, 0.08),
            HullForm::Monohull => range_f32(&mut rng, 0.20, 0.50),
            HullForm::Catamaran | HullForm::Trimaran => range_f32(&mut rng, 0.10, 0.35),
        };
        let mast_taper = range_f32(&mut rng, 0.15, 0.45);
        let stack_flare = range_f32(&mut rng, -0.35, -0.12);

        Self {
            archetype,
            hull_form,
            bow_style,
            hull_radius_scale,
            hull_length_scale,
            hull_spread_scale,
            hull_drop_scale,
            mast_height_scale,
            mast_radius_scale,
            bow_scale,
            smokestack_count,
            smokestack_height_scale,
            solar_panel_tilt_rad,
            prow_rake,
            mast_taper,
            stack_flare,
        }
    }
}

fn sample_u32(rng: &mut ChaCha8Rng, lo: u32, hi: u32) -> u32 {
    let lo_f = lo as f32;
    let hi_f = (hi + 1) as f32;
    (range_f32(rng, lo_f, hi_f) as u32).clamp(lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = VesselDesign::for_did("did:plc:test");
        let b = VesselDesign::for_did("did:plc:test");
        assert_eq!(a.archetype, b.archetype);
        assert_eq!(a.hull_form, b.hull_form);
        assert_eq!(a.bow_style, b.bow_style);
        assert_eq!(a.hull_radius_scale, b.hull_radius_scale);
        assert_eq!(a.smokestack_count, b.smokestack_count);
        assert_eq!(a.prow_rake, b.prow_rake);
    }

    #[test]
    fn fields_in_range() {
        for s in 0u64..32 {
            let v = VesselDesign::for_did(&format!("did:test:{s}"));
            assert!((0.7..=1.7).contains(&v.hull_radius_scale));
            assert!((0.7..=1.7).contains(&v.hull_length_scale));
            assert!((0.7..=1.7).contains(&v.hull_spread_scale));
            assert!((0.7..=1.7).contains(&v.mast_height_scale));
            assert!((0.5..=1.6).contains(&v.mast_radius_scale));
            assert!((0.7..=1.7).contains(&v.bow_scale));
            assert!(v.smokestack_count <= 3);
            assert!(v.solar_panel_tilt_rad.is_finite());
            assert!((0.0..=0.5).contains(&v.prow_rake));
            assert!((0.15..=0.45).contains(&v.mast_taper));
            assert!((-0.35..=-0.12).contains(&v.stack_flare));
        }
    }

    #[test]
    fn all_hull_forms_reachable() {
        let mut seen = [false; 4];
        for s in 0u64..200 {
            let v = VesselDesign::for_did(&format!("did:test:{s}"));
            let i = HullForm::ALL
                .iter()
                .position(|f| *f == v.hull_form)
                .unwrap();
            seen[i] = true;
        }
        assert_eq!(seen, [true; 4], "some hull form never sampled");
    }

    #[test]
    fn smokestack_count_matches_archetype() {
        // Solar vessels must never sprout a smokestack — verifies the
        // archetype gating in the deriver. Run plenty of seeds because
        // archetype selection is itself random.
        let mut seen_solar = false;
        let mut seen_steam = false;
        for s in 0u64..200 {
            let v = VesselDesign::for_did(&format!("did:test:{s}"));
            match v.archetype {
                VesselArchetype::Solar => {
                    seen_solar = true;
                    assert_eq!(v.smokestack_count, 0, "Solar with smokestacks: {v:?}");
                }
                VesselArchetype::Steam | VesselArchetype::Hybrid => {
                    seen_steam = true;
                    assert!(
                        v.smokestack_count >= 1,
                        "Steam/Hybrid without smokestacks: {v:?}"
                    );
                }
            }
        }
        assert!(
            seen_solar && seen_steam,
            "200 seeds didn't cover both branches"
        );
    }
}
