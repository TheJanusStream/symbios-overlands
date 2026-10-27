//! Obelisk ring - an Alien-Monolithic secondary that leads a crest site. Eight
//! tall black obelisks, 10.5 to 16 m, stand in a ring on a paved crest and
//! lean a little in toward its centre, glyph script lit up their outer faces
//! and a seam of light up their inner ones. At the centre a glyph core hangs
//! over a low dais, held on a thread of light, two tilted rings and four
//! shards turning round it.
//!
//! Each obelisk is a sub-tree rooted on its plinth, which is yawed to face
//! the centre; the shaft leans on the plinth and carries its own glyphs, so
//! a ruined site knocks each stone as one.

use std::f32::consts::{FRAC_PI_4, TAU};

use crate::catalogue::items::fantasy::rune_marks;
use crate::catalogue::items::util::{
    attach, cuboid_tapered, cylinder_tapered, footing_disc, glow, id_quat, nest, prim, quat_mul,
    quat_x, quat_y, quat_z, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{ENERGY_BLUE, GLYPH_CYAN, GLYPH_VIOLET, OBSIDIAN, fx, obsidian};

/// Spacing circle the settlement keeps round the ring.
const CLEARANCE: f32 = 12.5;
/// Radius of the paved crest the ring stands on, and of the ring itself.
const PAVE_R: f32 = 11.2;
const RING_R: f32 = 9.4;
/// Top of the paving (the footing's reveal).
const GRADE: f32 = 0.15;
/// Each obelisk's height, from the back (+Z) round by +X; the tallest
/// stands at the back, over the core as the front of the ring sees it.
const HEIGHTS: [f32; 8] = [16.0, 12.5, 14.0, 11.0, 13.0, 10.5, 14.5, 12.0];
/// Shaft base, across and through, and how much it narrows to its top.
const SHAFT_W: f32 = 1.3;
const SHAFT_D: f32 = 0.9;
const SHAFT_TAPER: f32 = 0.35;
/// The inward lean of every shaft (rad).
const LEAN: f32 = 0.06;
/// The plinth each shaft stands in.
const PLINTH: [f32; 3] = [2.0, 0.7, 1.6];
/// The dais, the core's height over it and the core's radius.
const DAIS_R: f32 = 4.6;
const DAIS_H: f32 = 0.5;
const CORE_Y: f32 = 6.8;
const CORE_R: f32 = 1.0;

pub struct ObeliskRing;

impl CatalogueEntry for ObeliskRing {
    fn slug(&self) -> &'static str {
        "obelisk_ring"
    }
    fn name(&self) -> &'static str {
        "Obelisk Ring"
    }
    fn description(&self) -> &'static str {
        "A ring of tall glyph-lit black obelisks on a crest, round a glyph core floating over a dais."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// A ring of standing stones is raised where it is seen from below.
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Crest
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienMonolithic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MONOLITH_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 50.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Dull black stone for the plinths, caps and dais rims - untextured, so the
/// many small parts stay cheap; the shafts wear the polished obsidian.
fn basalt() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.05, 0.05, 0.08]),
        roughness: Fp(0.35),
        metallic: Fp(0.5),
        ..Default::default()
    }
}

