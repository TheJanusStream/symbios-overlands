//! The facade vocabulary of the kit's downtown buildings (#1559): glazing,
//! cladding and the few lights a real building carries, laid by texture
//! wherever a district of sixty buildings would otherwise pay for detail in
//! parts.
//!
//! # A curtain wall is a stack-bond `Brick`
//!
//! The `Brick` generator lays `scale` rows up a tile and
//! `round(scale x aspect_ratio)` columns across it, and under the metre UV
//! convention a tile is square in metres - so `aspect_ratio` is columns over
//! rows, never a panel's width over its height (the #967 trap). With
//! `row_offset` 0 the bond stacks: each brick is one glass panel, the mortar
//! is the frame round it, and one prim carries a whole tower face of
//! mullions and floor transoms. The colours live in the texture
//! (`color_brick` the glass, `color_mortar` the frame) over a white
//! `base_color`, because `base_color` multiplies the bake.
//!
//! The mortar is one fraction of the cell both ways, so a tall panel gets a
//! deep transom and a slim mullion from one number: on a 1.6 m x 4 m office
//! panel, [`OFFICE_JOINT`] draws a 16 cm mullion and a 40 cm spandrel band
//! at every floor. Make the joint fat and the same generator lays a punched
//! facade instead - the cells are windows, the mortar is the wall
//! ([`PUNCHED_JOINT`]).
//!
//! A Box projection is centred on its prim, so [`facade`] slides the pattern
//! into the world frame: transoms land on the floor edges above a datum,
//! mullions on a grid through the building's axis. Every tower face here is
//! a whole, even number of panels wide and centred on that axis, so the four
//! side faces of one prim agree and a mullion runs down every corner.
//!
//! Never lay a panel grid on a tapered or twisted prim: the deform shears the
//! grid, and the owner saw exactly that on Isoline's Spire ("the uv-mapping
//! makes it look crooked"). A turn is built from straight pieces.
//!
//! # No emission on glass
//!
//! `emission_strength` adds its colour evenly over a whole surface, panes
//! and frames alike: on dark glass 0.05 of warm white turned a tower
//! grey-brown, and 0.3 made a lobby read as a beige wall. Glass here carries
//! none and reads by its reflections. What lights a building at dusk is small
//! and purposeful - a canopy soffit, a lobby seen through its glazing, a
//! framed sign, a thin crown line, the red aviation lights - and a broad lit
//! face stays far below the strength a thin line can take (see the kit's
//! emissive discipline in [`super`]).

use bevy_symbios_texture::metal::MetalStyle;

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cuboid_tapered, face_uv_offset, glow, id_quat, lit_interior, plane, prim, quarter_turn, quat_x,
    solid, sphere, tiles_per_metre, window_card,
};
use crate::pds::generator::FaceKey;
use crate::pds::{
    Fp, Fp2, Fp3, Fp64, Generator, SovereignBrickConfig, SovereignConcreteConfig,
    SovereignCorrugatedConfig, SovereignMaterialSettings, SovereignMetalConfig,
    SovereignPaversConfig, SovereignTextureConfig,
};

/// One facade module: panel (mullion-to-mullion) width by floor-to-floor
/// height, laid on a square tile holding a whole number of each.
#[derive(Clone, Copy, Debug)]
pub(super) struct Module {
    /// Panel width (m).
    pub(super) panel: f32,
    /// Floor-to-floor height (m).
    pub(super) floor: f32,
    /// The texture tile's edge (m): a whole number of panels and of floors.
    pub(super) tile: f32,
}

impl Module {
    /// Rows (floors) per tile.
    fn rows(&self) -> f64 {
        (self.tile / self.floor).round() as f64
    }
    /// Columns (panels) per tile.
    fn cols(&self) -> f64 {
        (self.tile / self.panel).round() as f64
    }
}

/// An office curtain wall: 1.6 m panels on 4 m floors, two rows of five on
/// an 8 m tile.
pub(super) const OFFICE: Module = Module {
    panel: 1.6,
    floor: 4.0,
    tile: 8.0,
};
/// A residential curtain wall: 1.6 m panels on 3.2 m floors, two rows of
/// four on a 6.4 m tile.
pub(super) const RESIDENTIAL: Module = Module {
    panel: 1.6,
    floor: 3.2,
    tile: 6.4,
};
/// Mid-rise flats and offices: 1.7 m bays on 3.4 m floors, two rows of four
/// on a 6.8 m tile.
pub(super) const MIDRISE: Module = Module {
    panel: 1.7,
    floor: 3.4,
    tile: 6.8,
};
/// A media block's office floors: 1.6 m bays on 3.6 m floors, four rows of
/// nine on a 14.4 m tile.
pub(super) const MEDIA: Module = Module {
    panel: 1.6,
    floor: 3.6,
    tile: 14.4,
};

