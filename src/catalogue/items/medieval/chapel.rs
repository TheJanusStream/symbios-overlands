//! Chapel — a Medieval secondary. A parish church of dressed ashlar on a
//! fieldstone footing: a steep slate gable nave with pointed-arch stained
//! lancets down each flank, stepped corner buttresses, a battlemented west
//! tower with corner pinnacles and a pointed-arch oak doorway, and an east
//! window with a stone cross at the gable. The quiet civic heart of the
//! burgh — built from the shared `gable_roof` (nordic), `pointed_arch`
//! (gothic) and `crenellations` (medieval) vocabulary.

use crate::catalogue::items::gothic_horror::pointed_arch;
use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cuboid_tapered_xz, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignMaterialSettings, SovereignStainedGlassConfig,
    SovereignTextureConfig,
};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    IRON_DARK, SLATE_GREY, STONE_GREY, STONE_PALE, WOOD_DARK, crenellations, iron, rough_stone,
    shingle, stone, timber,
};

pub struct Chapel;

impl CatalogueEntry for Chapel {
    fn slug(&self) -> &'static str {
        "chapel"
    }
    fn name(&self) -> &'static str {
        "Chapel"
    }
    fn description(&self) -> &'static str {
        "Stone parish church: a slate gable nave of pointed-arch lancets under a battlemented west tower."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.5,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Coloured leaded glass for the lancets — a deep jewel surface so the