/// One obelisk at `angle` round the ring (0 at +Z, a quarter turn at +X),
/// `h` tall: its plinth, the leaning shaft, the pyramidion and the lit
/// collar under it, glyphs up its outer face and a seam up its inner one.
fn obelisk(angle: f32, h: f32, k: usize) -> Generator {
    let at = [RING_R * angle.sin(), 0.0, RING_R * angle.cos()];
    // The plinth's own -Z faces the centre.
    let plinth = prim(
        solid(cuboid_tapered(PLINTH, 0.12, basalt())),
        [at[0], GRADE + PLINTH[1] * 0.5 - 0.05, at[2]],
        quat_y(angle),
    );
    // In the plinth's frame: the shaft stands in its top and leans toward
    // its -Z, about its own foot.
    let foot_y = GRADE + PLINTH[1] - 0.35;
    let local = |p: [f32; 3]| [at[0] + p[0], p[1], at[2] + p[2]];
    let shaft_at = [0.0, foot_y + h * 0.5 * LEAN.cos(), -h * 0.5 * LEAN.sin()];
    // In the shaft's own frame from here on: `y` up its axis from its centre.
    let in_shaft =
        |p: [f32; 3]| local([shaft_at[0] + p[0], shaft_at[1] + p[1], shaft_at[2] + p[2]]);
    let half_depth = |y: f32| SHAFT_D * 0.5 * (1.0 - SHAFT_TAPER * (y / h + 0.5));
    let mut parts = Vec::new();
    // Glyph script up the outer (+Z) face, each glyph on the face where it
    // stands as the shaft narrows.
    let glyph = glow(GLYPH_CYAN, 2.0);
    let sizes = [1.1_f32, 0.8, 1.2, 0.9, 1.0];
    let count = if h > 13.0 { 5 } else { 4 };
    for (i, &gh) in sizes.iter().take(count).enumerate() {
        let y = -h * 0.5 + 2.0 + i as f32 * (h - 4.4) / (count - 1) as f32;
        let nudge = if (i + k).is_multiple_of(2) {
            -0.12
        } else {
            0.1
        };
        // The branches stand a little prouder than the stave, so no two
        // strokes share a face.
        for (j, mut stroke) in rune_marks(
            in_shaft([nudge, y, half_depth(y) + 0.03]),
            gh,
            glyph.clone(),
        )
        .into_iter()
        .enumerate()
        {
            stroke.transform.translation.0[2] += 0.012 * j as f32;
            parts.push(stroke);
        }
    }
    // A seam of light up the inner face, leaning back with its taper.
    let slope = (SHAFT_D * 0.5 * SHAFT_TAPER / h).atan();
    let seam_len = h - 3.0;
    parts.push(prim(
        cuboid_tapered([0.09, seam_len, 0.06], 0.0, glow(ENERGY_BLUE, 2.4)),
        in_shaft([0.0, 0.0, -(half_depth(0.0) + 0.02)]),
        quat_x(slope),
    ));
    // The pyramidion and the lit collar it sits on.
    let top_w = SHAFT_W * (1.0 - SHAFT_TAPER);
    let top_d = SHAFT_D * (1.0 - SHAFT_TAPER);
    parts.push(prim(
        cuboid_tapered(
            [top_w + 0.1, 0.1, top_d + 0.1],
            0.0,
            glow(GLYPH_VIOLET, 2.2),
        ),
        in_shaft([0.0, h * 0.5 - 0.02, 0.0]),
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([top_w, 1.1, top_d], 0.97, basalt()),
        in_shaft([0.0, h * 0.5 + 0.55, 0.0]),
        id_quat(),
    ));
    let shaft = nest(
        prim(
            solid(cuboid_tapered(
                [SHAFT_W, h, SHAFT_D],
                SHAFT_TAPER,
                obsidian(OBSIDIAN),
            )),
            local(shaft_at),
            quat_x(-LEAN),
        ),
        parts,
    );
    nest(plinth, vec![shaft])
}