/// Mortar fraction for a curtain wall: the generator draws the brick within
/// its bevel radius of an inner box `0.5 - mortar - bevel_r` from the cell
/// centre, so the brick reaches `0.5 - mortar` and the frame is twice the
/// mortar, 10% of the cell each way - a 16 cm mullion and a 40 cm transom
/// on an [`OFFICE`] panel.
pub(super) const OFFICE_JOINT: f64 = 0.05;
/// Mortar fraction for a punched facade: the cell keeps 60% of its width
/// and height as window, the rest is wall.
pub(super) const PUNCHED_JOINT: f64 = 0.2;

/// An `f64` on the record's fixed-point grid, so a value that equals a
/// config default elides the same way before and after a round trip (#943).
fn g64(x: f64) -> Fp64 {
    Fp64((x * 10_000.0).round() / 10_000.0)
}

/// A gridded facade on the prim centred at `center`: `cell` colours the
/// glass, `frame` the mullions and transoms (or, with a fat `joint`, the
/// wall between punched windows). `datum` is the height (m, in the same
/// frame as `center`) of any one floor edge - the transoms land on it and
/// every `module.floor` above and below.
///
/// The prim's side faces must be a whole, even number of `module.panel`
/// wide and centred on `x = z = 0` for a mullion (or a pier) to run down
/// every corner; a prim off that axis keeps its rows and lets its columns
/// fall where they fall.
pub(super) fn facade(
    module: Module,
    cell: [f32; 3],
    frame: [f32; 3],
    joint: f64,
    center: [f32; 3],
    datum: f32,
) -> SovereignMaterialSettings {
    let [u, v] = face_uv_offset(FaceKey::SideNz, center).0;
    SovereignMaterialSettings {
        base_color: Fp3([1.0, 1.0, 1.0]),
        roughness: Fp(0.32),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(module.tile),
        uv_offset: Fp2([u, v + datum]),
        texture: SovereignTextureConfig::Brick(SovereignBrickConfig {
            scale: g64(module.rows()),
            row_offset: g64(0.0),
            aspect_ratio: g64(module.cols() / module.rows()),
            mortar_size: g64(joint),
            bevel: g64(0.2),
            cell_variance: g64(0.03),
            roughness: g64(0.08),
            color_brick: Fp3(cell),
            color_mortar: Fp3(frame),
            normal_strength: Fp(1.5),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Horizontal louvre blades - a plant floor, a crown's plant screen, a
/// street-level intake. Ribbed sheet turned a quarter so the ribs run
/// across, 0.2 m apart.
pub(super) fn louvre(color: [f32; 3]) -> SovereignMaterialSettings {
    quarter_turn(SovereignMaterialSettings {
        base_color: Fp3([1.0, 1.0, 1.0]),
        roughness: Fp(0.55),
        metallic: Fp(0.5),
        uv_scale: tiles_per_metre(2.4),
        texture: SovereignTextureConfig::Corrugated(SovereignCorrugatedConfig {
            color_metal: Fp3(color),
            color_rust: Fp3(color),
            ridges: g64(12.0),
            ridge_depth: g64(1.4),
            roughness: g64(0.5),
            metallic: Fp(0.5),
            rust_level: g64(0.0),
            normal_strength: Fp(3.0),
            ..Default::default()
        }),
        ..Default::default()
    })
}

/// Fluted stone - a tower's podium cladding. Board-formed concrete's
/// formwork lines turned upright, 0.3 m apart, in a dark polished stone.
pub(super) fn fluted_stone(color: [f32; 3]) -> SovereignMaterialSettings {
    quarter_turn(SovereignMaterialSettings {
        base_color: Fp3([1.0, 1.0, 1.0]),
        roughness: Fp(0.45),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(2.4),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            color_pit: Fp3(color),
            formwork_lines: g64(8.0),
            formwork_depth: g64(0.3),
            pit_density: g64(0.0),
            roughness: g64(0.2),
            ..Default::default()
        }),
        ..Default::default()
    })
}

/// Smooth precast concrete - parking spandrels, cores, parapets - with a
/// faint horizontal panel joint every 1.2 m.
pub(super) fn precast(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([1.0, 1.0, 1.0]),
        roughness: Fp(0.85),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(2.4),
        texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
            color_base: Fp3(color),
            color_pit: Fp3(crate::catalogue::items::util::tint(
                color,
                [0.75, 0.75, 0.75],
            )),
            formwork_lines: g64(2.0),
            formwork_depth: g64(0.08),
            pit_density: g64(0.06),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Large-format paving - plinths, forecourts, roof terraces.
pub(super) fn paving(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([1.0, 1.0, 1.0]),
        roughness: Fp(0.8),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(3.6),
        texture: SovereignTextureConfig::Pavers(SovereignPaversConfig {
            scale: g64(4.0),
            aspect_ratio: g64(1.0),
            grout_width: g64(0.03),
            cell_variance: g64(0.06),
            color_stone: Fp3(color),
            color_grout: Fp3(crate::catalogue::items::util::tint(color, [0.6, 0.6, 0.6])),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Perforated metal - a parking deck's screen. Holes 0.18 m across on a
/// 0.3 m pitch, darkened where they punch through.
pub(super) fn perforated(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([1.0, 1.0, 1.0]),
        roughness: Fp(0.5),
        metallic: Fp(0.6),
        uv_scale: tiles_per_metre(2.4),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Perforated,
            scale: g64(8.0),
            hole_size: g64(0.6),
            color_metal: Fp3(color),
            color_rust: Fp3(color),
            roughness: g64(0.45),
            metallic: Fp(0.6),
            rust_level: g64(0.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Brushed dark steel - masts, frames, fins, canopy edges.
pub(super) fn steel(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([1.0, 1.0, 1.0]),
        roughness: Fp(0.4),
        metallic: Fp(0.8),
        uv_scale: tiles_per_metre(1.2),
        texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
            style: MetalStyle::Brushed,
            color_metal: Fp3(color),
            color_rust: Fp3(color),
            roughness: g64(0.4),
            metallic: Fp(0.8),
            rust_level: g64(0.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Glass, frame and wall colours shared by the kit's towers.
pub(super) mod palette {
    /// Dark office glass, cool grey-blue.
    pub(in super::super) const GLASS_OFFICE: [f32; 3] = [0.030, 0.040, 0.055];
    /// Dark residential glass, a touch of bronze.
    pub(in super::super) const GLASS_BRONZE: [f32; 3] = [0.050, 0.045, 0.040];
    /// Graphite aluminium - mullions, transoms, spandrel bands.
    pub(in super::super) const FRAME_GRAPHITE: [f32; 3] = [0.080, 0.085, 0.095];
    /// Near-black louvre and fin metal.
    pub(in super::super) const LOUVRE_DARK: [f32; 3] = [0.060, 0.064, 0.070];
    /// Dark granite podium stone.
    pub(in super::super) const STONE_DARK: [f32; 3] = [0.090, 0.090, 0.095];
    /// Mid-grey precast concrete.
    pub(in super::super) const CONCRETE_MID: [f32; 3] = [0.36, 0.36, 0.35];
    /// Roof membrane and terrace grey.
    pub(in super::super) const ROOF_GREY: [f32; 3] = [0.13, 0.13, 0.14];
    /// Warm lobby and shop light.
    pub(in super::super) const WARM_LIGHT: [f32; 3] = [1.0, 0.80, 0.56];
    /// Cool white accent line.
    pub(in super::super) const COOL_WHITE: [f32; 3] = [0.82, 0.90, 1.0];
    /// Aviation obstruction red.
    pub(in super::super) const AVIATION_RED: [f32; 3] = [1.0, 0.08, 0.05];
}

/// A solid axis-aligned slab at `center` - walls, slabs, tiers, piers.
pub(super) fn block(
    size: [f32; 3],
    center: [f32; 3],
    material: SovereignMaterialSettings,
) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, material)),
        center,
        id_quat(),
    )
}

/// A thin non-solid lit bar - a crown line, a sign border, a lamp head.
pub(super) fn light_bar(
    size: [f32; 3],
    center: [f32; 3],
    color: [f32; 3],
    strength: f32,
) -> Generator {
    prim(
        cuboid_tapered(size, 0.0, glow(color, strength)),
        center,
        id_quat(),
    )
}

/// A red aviation obstruction light, `radius` across, centred at `center`.
pub(super) fn aviation_light(center: [f32; 3], radius: f32) -> Generator {
    prim(
        sphere(radius, 4, glow(palette::AVIATION_RED, 6.0)),
        center,
        id_quat(),
    )
}

/// A glazed lobby set into the front (`-Z`) of a podium: two piers framing
/// the opening, a lit room behind it, and glazing whose panes are cut away
/// so the room's light reads through dark frames.
///
/// The glazing is a `Window` card on a plane in the reveal, lapped 3 cm into
/// both piers, the floor and the head, so no edge of it stands in the open
/// and no edge ties with a face. The lit room runs 2 cm under the floor and
/// 3 cm past the head - into the slab and the canopy - and never ends in a
/// plane a pier's face lies in.
pub(super) struct Lobby {
    /// Half the podium's width (m): the piers stand 5 cm inside it.
    pub(super) half_w: f32,
    /// The opening's floor and head (m).
    pub(super) floor: f32,
    pub(super) head: f32,
    /// The glazing plane (z, m) - the piers stand 0.55 m proud of it.
    pub(super) glaze_z: f32,
    /// The podium face the lobby backs onto (z, m).
    pub(super) back_z: f32,
    pub(super) pier_w: f32,
    pub(super) panes: (u32, u32),
    /// The lit room's colour, and how brightly it reads (0.1-0.6).
    pub(super) room: [f32; 3],
    pub(super) lit: f32,
}

/// The parts of a [`Lobby`]: two piers in `pier`, the lit room, the glazing.
pub(super) fn lobby(l: &Lobby, pier: SovereignMaterialSettings) -> Vec<Generator> {
    let pier_z0 = l.glaze_z - 0.55;
    let pier_z1 = l.back_z + 0.05;
    let pier_x = l.half_w - 0.05 - l.pier_w * 0.5;
    // Piers start 1 cm into the slab they stand on, so their feet never
    // share the plane of the podium's.
    let h = l.head + 0.05 - (l.floor - 0.01);
    let cy = l.floor - 0.01 + h * 0.5;
    let mut out = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        out.push(block(
            [l.pier_w, h, pier_z1 - pier_z0],
            [sx * pier_x, cy, (pier_z0 + pier_z1) * 0.5],
            pier.clone(),
        ));
    }
    let opening = (pier_x - l.pier_w * 0.5) * 2.0;
    let (room_y0, room_y1) = (l.floor - 0.02, l.head + 0.03);
    let room_z0 = l.glaze_z + 0.1;
    let room = lit_room(
        [opening + 0.1, room_y1 - room_y0, pier_z1 - 0.01 - room_z0],
        [
            0.0,
            (room_y0 + room_y1) * 0.5,
            (room_z0 + pier_z1 - 0.01) * 0.5,
        ],
        l.room,
        l.lit,
    );
    out.push(room);
    out.push(glazing(
        [opening + 0.06, l.head - l.floor + 0.06],
        [0.0, (l.floor + l.head) * 0.5, l.glaze_z],
        l.panes,
    ));
    out
}

/// A lit room of `size` at `center`: the dim self-lit inside a card's cut
/// panes show.
pub(super) fn lit_room(size: [f32; 3], center: [f32; 3], room: [f32; 3], lit: f32) -> Generator {
    prim(
        cuboid_tapered(size, 0.0, lit_interior(room, lit)),
        center,
        id_quat(),
    )
}

/// Glazing on the `-Z`-facing plane through `center`: a `Window` card of
/// `size` (`[width, height]`) whose panes are cut away.
pub(super) fn glazing(size: [f32; 2], center: [f32; 3], panes: (u32, u32)) -> Generator {
    prim(
        plane(
            size,
            window_card([0.05, 0.05, 0.05], panes.0, panes.1, 0.3, 0.02),
        ),
        center,
        quat_x(-FRAC_PI_2),
    )
}

/// A framed sign on a wall facing `-Z` at `wall_z`: a dark steel backing
/// bedded 2 cm into the wall and a lit face proud of it with a 0.2 m dark
/// margin round it - lit colour in a frame, never a bare glowing slab.
pub(super) fn framed_sign(
    wall_z: f32,
    center: [f32; 2],
    size: [f32; 2],
    color: [f32; 3],
    strength: f32,
) -> Vec<Generator> {
    let [cx, cy] = center;
    let [w, h] = size;
    let back_t = 0.14;
    let back_z = wall_z - back_t * 0.5 + 0.02;
    vec![
        block(
            [w, h, back_t],
            [cx, cy, back_z],
            steel(palette::LOUVRE_DARK),
        ),
        light_bar(
            [w - 0.4, h - 0.4, 0.06],
            [cx, cy, back_z - back_t * 0.5 - 0.01],
            color,
            strength,
        ),
    ]
}

/// Every gridded facade in a built tree - a `Brick` stack bond as a
/// cuboid's material or one face's - as `(centre in the ground frame,
/// size, material, is the base material)`.
#[cfg(test)]
pub(super) fn facades(
    root: &Generator,
) -> Vec<([f32; 3], [f32; 3], SovereignMaterialSettings, bool)> {
    fn walk(
        g: &Generator,
        at: [f32; 3],
        out: &mut Vec<([f32; 3], [f32; 3], SovereignMaterialSettings, bool)>,
    ) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        if let crate::pds::GeneratorKind::Cuboid { size, common, .. } = &g.kind {
            let is_brick = |m: &SovereignMaterialSettings| {
                matches!(m.texture, SovereignTextureConfig::Brick(_))
            };
            if is_brick(&common.material) {
                out.push((here, size.0, common.material.clone(), true));
            }
            for f in &common.faces {
                if is_brick(&f.material) {
                    out.push((here, size.0, f.material.clone(), false));
                }
            }
        }
        for c in &g.children {
            walk(c, here, out);
        }
    }
    let mut out = Vec::new();
    walk(root, [0.0; 3], &mut out);
    out
}

/// How far a texture coordinate `t` (in tiles) sits from the nearest
/// boundary of `cells` cells per tile, in cells.
#[cfg(test)]
fn off_grid(t: f32, cells: f64) -> f32 {
    let phase = t * cells as f32;
    (phase - phase.round()).abs()
}

/// The rows and columns a facade material lays per tile.
#[cfg(test)]
fn grid_of(m: &SovereignMaterialSettings) -> (f64, f64) {
    let SovereignTextureConfig::Brick(cfg) = &m.texture else {
        panic!("not a gridded facade");
    };
    (cfg.scale.0, (cfg.scale.0 * cfg.aspect_ratio.0).round())
}

/// Assert that every gridded facade in `root` puts a transom on the floor
/// line at height `datum` (#1559): a Box projection is centred on its prim,
/// so without the datum the transoms land wherever the prim's centre puts
/// them - 1.6 m off the floor edges on the megatower's first tier. Read
/// from each material's own offset, scale and row count; returns how many
/// facades it checked.
#[cfg(test)]
pub(super) fn assert_floor_lines(root: &Generator, slug: &str, datum: f32) -> usize {
    let all = facades(root);
    for (c, _, m, _) in &all {
        let (rows, _) = grid_of(m);
        // A side face reads V = -y in the prim's frame.
        let v = -(datum - c[1]) + m.uv_offset.0[1];
        let off = off_grid(v * m.uv_scale.0, rows);
        assert!(
            off < 1e-3,
            "{slug}: the facade at {c:?} lays its transoms {off} of a floor off the floor line \
             at {datum} m"
        );
    }
    all.len()
}

/// Assert that every base-material facade in `root` ends its street face on
/// a mullion (or a pier) at both edges, and - where the prim is centred on
/// the building's axis - its flank faces too (#1559). Returns how many it
/// checked.
#[cfg(test)]
pub(super) fn assert_corner_mullions(root: &Generator, slug: &str) -> usize {
    let mut checked = 0;
    for (c, size, m, base) in facades(root) {
        if !base {
            continue;
        }
        let (_, cols) = grid_of(&m);
        let mut edges = Vec::new();
        // The -Z face: U = -x.
        for sx in [-1.0_f32, 1.0] {
            edges.push(-(sx * size[0] * 0.5) + m.uv_offset.0[0]);
        }
        if c[0].abs() < 1e-4 && c[2].abs() < 1e-4 {
            // The +X face reads U = -z, the -X face U = z: on the axis, both
            // share the -Z face's offset.
            for sz in [-1.0_f32, 1.0] {
                edges.push(-(sz * size[2] * 0.5) + m.uv_offset.0[0]);
            }
        }
        for u in edges {
            let off = off_grid(u * m.uv_scale.0, cols);
            assert!(
                off < 1e-3,
                "{slug}: the facade at {c:?} (size {size:?}) cuts a panel {off} of its width \
                 short at a corner"
            );
        }
        checked += 1;
    }
    checked
}

/// A gridded facade's floor-to-floor height (m): a tile holds `rows`
/// floors.
#[cfg(test)]
pub(super) fn floor_of(m: &SovereignMaterialSettings) -> f32 {
    let (rows, _) = grid_of(m);
    1.0 / (m.uv_scale.0 * rows as f32)
}
