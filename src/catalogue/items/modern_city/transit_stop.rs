//! Transit stop — a Modern-City secondary. A raised concrete platform under
//! a frosted-glass canopy, walled on the rear and one side by clear glazed
//! screens: the light-rail / bus interchange that anchors the street grid.
//!
//! The screens follow the `Window`-card idiom established by
//! [`corner_store`](super::corner_store): the glazing is a
//! [`window_card`] on a flat [`plane`], not a `Window` texture wrapped
//! around a solid slab. Wrapped on a cuboid the card tiles once per metre
//! (every prim is metre-UV since #936) and reads as a solid teal wall with
//! a postage-stamp grid in one corner; on a `Plane` with `UvMapping::Fit`
//! it fills the opening once and the panes are cut clear so the street
//! shows through the shelter (#941).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, plane, prim, quat_mul, quat_x,
    quat_y, solid, window_card,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, LAMP_WARM, SIGNAL_GREEN, concrete, enamel, steel};

/// Anthracite aluminium — the posts, glazing frames, and window mullions.
/// The standard RAL 7016 grey of modern street furniture; dark so the cut
/// panes read as a crisp glazed grid rather than a bright frame.
const MULLION: [f32; 3] = [0.24, 0.26, 0.29];
/// Frosted canopy glass — a light tone so the opaque roof panes stay a pale
/// blue-grey glass rather than being multiplied dark by their frame colour.
const CANOPY_GLASS: [f32; 3] = [0.76, 0.80, 0.84];
/// Transit livery blue — the glossy painted fascia, the one saturated accent
/// that lifts the shelter out of an all-grey steel-and-concrete read.
const TRANSIT_BLUE: [f32; 3] = [0.09, 0.34, 0.66];
/// Amber-varnished bench slats — a warm counter to the cool blue and glass.
const BENCH_WOOD: [f32; 3] = [0.72, 0.40, 0.14];
/// Hazard-yellow tactile strip along the platform edge — the safety marking
/// every real transit platform carries, and a second spot of colour.
const SAFETY_YELLOW: [f32; 3] = [0.92, 0.74, 0.12];

/// A clear glazed screen — a `window_card` with the panes cut open (opacity
/// below the `0.5` alpha-mask cutoff) so you see the street through the
/// shelter, on the shelter's anthracite frame. `panes` is `(across, up)`.
fn screen(panes: (u32, u32)) -> SovereignMaterialSettings {
    window_card(MULLION, panes.0, panes.1, 0.3, 0.03)
}

pub struct TransitStop;