/// The dais at the centre: its stepped drum, a lit ring and spokes on its
/// top toward each stone, the thread of light rising from it and the core
/// held on that thread.
fn dais() -> Generator {
    let top = GRADE + DAIS_H;
    let drum = prim(
        solid(cylinder_tapered(DAIS_R, DAIS_H, 32, 0.0, basalt())),
        [0.0, GRADE + DAIS_H * 0.5 - 0.02, 0.0],
        id_quat(),
    );
    let mut parts = vec![
        prim(
            solid(cylinder_tapered(
                DAIS_R + 0.8,
                0.24,
                32,
                0.0,
                obsidian(OBSIDIAN),
            )),
            [0.0, GRADE + 0.1, 0.0],
            id_quat(),
        ),
        prim(
            torus(0.07, DAIS_R - 0.7, glow(ENERGY_BLUE, 2.6)),
            [0.0, top, 0.0],
            id_quat(),
        ),
        prim(
            torus(0.06, 1.3, glow(GLYPH_CYAN, 2.4)),
            [0.0, top, 0.0],
            id_quat(),
        ),
    ];
    for i in 0..HEIGHTS.len() {
        let a = i as f32 * TAU / HEIGHTS.len() as f32;
        let r = (1.3 + DAIS_R - 0.7) * 0.5;
        parts.push(prim(
            cuboid_tapered([0.1, 0.05, DAIS_R - 2.1], 0.0, glow(GLYPH_CYAN, 2.2)),
            [r * a.sin(), top + 0.01, r * a.cos()],
            quat_y(a),
        ));
    }
    // The core, held on its thread: the lit heart, two tilted glyph rings
    // and four shards, which float round it by design.
    let core = prim(
        sphere(CORE_R, 6, glow(GLYPH_CYAN, 2.6)),
        [0.0, CORE_Y, 0.0],
        id_quat(),
    );
    let mut core_parts = vec![
        prim(
            torus(0.08, 2.0, glow(GLYPH_VIOLET, 2.4)),
            [0.0, CORE_Y, 0.0],
            quat_x(0.5),
        ),
        prim(
            torus(0.07, 2.5, glow(ENERGY_BLUE, 2.4)),
            [0.0, CORE_Y, 0.0],
            quat_z(-0.6),
        ),
    ];
    for i in 0..4 {
        let a = i as f32 * TAU / 4.0 + FRAC_PI_4;
        let up = if i % 2 == 0 { 0.45 } else { -0.45 };
        core_parts.push(prim(
            cuboid_tapered([0.42, 1.4, 0.42], 0.85, obsidian(OBSIDIAN)),
            [3.2 * a.sin(), CORE_Y + up, 3.2 * a.cos()],
            quat_mul(quat_y(a), quat_x(0.25)),
        ));
    }
    let thread_h = CORE_Y - top;
    parts.push(nest(
        prim(
            cylinder_tapered(0.07, thread_h, 8, 0.0, glow(ENERGY_BLUE, 2.8)),
            [0.0, top + thread_h * 0.5, 0.0],
            id_quat(),
        ),
        vec![nest(core, core_parts)],
    ));
    nest(drum, parts)
}

fn build_tree() -> Generator {
    // The footing is the paved crest itself, the whole ring standing on it.
    let mut root = footing_disc(PAVE_R, CLEARANCE);
    root.audio = fx::monolith_hum();
    attach(&mut root, dais());
    for (k, &h) in HEIGHTS.iter().enumerate() {
        let angle = k as f32 * TAU / HEIGHTS.len() as f32;
        attach(&mut root, obelisk(angle, h, k));
    }
    attach(&mut root, fx::energy_motes([0.0, 1.0, 0.0], 0x0B1C_0B15));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats_but};
    use crate::catalogue::items::util::{assert_sanitize_stable, has_emissive};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ObeliskRing.build(""), "obelisk_ring");
    }

    /// The core's rings and shards float round it by design; nothing else
    /// may.
    #[test]
    fn nothing_floats_but_the_core_or_z_fights() {
        let g = ObeliskRing.build("");
        assert_nothing_floats_but(&g, "obelisk_ring", &[&[0, 11, 0]]);
        assert_no_z_fighting(&g, "obelisk_ring");
    }

    #[test]
    fn keeps_its_glyphs() {
        assert!(has_emissive(&ObeliskRing.build("")));
    }

    #[test]
    fn the_record_stays_light() {
        let bytes = serde_json::to_vec(&ObeliskRing.build("")).unwrap().len();
        eprintln!("obelisk_ring record: {bytes} B");
        assert!(bytes < 60_000, "{bytes} B");
    }
}
