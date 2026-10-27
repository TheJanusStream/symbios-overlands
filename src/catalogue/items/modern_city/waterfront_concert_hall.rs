//! Waterfront concert hall - a Modern-City secondary that leads a harbour
//! downtown. Sweeping white shell roofs rise in two groups off a broad
//! concrete podium, each shell's open mouth turned to the water on -Z and
//! glazed in dark glass behind steel fins; the front shell's glass is the
//! foyer, lit from dusk to dawn. A stepped plaza runs down to the shore.
//!
//! Primitive-built in one flat ground-relative frame. Each shell is a
//! quarter of a hollow sphere (its upper, +Z half) stretched tall and
//! pressed shallow, so its mouth is a pointed-looking half ellipse; the
//! glazing that fills it is a half disc stretched the same way.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    attach, cuboid_tapered, cylinder_tapered, footing, from_dusk, glow, id_quat, nest, prim,
    prim_scaled, quat_x, solid, sphere, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, SitePreference, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, LAMP_WARM, STEEL_GREY, concrete, steel};

/// Top of the podium the hall stands on.
const PODIUM_TOP: f32 = 1.2;
/// Half the podium's width (X) and depth (Z).
const PODIUM_HX: f32 = 15.0;
const PODIUM_HZ: f32 = 11.0;
/// How a shell's sphere is stretched up and pressed front to back.
const SHELL_RISE: f32 = 1.35;
const SHELL_DEPTH: f32 = 0.75;
/// Wall thickness of a shell, in its own (unscaled) metres.
const SHELL_WALL: f32 = 0.4;
/// The risers of the plaza steps down to the water.
const STEPS: usize = 3;
const STEP_RISE: f32 = 0.3;
const STEP_TREAD: f32 = 0.9;
/// Spacing circle the settlement keeps round the hall.
const CLEARANCE: f32 = 16.0;

/// Warm foyer light seen through the front shell's glass after dark.
const FOYER_WARM: [f32; 3] = [1.0, 0.70, 0.38];
/// Dark harbour glass of the shell mouths.
const GLASS_DARK: [f32; 3] = [0.10, 0.13, 0.17];
/// The glazed white of the shells.
const SHELL_WHITE: [f32; 3] = [0.90, 0.89, 0.85];
/// Paler concrete of the plaza steps.
const STEP_PALE: [f32; 3] = [0.70, 0.69, 0.66];

pub struct WaterfrontConcertHall;

impl CatalogueEntry for WaterfrontConcertHall {
    fn slug(&self) -> &'static str {
        "waterfront_concert_hall"
    }
    fn name(&self) -> &'static str {
        "Waterfront Concert Hall"
    }
    fn description(&self) -> &'static str {
        "Sweeping white shell roofs over a glass foyer facing the water, on a stepped plaza."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// The harbour's edge, its foyer to the water.
    fn site(&self) -> SitePreference {
        SitePreference::Shore
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 44.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The glazed white of a shell: smooth, bright, a little glossy.
fn shell_tile() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(SHELL_WHITE),
        roughness: Fp(0.3),
        metallic: Fp(0.0),
        ..Default::default()
    }
}

/// Dark glass that reads by its reflections, with no light of its own.
fn dark_glass() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(GLASS_DARK),
        roughness: Fp(0.08),
        metallic: Fp(0.6),
        ..Default::default()
    }
}

/// The foyer glass: dark by day, the warm foyer behind it from dusk to dawn.
fn foyer_glass() -> SovereignMaterialSettings {
    from_dusk(SovereignMaterialSettings {
        emission_color: Fp3(FOYER_WARM),
        emission_strength: Fp(1.2),
        ..dark_glass()
    })
}

/// The bore of a shell of outer radius `r`, as the fraction a hollow takes.
fn bore(r: f32) -> f32 {
    ((r - SHELL_WALL) / r).min(0.95)
}

/// Which way a shell's mouth opens.
#[derive(Clone, Copy)]
enum Mouth {
    /// To the water, on -Z.
    Front,
    /// To the land behind, on +Z.
    Back,
}

impl Mouth {
    /// The direction the mouth looks along Z.
    fn sign(self) -> f32 {
        match self {
            Mouth::Front => -1.0,
            Mouth::Back => 1.0,
        }
    }
}