/// daylit chapel glints without the forge's emissive glow.
fn stained() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.22, 0.30, 0.50]),
        roughness: Fp(0.1),
        metallic: Fp(0.2),
        uv_scale: Fp(1.0),
        texture: SovereignTextureConfig::StainedGlass(SovereignStainedGlassConfig {
            cell_count: 10,
            saturation: Fp(0.95),
            grime_level: Fp64(0.12),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// One pointed-arch stained lancet on a ±Z flank wall: a jewel light set
/// proud of the wall, two jamb ribs, a sill, and a `pointed_arch` head.
/// `cx` is the window centre along X, `zf` the wall face Z (sign picks the
/// proud direction), `sill` the sill height, `half_w` the half opening width,
/// `body_h` the straight light height below the springline.
fn lancet(cx: f32, zf: f32, sill: f32, half_w: f32, body_h: f32) -> Vec<Generator> {
    let n = if zf < 0.0 { -1.0_f32 } else { 1.0 };
    let glass_z = zf + 0.03 * n;
    let rib_z = zf + 0.06 * n;
    let spring = sill + body_h;
    let glass_h = body_h + half_w * 0.7;
    let mut v = vec![
        // Jewel light, slightly proud of the wall.
        prim(
            cuboid_tapered([half_w * 1.7, glass_h, 0.1], 0.0, stained()),
            [cx, sill + glass_h * 0.5, glass_z],
            id_quat(),
        ),
        // Stone sill ledge.
        prim(
            solid(cuboid_tapered(
                [half_w * 2.0 + 0.2, 0.12, 0.26],
                0.0,
                stone(STONE_PALE),
            )),
            [cx, sill - 0.04, rib_z],
            id_quat(),
        ),
    ];
    // Two jamb ribs framing the light.
    for s in [-1.0_f32, 1.0] {
        v.push(prim(
            solid(cuboid_tapered([0.1, body_h, 0.2], 0.0, stone(STONE_PALE))),
            [cx + s * half_w, sill + body_h * 0.5, rib_z],
            id_quat(),
        ));
    }
    // Pointed-arch head (two stone arcs meeting at the apex).
    v.extend(pointed_arch(
        [cx, spring, rib_z],
        half_w,
        0.09,
        stone(STONE_PALE),
    ));
    v
}

fn build_tree() -> Generator {
    let l = 7.0_f32; // nave length (X)
    let w = 4.6_f32; // nave width (Z); long flanks face ±Z (camera = −Z)
    let foot_h = 0.4;
    let wall_h = 4.0;
    let wall_top = foot_h + wall_h;
    let roof_rise = 2.6;
    let ridge_y = wall_top + roof_rise;
    let east = l * 0.5; // +X chancel gable

    let mut prims = vec![
        // Fieldstone footing — the root (identity rotation).
        prim(
            solid(cuboid_tapered(
                [l + 1.0, foot_h, w + 1.0],
                0.0,
                rough_stone(STONE_GREY),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
        // Dressed-ashlar nave body.
        prim(
            solid(cuboid_tapered([l, wall_h, w], 0.0, stone(STONE_PALE))),
            [0.0, foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Steep slate gable roof over the nave (ridge ‖ X, slopes face ±Z).
    prims.push(gable_roof(
        [l + 0.7, roof_rise, w + 0.8],
        [0.0, wall_top + roof_rise * 0.5, 0.0],
        shingle(SLATE_GREY),
    ));
    // Triangular ashlar gable-end infill so no daylight shows under the slate.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered_xz(
                [0.3, roof_rise, w],
                [0.0, 0.94],
                stone(STONE_PALE),
            )),
            [sx * (l * 0.5 - 0.04), wall_top + roof_rise * 0.5, 0.0],
            id_quat(),
        ));
    }
    // Ridge beam capping the apex.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 0.7, 0.18, 0.22],
            0.0,
            timber(WOOD_DARK),
        )),
        [0.0, ridge_y, 0.0],
        id_quat(),
    ));

    // Pointed-arch stained lancets down both flanks (camera sees the −Z run).
    for &zf in &[-(w * 0.5), w * 0.5] {
        for &cx in &[-0.7_f32, 1.2, 2.9] {
            prims.extend(lancet(cx, zf, foot_h + 1.4, 0.5, 1.7));
        }
    }

    // Stepped corner buttresses with weathered set-off caps along the flanks.
    for sz in [-1.0_f32, 1.0] {
        for cx in [-0.2_f32, 2.6] {
            let bz = sz * (w * 0.5 + 0.2);
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.55, wall_h * 0.82, 0.5],
                    0.0,
                    stone(STONE_GREY),
                )),
                [cx, foot_h + wall_h * 0.41, bz],
                id_quat(),
            ));
            prims.push(prim(
                solid(cuboid_tapered([0.55, 0.5, 0.5], 0.7, stone(STONE_GREY))),
                [cx, foot_h + wall_h * 0.82 + 0.22, bz],
                id_quat(),
            ));
        }
    }

    // ── East (+X) chancel gable: a tall lancet + a stone cross on the apex ──
    let ew_h = 2.4;
    let ew_sill = foot_h + 1.6;
    prims.push(prim(
        cuboid_tapered([0.1, ew_h, 1.3], 0.0, stained()),
        [east + 0.04, ew_sill + ew_h * 0.5, 0.0],
        id_quat(),
    ));
    // Stone surround mullion + transom across the east window.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.16, ew_h + 0.3, 0.12],
            0.0,
            stone(STONE_PALE),
        )),
        [east + 0.07, ew_sill + ew_h * 0.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.16, 0.12, 1.4], 0.0, stone(STONE_PALE))),
        [east + 0.07, ew_sill + ew_h * 0.6, 0.0],
        id_quat(),
    ));
    let cross_y = ridge_y + 0.7;
    prims.push(prim(
        solid(cuboid_tapered([0.16, 1.0, 0.16], 0.0, stone(STONE_PALE))),
        [east - 0.1, cross_y, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.16, 0.16, 0.66], 0.0, stone(STONE_PALE))),
        [east - 0.1, cross_y + 0.2, 0.0],
        id_quat(),
    ));

    // ── West (−X) battlemented tower ──
    let tx = -l * 0.5 - 0.4; // tower centre, engaged with the west gable
    let thw = 1.5; // tower half-width (X)
    let thz = 1.7; // tower half-depth (Z)
    let tower_h = 7.4;
    let tower_top = foot_h + tower_h;
    // Tower shaft.
    prims.push(prim(
        solid(cuboid_tapered(
            [thw * 2.0, tower_h, thz * 2.0],
            0.0,
            stone(STONE_GREY),
        )),
        [tx, foot_h + tower_h * 0.5, 0.0],
        id_quat(),
    ));
    // Corbel string-course just under the parapet.
    prims.push(prim(
        solid(cuboid_tapered(
            [thw * 2.0 + 0.24, 0.3, thz * 2.0 + 0.24],
            0.0,
            stone(STONE_PALE),
        )),
        [tx, tower_top - 0.15, 0.0],
        id_quat(),
    ));
    // Battlemented parapet — the medieval crenellation ring.
    prims.extend(crenellations(
        [tx, tower_top, 0.0],
        thw + 0.12,
        thz + 0.12,
        0.75,
        0.42,
        0.34,
        stone(STONE_GREY),
    ));
    // Four corner pinnacles rising above the battlements.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cone(0.34, 1.3, 6, shingle(SLATE_GREY))),
            [tx + sx * (thw + 0.04), tower_top + 0.95, sz * (thz + 0.04)],
            id_quat(),
        ));
    }
    // Belfry louvre slits high on the tower's −Z (camera) face.
    for sx in [-0.55_f32, 0.55] {
        prims.push(prim(
            cuboid_tapered([0.34, 1.1, 0.1], 0.0, timber(WOOD_DARK)),
            [tx + sx, foot_h + tower_h * 0.72, -(thz + 0.02)],
            id_quat(),
        ));
    }

    // Pointed-arch oak west doorway on the tower's −Z (camera) face.
    let door_z = -(thz + 0.02);
    let door_hw = 0.62;
    let door_body = 1.9;
    let door_sill = foot_h;
    // Dark recess + oak door leaf, banded with iron.
    prims.push(prim(
        solid(cuboid_tapered(
            [door_hw * 2.0, door_body + door_hw, 0.18],
            0.0,
            timber(WOOD_DARK),
        )),
        [tx, door_sill + (door_body + door_hw) * 0.5, door_z - 0.04],
        id_quat(),
    ));
    for ty in [0.65_f32, 1.7] {
        prims.push(prim(
            solid(cuboid_tapered(
                [door_hw * 2.0, 0.12, 0.1],
                0.0,
                iron(IRON_DARK),
            )),
            [tx, door_sill + ty, door_z - 0.1],
            id_quat(),
        ));
    }
    // Stone jambs + pointed-arch hood over the doorway.
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.13, door_body, 0.24],
                0.0,
                stone(STONE_PALE),
            )),
            [tx + s * door_hw, door_sill + door_body * 0.5, door_z - 0.08],
            id_quat(),
        ));
    }
    prims.extend(pointed_arch(
        [tx, door_sill + door_body, door_z - 0.08],
        door_hw,
        0.11,
        stone(STONE_PALE),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Chapel.build(""), "chapel");
    }
}