impl CatalogueEntry for TransitStop {
    fn slug(&self) -> &'static str {
        "transit_stop"
    }
    fn name(&self) -> &'static str {
        "Transit Stop"
    }
    fn description(&self) -> &'static str {
        "Raised platform under a glass canopy with benches and a lit sign pylon."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let plat_h = 0.6;
    let post_h = 3.0;
    let canopy_y = plat_h + post_h;
    // Posts stand at ±3.6 (X) × ±1.1 (Z); the glazing frames tie to them.
    let post_x = 3.6;
    let post_z = 1.1;
    // The glazing sits in a frame between a kick rail and a head rail, so the
    // glass reads as held, not floating.
    let sill_y = plat_h + 0.2;
    let head_y = canopy_y - 0.3;
    let glass_cy = (sill_y + head_y) * 0.5;
    let glass_h = head_y - sill_y;

    let mut prims = vec![
        // Raised concrete platform — the root.
        prim(
            solid(cuboid_tapered(
                [9.0, plat_h, 3.2],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, plat_h * 0.5, 0.0],
            id_quat(),
        ),
        // Hazard-yellow tactile strip along the −Z platform edge, sunk a
        // little into the deck so its underside never goes coplanar with the
        // platform top.
        prim(
            solid(cuboid_tapered(
                [9.0, 0.05, 0.35],
                0.0,
                enamel(SAFETY_YELLOW),
            )),
            [0.0, plat_h + 0.005, -1.35],
            id_quat(),
        ),
    ];

    // Canopy posts.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.22, post_h, 0.22], 0.0, steel(MULLION))),
                [sx * post_x, plat_h + post_h * 0.5, sz * post_z],
                id_quat(),
            ));
        }
    }

    // --- Frosted-glass canopy: a painted perimeter frame with a glass infill.

    // Front and back fascia beams, in the transit-blue livery. The front one
    // is deeper — it carries the route sign — and both cap the glass edges.
    prims.push(prim(
        solid(cuboid_tapered([8.6, 0.5, 0.22], 0.0, enamel(TRANSIT_BLUE))),
        [0.0, canopy_y - 0.05, -1.65],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([8.6, 0.3, 0.22], 0.0, enamel(TRANSIT_BLUE))),
        [0.0, canopy_y + 0.05, 1.65],
        id_quat(),
    ));
    // Side rails, butted between the fascias (shorter in Z) so no coplanar
    // face is shared at the corners — steel butt joints, no z-fight.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.22, 0.24, 3.1], 0.0, steel(MULLION))),
            [sx * 4.19, canopy_y + 0.05, 0.0],
            id_quat(),
        ));
    }
    // Glass infill — a frosted panel filling the frame opening, laid flat
    // (normal +Y) and double-sided so it reads from the street below too.
    // Opacity above the mask cutoff, so the panes stay a solid pale glass.
    prims.push(prim(
        plane([8.1, 3.1], window_card(CANOPY_GLASS, 6, 2, 0.72, 0.04)),
        [0.0, canopy_y + 0.06, 0.0],
        id_quat(),
    ));
    // Lit route sign on the front fascia, on the −Z render front.
    prims.push(prim(
        cuboid_tapered([3.2, 0.42, 0.1], 0.0, glow(LAMP_WARM, 1.6)),
        [0.0, canopy_y - 0.05, -1.77],
        id_quat(),
    ));

    // --- Glazed screens, each held in a steel frame between the posts, so
    // the shelter is enclosed behind and to one side but open on −Z.

    // Rear screen (+Z): a kick rail and a head rail span between the back
    // posts; the clear glass sits just outboard of them, five panes across.
    let rear_w = 2.0 * post_x - 0.22;
    let rear_z = post_z + 0.06;
    for y in [sill_y, head_y] {
        prims.push(prim(
            solid(cuboid_tapered([rear_w, 0.1, 0.1], 0.0, steel(MULLION))),
            [0.0, y, post_z],
            id_quat(),
        ));
    }
    prims.push(prim(
        plane([rear_w, glass_h], screen((5, 1))),
        [0.0, glass_cy, rear_z],
        quat_x(-FRAC_PI_2),
    ));

    // Left side screen (−X): the same kick/head rails between the −X posts,
    // with the glass facing −X (the combined rotation stands the quad up as
    // the rear, then yaws it a quarter-turn so `panes` reads across-depth).
    let side_d = 2.0 * post_z - 0.22;
    let side_x = -post_x - 0.06;
    for y in [sill_y, head_y] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.1, side_d], 0.0, steel(MULLION))),
            [-post_x, y, 0.0],
            id_quat(),
        ));
    }
    prims.push(prim(
        plane([side_d, glass_h], screen((2, 1))),
        [side_x, glass_cy, 0.0],
        quat_mul(quat_y(FRAC_PI_2), quat_x(-FRAC_PI_2)),
    ));

    // Two benches against the rear screen, seats facing the open −Z front,
    // each carried on two steel legs so it stands on the platform.
    let seat_y = plat_h + 0.5;
    for sx in [-1.0_f32, 1.0] {
        let cx = sx * 1.8;
        // Warm timber-look slat seat and back.
        prims.push(prim(
            solid(cuboid_tapered([2.4, 0.12, 0.6], 0.0, enamel(BENCH_WOOD))),
            [cx, seat_y, 0.7],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([2.4, 0.5, 0.12], 0.0, enamel(BENCH_WOOD))),
            [cx, plat_h + 0.75, 1.05],
            id_quat(),
        ));
        // Steel legs at each end, from the deck up to the seat underside.
        for lx in [cx - 1.0, cx + 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.12, 0.44, 0.5], 0.0, steel(MULLION))),
                [lx, plat_h + 0.22, 0.72],
                id_quat(),
            ));
        }
    }

    // A waste bin at the open end.
    prims.push(prim(
        solid(cylinder_tapered(
            0.32,
            1.0,
            12,
            0.05,
            steel([0.35, 0.37, 0.4]),
        )),
        [-3.4, plat_h + 0.5, -0.6],
        id_quat(),
    ));

    // Lit sign pylon — a standalone roadside totem, set well clear of the
    // canopy (which reaches x ≈ 4.3) and standing on the ground beside the
    // raised platform rather than intersecting the roof.
    prims.push(prim(
        solid(cuboid_tapered([0.3, 4.4, 0.3], 0.0, steel(MULLION))),
        [5.3, 2.2, -1.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.4, 0.9, 0.12], 0.0, glow(SIGNAL_GREEN, 2.2)),
        [5.3, 3.7, -1.0],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TransitStop.build(""), "transit_stop");
    }

    /// #941: every `Window` card sits on a `Plane` at `uv_scale` 1.0. On any
    /// other prim the metre-UV projection tiles the card; a non-1.0 scale
    /// smears its clamp-to-edge texels. Guards the fix from regressing.
    #[test]
    fn glazing_cards_are_unscaled_planes() {
        use crate::pds::material_finish::node_materials_mut;

        fn walk(g: &mut Generator) {
            let tag = g.kind.kind_tag();
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in node_materials_mut(&mut g.kind) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(is_plane, "Window card must sit on a Plane, found {tag}");
                    assert_eq!(
                        m.uv_scale.0, 1.0,
                        "Window cards upload clamp-to-edge; uv_scale must stay 1.0"
                    );
                }
            }
            for c in &mut g.children {
                walk(c);
            }
        }
        walk(&mut TransitStop.build(""));
    }
}