/// One shell standing on the podium, its mouth at `z`: the upper quarter of
/// a hollow sphere of radius `r` centred on (`x`, `z`) - the half behind
/// its mouth - stretched by [`SHELL_RISE`] and pressed by [`SHELL_DEPTH`].
/// Its glazing fills the mouth: a half disc of `glass` behind a row of
/// `fins` steel mullions, a transom across at `transom` above the podium
/// when given.
#[allow(clippy::too_many_arguments)]
fn shell(
    x: f32,
    z: f32,
    r: f32,
    mouth: Mouth,
    glass: SovereignMaterialSettings,
    fins: u32,
    transom: Option<f32>,
) -> Vec<Generator> {
    let y = PODIUM_TOP - 0.05;
    let inner = r * bore(r);
    let s = mouth.sign();
    // The sphere's +Z half is longitudes 0 to a half turn.
    let (half, stand) = match mouth {
        Mouth::Front => ([0.0, 0.5], quat_x(-FRAC_PI_2)),
        Mouth::Back => ([0.5, 1.0], quat_x(FRAC_PI_2)),
    };
    let mut out = vec![prim_scaled(
        solid(with_cut(
            sphere(r, 6, shell_tile()),
            half,
            [0.5, 1.0],
            bore(r),
        )),
        [x, y, z],
        id_quat(),
        [1.0, SHELL_RISE, SHELL_DEPTH],
    )];
    // The glazing: a thin half disc stood in the mouth, a hand's depth
    // inside it, its rim tucked into the shell's wall.
    let glass_r = inner + 0.12;
    out.push(prim_scaled(
        with_cut(
            cylinder_tapered(glass_r, 0.08, 40, 0.0, glass),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        [x, y, z - 0.14 * s],
        stand,
        [1.0, 1.0, SHELL_RISE],
    ));
    // Steel fins standing just proud of the glass, each up to the rim.
    let reach = inner + 0.15;
    for i in 1..=fins {
        let dx = reach * (2.0 * i as f32 / (fins + 1) as f32 - 1.0);
        let h = SHELL_RISE * (reach * reach - dx * dx).sqrt();
        out.push(prim(
            solid(cuboid_tapered([0.18, h, 0.3], 0.0, steel(STEEL_GREY))),
            [x + dx, y + h * 0.5, z + 0.08 * s],
            id_quat(),
        ));
    }
    if let Some(t) = transom {
        let half = (inner * inner - (t / SHELL_RISE).powi(2)).sqrt();
        out.push(prim(
            solid(cuboid_tapered(
                [half * 2.0, 0.2, 0.22],
                0.0,
                steel(STEEL_GREY),
            )),
            [x, PODIUM_TOP + t, z + 0.15 * s],
            id_quat(),
        ));
    }
    out
}

/// A plaza lamp: a slim steel column under a warm lantern, lit at every
/// hour as a lamp is.
fn lamp(x: f32, z: f32) -> Generator {
    let h = 4.2;
    nest(
        prim(
            solid(cylinder_tapered(0.12, h, 10, 0.15, steel(STEEL_GREY))),
            [x, PODIUM_TOP + h * 0.5, z],
            id_quat(),
        ),
        vec![
            prim(
                cylinder_tapered(0.26, 0.5, 12, 0.0, glow(LAMP_WARM, 3.0)),
                [x, PODIUM_TOP + h + 0.2, z],
                id_quat(),
            ),
            prim(
                cylinder_tapered(0.34, 0.1, 12, 0.3, steel(STEEL_GREY)),
                [x, PODIUM_TOP + h + 0.5, z],
                id_quat(),
            ),
        ],
    )
}

fn build_tree() -> Generator {
    let steps_front = -PODIUM_HZ - STEPS as f32 * STEP_TREAD;
    let mut root = footing(
        PODIUM_HX * 2.0,
        PODIUM_HZ - steps_front,
        [0.0, (PODIUM_HZ + steps_front) * 0.5],
        CLEARANCE,
    );

    // The hall: three great shells rising toward the back, their foyer to
    // the water, and a pair of small ones to the east over the restaurant.
    let mut hall = Vec::new();
    use Mouth::{Back, Front};
    hall.extend(shell(-3.5, -6.0, 6.5, Front, foyer_glass(), 7, Some(3.2)));
    hall.extend(shell(-3.5, -2.5, 8.5, Front, dark_glass(), 7, None));
    hall.extend(shell(-3.5, 1.5, 10.5, Front, dark_glass(), 9, None));
    // The stage house's back shell, turned to the land.
    hall.extend(shell(-3.5, 9.6, 6.0, Back, dark_glass(), 5, None));
    hall.extend(shell(10.5, 4.6, 3.6, Front, foyer_glass(), 3, None));
    hall.extend(shell(10.5, 6.8, 4.4, Front, dark_glass(), 3, None));
    for (x, z) in [(-13.0, -9.6), (6.0, -9.6), (13.0, -9.6), (13.0, 1.0)] {
        hall.push(lamp(x, z));
    }
    // Low bollards along the head of the steps.
    for i in 0..7 {
        let x = -12.0 + i as f32 * 4.0;
        hall.push(prim(
            solid(cylinder_tapered(0.14, 0.8, 10, 0.1, steel(STEEL_GREY))),
            [x, PODIUM_TOP + 0.4, -PODIUM_HZ + 0.5],
            id_quat(),
        ));
    }
    let podium = nest(
        prim(
            solid(cuboid_tapered(
                [PODIUM_HX * 2.0, PODIUM_TOP, PODIUM_HZ * 2.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, PODIUM_TOP * 0.5, 0.0],
            id_quat(),
        ),
        hall,
    );
    attach(&mut root, podium);

    // The broad steps down to the water, each a little narrower than the
    // one below so no two ends share a plane, all reaching under the
    // podium's edge.
    for k in 0..STEPS {
        let top = STEP_RISE * (k + 1) as f32;
        let front = steps_front + k as f32 * STEP_TREAD;
        let back = -PODIUM_HZ + 0.2;
        attach(
            &mut root,
            prim(
                solid(cuboid_tapered(
                    [26.0 - 0.4 * k as f32, top, back - front],
                    0.0,
                    concrete(STEP_PALE),
                )),
                [0.0, top * 0.5, (front + back) * 0.5],
                id_quat(),
            ),
        );
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&WaterfrontConcertHall.build(""), "waterfront_concert_hall");
    }

    #[test]
    fn nothing_floats_or_z_fights() {
        let built = WaterfrontConcertHall.build("");
        assert_no_z_fighting(&built, "waterfront_concert_hall");
        assert_nothing_floats(&built, "waterfront_concert_hall");
    }

    /// A landmark's record stays well inside the 100 KiB budget.
    #[test]
    fn stays_well_inside_the_record_budget() {
        let bytes = serde_json::to_vec(&WaterfrontConcertHall.build(""))
            .expect("a built entry serialises")
            .len();
        assert!(
            bytes < 60_000,
            "waterfront_concert_hall serialises to {bytes} B"
        );
    }
}
