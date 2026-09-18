//! The sloop: a gaff-rigged sailing boat, and the boat family's universal
//! floor.
//!
//! Every dimension here is a fraction of the seeded hull, read off
//! [`HullProfile`], so a 1.7 m seed and a 4.2 m one are the same boat at two
//! sizes rather than two different mistakes. The two exceptions are the ones
//! that *cannot* scale: the sanitiser's dimension floor ([`super::MIN_DIM`])
//! and the
//! gateway air-draft cap ([`AIR_DRAFT_CAP`]), which is an absolute height
//! above the ground and so makes a big boat's rig relatively shorter.
//!
//! The shape was agreed by render before any of this was written (#1359 rules
//! 1, 12 and 14): the prototype is `target/dump/vehicles2026-09/sloop2.py`,
//! judged at the chase camera's range on `render --play-view` and then on
//! zoomed sheets.
//!
//! # Why a gaff rig
//!
//! A bermudan sloop's air draft is about 1.5 times her length. At 2.8 m that
//! is 4.2 m, and the lowest seeded gateway lintel is 2.86 m. A gaff rig puts
//! the same sail area under a short mast by hanging it from a spar that peaks
//! aft, which is the only way this boat fits through a gate. She carries no
//! backstay for the same reason a real gaff boat does not: the peak swings
//! through where one would be.

use std::f32::consts::PI;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;
use crate::seeded_defaults::{BoatBlueprint, ParticleAura};

use super::super::common::{cuboid, id_quat, prim, spine, with_cut, with_shape};
use super::profile::{HullProfile, Station};
use super::{
    AIR_DRAFT_CAP, AIR_DRAFT_MARGIN, BoatColours, BoatCraft, BoatFeel, boat_colours, dim, hover,
};

/// Section depth per unit half-beam. The knob that turns the plan form below
/// into a hull: 1.10 puts the load waterline at about 0.73 of the overall
/// length, which is a sailing boat's. Lower and she is a shallow dish whose
/// ends lift clear of the water; higher and she is a deep narrow canoe.
const SECTION: f32 = 1.10;

/// The sloop's plan form: `(z fraction of LOA, half-beam fraction)` from
/// transom to stem. Ten stations, well inside the sanitiser's sixteen.
/// Maximum beam just abaft midships, a full run aft to a real transom, and a
/// bow that comes to a point over the last tenth.
const PLAN: &[(f32, f32)] = &[
    (-0.500, 0.78),
    (-0.420, 0.88),
    (-0.300, 0.96),
    (-0.150, 1.00),
    (0.000, 0.99),
    (0.150, 0.93),
    (0.280, 0.80),
    (0.380, 0.58),
    (0.450, 0.34),
    (0.500, 0.06),
];

/// Where the topsides stop and the antifouling starts, as a fraction round the
/// swept section (`path_cut` 0.5 is one deck edge, 0.75 the keel, 1.0 the
/// other). Chosen so the line lands ON the waterline amidships and creeps a
/// little above it at the quarters, which is what a boot-top does.
const BOOT_F: f32 = 0.62;

/// The same, as an angle down from the deck edge - what the boot stripe and
/// the antifoul edge are both placed by, so they cannot disagree.
fn boot_angle() -> f32 {
    (BOOT_F - 0.5) / 0.5 * PI
}

/// How far round the section the cove line sits, as an angle down from the
/// deck edge - a hand's width under the sheer, where a scribed cove goes.
const COVE_ANGLE: f32 = 0.22;

/// Masthead height as a fraction of the overall length, before the air-draft
/// cap has its say. 0.84 is what the agreed prototype carries at the nominal
/// 2.8 m, and at that size the cap lands on the same number.
const TRUCK_PER_LOA: f32 = 0.84;

/// Deck camber, cabin crown and side-deck crown, as section-depth factors on
/// their own sweeps.
const DECK_CROWN: f32 = 0.14;
const TRUNK_CROWN: f32 = 0.55;
const SIDE_DECK_CROWN: f32 = 0.26;

/// The cockpit's ends, as z fractions of the overall length. The deck is
/// broken here rather than drawn over: a sweep cannot be holed, so a sunk sole
/// under an unbroken deck is simply invisible.
const COCKPIT: (f32, f32) = (-0.421, -0.121);

/// The height of this hull's masthead above her design waterline (m) - what
/// the air-draft cap is checked against, and the one number on the boat that
/// is resolved against an absolute limit rather than her own size.
#[cfg(test)]
pub(super) fn masthead(hull: &HullProfile) -> f32 {
    Rig::new(hull).truck
}

pub(super) struct Sloop;

impl BoatCraft for Sloop {
    fn profile(&self, bp: &BoatBlueprint) -> HullProfile {
        HullProfile::new(bp, SECTION, PLAN)
    }

    fn build(&self, ctx: &PartCtx, hull: &HullProfile) -> Generator {
        let c = boat_colours(ctx);
        let rig = Rig::new(hull);
        // Root: a hidden hub inside the canoe body. The hull cannot BE the
        // structural root - an elliptical section needs a per-axis node scale
        // and a structural root may not carry one (#798, the root-scale
        // discipline) - so the tree hangs off a cube small enough to be a
        // pixel, amidships where the hull is deepest, which is where the
        // legacy `boat_root` box's poking-out-at-the-ends bug becomes
        // impossible rather than merely fixed.
        let hub = dim(hull.loa * 0.007);
        let mut root = prim(
            cuboid([hub; 3], c.timber.clone()),
            [0.0, -hull.freeboard * 0.2, 0.0],
            id_quat(),
        );
        let kids = &mut root.children;
        skin(kids, hull, &c);
        underbody(kids, hull, &c);
        deck(kids, hull, &c);
        deck_furniture(kids, hull, &c);
        rig.build(kids, hull, &c);
        root
    }

    fn feel(&self) -> BoatFeel {
        // The retired monohull's numbers exactly. The hull *arrangements*
        // carried the feel before #1363 and a sloop is what the monohull was;
        // keeping them unchanged means the drive the owner validated with the
        // scale bridge (#1361) is the drive that ships. Per-type feel is
        // #1381's whole slice.
        BoatFeel {
            mass_factor: 4.0,
            drive_accel: 9.0,
            turn_accel: 7.0,
            linear_damping: 1.5,
            angular_damping: 6.0,
        }
    }

    fn fx_mount(&self, aura: ParticleAura, hull: &HullProfile) -> [f32; 3] {
        match aura {
            // A sloop has no funnel, so steam and wake both leave at the
            // transom, low, where a hovering hull's spray would.
            ParticleAura::Steam | ParticleAura::Wake => {
                // The after end of the wetted length, not the transom: on a
                // hull with this much rocker the transom can be clear of the
                // water, and a wake hung there would trail from thin air. Just
                // under the surface, because that is where spray leaves.
                [0.0, -hull.draft * 0.12, hull.waterline().0]
            }
            // Anything else is a flourish and belongs over the deck, amidships
            // where the boat is widest and it cannot hang over the side.
            _ => {
                let [x, y, z] = hull.cabin();
                [x, y + hull.freeboard * 0.6, z]
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Hull
// ---------------------------------------------------------------------------

/// A swept shape whose section is squashed by its own node scale - a hull, a
/// deck, a cabin crown, a foil.
///
/// **The path's y is pre-divided by that scale here, and that is the trap this
/// helper exists to close.** A node scale moves its path as well as its
/// profile, so a sheer written straight into the path is drawn squashed by
/// exactly the factor that makes the hull a hull. Both prototypes fell into
/// it: the first probe's sheer was flattened by 0.62 without anybody noticing,
/// and the agreed prototype's lead shoe was drawn at half the depth it was
/// written at, floating in the middle of the keel fin instead of shoeing its
/// foot. Callers pass TRUE metres and get true metres.
///
/// `cut` is the swept profile's kept fraction: `[0.5, 1.0]` is the lower half
/// (and its flat cut face is a deck following the sheer), `[0.0, 0.5]` the
/// upper half (a crown), `[0.0, 1.0]` the whole tube.
fn sweep(
    points: &[([f32; 3], f32)],
    resolution: u32,
    scale: [f32; 3],
    cut: [f32; 2],
    material: SovereignMaterialSettings,
) -> Generator {
    let path: Vec<([f32; 3], f32)> = points
        .iter()
        .map(|&([x, y, z], r)| ([x / scale[0], y / scale[1], z / scale[2]], dim(r)))
        .collect();
    let mut node = prim(
        with_cut(spine(&path, resolution, material), cut, [0.0, 1.0], 0.0),
        [0.0, 0.0, 0.0],
        id_quat(),
    );
    node.transform.scale = Fp3(scale);
    node
}

/// A thin spine run through the SAME stations as the hull - the airship's gore
/// idiom, and the reason nothing on this boat can float. `place` reads a
/// station and answers where the line sits on the skin; it is seated with its
/// centre ON that surface rather than tangent to it, because a coplanar tube
/// stipples against what it lies on.
fn trim_line(
    hull: &HullProfile,
    radius: f32,
    material: SovereignMaterialSettings,
    place: impl Fn(&Station, f32) -> [f32; 3],
    keep: impl Fn(&Station) -> bool,
) -> Vec<Generator> {
    let mut out = Vec::with_capacity(2);
    for side in [-1.0f32, 1.0] {
        let pts: Vec<([f32; 3], f32)> = hull
            .stations()
            .iter()
            .filter(|s| keep(s))
            .map(|s| (place(s, side), dim(radius)))
            .collect();
        if pts.len() >= 2 {
            out.push(prim(spine(&pts, 8, material.clone()), [0.0; 3], id_quat()));
        }
    }
    out
}

/// Topsides, antifouled underbody, and the three lines that run along them.
fn skin(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
    let st = hull.stations();
    let path = |grow: f32| -> Vec<([f32; 3], f32)> {
        st.iter()
            .map(|s| ([0.0, s.sheer, s.z], s.half_beam * grow))
            .collect()
    };
    kids.push(sweep(
        &path(1.0),
        24,
        [1.0, hull.section, 1.0],
        [0.5, 1.0],
        c.topsides.clone(),
    ));
    // The underbody is a second sweep of the SAME stations, a hair proud so it
    // cannot z-fight the hull it lies on, cut to a band about the keel.
    kids.push(sweep(
        &path(1.008),
        24,
        [1.0, hull.section, 1.0],
        [BOOT_F, 1.0 - (BOOT_F - 0.5)],
        c.antifoul.clone(),
    ));
    let (cos, sin) = (boot_angle().cos(), boot_angle().sin());
    let section = hull.section;
    // Boot stripe, on the paint/antifoul join - computed from the same angle
    // the cut is, so it lands on the line rather than near it.
    kids.extend(trim_line(
        hull,
        hull.loa * 0.0057,
        c.boot.clone(),
        move |s, side| {
            [
                side * s.half_beam * cos,
                s.sheer - s.half_beam * section * sin,
                s.z,
            ]
        },
        |_| true,
    ));
    // The COVE LINE: the boot stripe's own colour a hand under the deck edge.
    //
    // It is here because of where the chase camera stands (#1365). The boot
    // top is at the waterline, and at 22.9 degrees of down-angle the topsides
    // roll away under the deck edge, so the one identity line the brief names
    // first is the one line the player almost never sees. A cove scribed just
    // under the sheer is the traditional answer to the same problem and it is
    // in frame from above, so the seed's colour is findable on a hull at play
    // distance rather than only on a sheet shot from the waterline.
    kids.extend(trim_line(
        hull,
        hull.loa * 0.0060,
        c.boot.clone(),
        move |s, side| {
            [
                side * s.half_beam * COVE_ANGLE.cos(),
                s.sheer - s.half_beam * section * COVE_ANGLE.sin(),
                s.z,
            ]
        },
        |_| true,
    ));
    // Rub rail on the deck edge, toe rail just inboard and standing proud of
    // the deck - the two lines that give a hull her sheer at play distance.
    kids.extend(trim_line(
        hull,
        hull.loa * 0.0071,
        c.timber.clone(),
        |s, side| [side * s.half_beam * 0.995, s.sheer, s.z],
        |_| true,
    ));
    let inset = hull.half_beam * 0.125;
    let rise = hull.freeboard * 0.057;
    kids.extend(trim_line(
        hull,
        hull.loa * 0.0046,
        c.brightwork.clone(),
        move |s, side| [side * (s.half_beam - inset).max(0.01), s.sheer + rise, s.z],
        move |s| s.half_beam >= inset * 2.0,
    ));
}

/// Keel fin, lead shoe, rudder and tiller. On show, because she hovers: a
/// boat's underbody is normally the half nobody sees, and this one spends its
/// whole life a quarter of a draft above the ground.
fn underbody(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
    let (loa, draft) = (hull.loa, hull.draft);
    // The fin's top follows the canoe bottom read off the profile and its
    // bottom is the design draft, so it is deepest FORWARD, where the canoe is
    // shallowest - which is what a constant draft line means.
    let fin: Vec<([f32; 3], f32)> = [
        (0.1857, -0.464),
        (0.1071, -0.929),
        (0.0179, -1.000),
        (-0.0893, -0.964),
        (-0.1571, -0.679),
    ]
    .iter()
    .map(|&(zf, bf)| {
        let z = zf * loa;
        let top = hull.keel_at(z) + loa * 0.0043;
        let bottom = bf * draft;
        (
            [0.0, (top + bottom) * 0.5, z],
            dim((top - bottom) * 0.5).max(0.02),
        )
    })
    .collect();
    kids.push(sweep(
        &fin,
        10,
        [0.16, 1.0, 1.0],
        [0.0, 1.0],
        c.antifoul.clone(),
    ));
    // A lead shoe along the fin's bottom. Without it the fin is the same
    // antifoul red as the hull bottom it grows out of and does not read as a
    // keel at all - the first render of this hull had no keel in it.
    let shoe: Vec<([f32; 3], f32)> = [
        (0.1286, -0.839),
        (0.0357, -0.957),
        (-0.0714, -0.936),
        (-0.1429, -0.732),
    ]
    .iter()
    .map(|&(zf, yf)| ([0.0, yf * draft + loa * 0.007, zf * loa], dim(loa * 0.0114)))
    .collect();
    kids.push(sweep(
        &shoe,
        10,
        [0.30, 0.55, 1.0],
        [0.0, 1.0],
        c.lead.clone(),
    ));
    // Rudder hung on the transom, and a tiller over it into the cockpit.
    let [_, head, transom] = hull.stern_fitting();
    let rudder = [
        ([0.0, head - loa * 0.018, transom], dim(loa * 0.0179)),
        (
            [0.0, draft * 0.143, transom - loa * 0.011],
            dim(loa * 0.0279),
        ),
        (
            [0.0, -draft * 0.893, transom - loa * 0.021],
            dim(loa * 0.0161),
        ),
    ];
    kids.push(sweep(
        &rudder,
        10,
        [0.17, 1.0, 1.0],
        [0.0, 1.0],
        c.antifoul.clone(),
    ));
    let tiller = [
        ([0.0, head + loa * 0.014, transom], dim(loa * 0.0068)),
        (
            [0.0, head + loa * 0.036, transom + loa * 0.171],
            dim(loa * 0.005),
        ),
    ];
    kids.push(prim(
        spine(&tiller, 8, c.brightwork.clone()),
        [0.0; 3],
        id_quat(),
    ));
}

// ---------------------------------------------------------------------------
// Deck
// ---------------------------------------------------------------------------

/// A laid deck, broken at the cockpit, with side decks down each side of the
/// well.
///
/// Its own sweep rather than the topsides' cut face, and that is the whole
/// reason it exists: left as the cut face the deck is one flat plane in the
/// hull's own paint, and at the chase camera's range the boat reads as a bar
/// of soap.
fn deck(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
    let drop = hull.freeboard * 0.04;
    let run = |from: f32, to: f32| -> Generator {
        let mut zs = vec![from];
        zs.extend(
            hull.stations()
                .iter()
                .map(|s| s.z)
                .filter(|&z| from < z && z < to),
        );
        zs.push(to);
        let pts: Vec<([f32; 3], f32)> = zs
            .iter()
            .map(|&z| {
                (
                    [0.0, hull.sheer_z(z) - drop, z],
                    hull.half_beam_at(z) * 0.985,
                )
            })
            .collect();
        sweep(
            &pts,
            18,
            [1.0, DECK_CROWN, 1.0],
            [0.0, 0.5],
            c.timber.clone(),
        )
    };
    kids.push(run(COCKPIT.1 * hull.loa, hull.stem_z()));
    kids.push(run(hull.transom_z(), COCKPIT.0 * hull.loa));
    let side_r = hull.half_beam * 0.18;
    for side in [-1.0f32, 1.0] {
        let pts: Vec<([f32; 3], f32)> = [COCKPIT.0, -0.339, -0.236, COCKPIT.1]
            .iter()
            .map(|&zf| {
                let z = zf * hull.loa;
                (
                    [
                        side * (hull.half_beam_at(z) * 0.985 - side_r),
                        hull.sheer_z(z) - drop,
                        z,
                    ],
                    dim(side_r),
                )
            })
            .collect();
        kids.push(sweep(
            &pts,
            10,
            [1.0, SIDE_DECK_CROWN, 1.0],
            [0.0, 1.0],
            c.timber.clone(),
        ));
    }
}

/// The cabin trunk's own stations: `(z fraction of LOA, half-beam fraction)`,
/// forward-going, the same idiom as [`PLAN`].
const TRUNK: &[(f32, f32)] = &[
    (-0.120, 0.60),
    (-0.020, 0.64),
    (0.080, 0.64),
    (0.160, 0.58),
    (0.215, 0.36),
];

/// Where the three lit ports sit ALONG the trunk, as a parameter in the
/// sweep's own control-point units (`0` is the after station, `1` the next,
/// and so on) - see [`trunk_ports`].
const PORTS: [f32; 3] = [0.45, 1.30, 2.15];

/// The trunk's swept path in true metres, read off [`TRUNK`].
fn trunk_path(hull: &HullProfile) -> Vec<([f32; 3], f32)> {
    TRUNK
        .iter()
        .map(|&(zf, rf)| {
            (
                [0.0, hull.sheer_at(zf) + hull.loa * 0.002, zf * hull.loa],
                dim(rf * hull.half_beam),
            )
        })
        .collect()
}

/// The three port seats, each `(centreline point, trunk radius there)`, taken
/// from the trunk's **drawn** centreline rather than from a fraction restated
/// beside it.
///
/// This is the rule the whole family follows - every line is read off the
/// shape it lies on - and the ports were the one place that broke it: each
/// carried its own half-beam fraction and its own `sheer_at`, both LINEAR
/// reads of a curve the mesher splines. On the agreed PROTOTYPE that cost the
/// two forward ports up to 25 mm and left the boat in four pieces; the ported
/// fractions happened to land close enough that the built sloop still met her
/// cabin, which is luck rather than a rule, and
/// `tests::a_boat_is_one_machine_at_her_blueprint_extremes` is what turned
/// that from a belief into a measurement (#1366). Sampling the same
/// Catmull-Rom `sweeps.rs` draws cannot drift, because there is nothing left
/// to restate.
fn trunk_ports(path: &[([f32; 3], f32)]) -> Vec<([f32; 3], f32)> {
    use bevy::math::cubic_splines::{CubicCardinalSpline, CubicGenerator};
    use bevy::math::{Vec3, Vec4};

    let ctrl: Vec<Vec4> = path
        .iter()
        .map(|&([x, y, z], r)| Vec3::new(x, y, z).extend(r))
        .collect();
    let Ok(curve) = CubicCardinalSpline::new_catmull_rom(ctrl).to_curve() else {
        // Fewer than two stations is not a trunk; there is nothing to hang a
        // port on and nothing to interpolate.
        return Vec::new();
    };
    PORTS
        .iter()
        .map(|&at| {
            let v = curve.position(at);
            ([v.x, v.y, v.z], v.w)
        })
        .collect()
}

/// Cabin trunk with its lit port band, companionway, foredeck hatch, and the
/// cockpit well with its coaming.
fn deck_furniture(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
    let loa = hull.loa;
    // Trunk: an UPPER half-pipe over its own stations, so its flat underside
    // sits on the deck and its crown is swept rather than a box lid.
    let trunk = trunk_path(hull);
    kids.push(sweep(
        &trunk,
        20,
        [1.0, TRUNK_CROWN, 1.0],
        [0.0, 0.5],
        c.topsides.clone(),
    ));
    // The lit port band. NO glass volume: `SovereignMaterialSettings` has no
    // alpha, so one would render as a dark crate (#1359 rule 4). Each port is
    // placed by a parameter ALONG the trunk's own drawn centreline, so its
    // height, its station and the radius it is pressed into all come from the
    // one sweep - the rule the rest of the family follows.
    let h = loa * 0.0164;
    for ([_, y, z], r) in trunk_ports(&trunk) {
        // The half-width of the crowned section at the port's own height.
        // Its centre sits ON that surface, the same seat every trim line on
        // this boat takes, so half the port is let into the coachroof.
        let x = r * (1.0 - (h / (r * TRUNK_CROWN)).powi(2)).max(0.04).sqrt();
        for side in [-1.0f32, 1.0] {
            kids.push(prim(
                cuboid(
                    [dim(loa * 0.005), dim(loa * 0.0214), dim(loa * 0.0375)],
                    c.window.clone(),
                ),
                [side * x, y + h, z],
                id_quat(),
            ));
        }
    }
    // Companionway in the trunk's after face, so the cockpit leads somewhere.
    // The inside of the boat, in her interior shadow rather than her trim -
    // a companionway painted the boot stripe's colour spends the seed's
    // identity on a hatchway nobody sees at play distance.
    kids.push(prim(
        cuboid(
            [dim(loa * 0.0857), dim(loa * 0.05), dim(loa * 0.0071)],
            c.interior.clone(),
        ),
        [
            0.0,
            hull.sheer_at(-0.115) + loa * 0.0271,
            -0.115 * loa - loa * 0.0043,
        ],
        id_quat(),
    ));
    // Foredeck hatch: the fore deck is the largest flat on the boat and reads
    // as a blank without one.
    let hatch_z = 0.307 * loa;
    kids.push(prim(
        cuboid(
            [dim(loa * 0.0929), dim(loa * 0.015), dim(loa * 0.1071)],
            c.brightwork.clone(),
        ),
        [0.0, hull.sheer_z(hatch_z) + loa * 0.0107, hatch_z],
        id_quat(),
    ));
    // The cockpit: a solid well plugging the gap in the deck, and a coaming
    // round its edge. Solid rather than a thin sole, because through a bare
    // gap you see the inside of the topsides shell and a pale hole does not
    // read as a cockpit.
    let [_, _, well_z] = hull.cockpit();
    let well_top = hull.sheer_z(well_z) - loa * 0.0057;
    let well_h = loa * 0.1214;
    kids.push(prim(
        cuboid(
            [dim(hull.half_beam * 1.55), dim(well_h), dim(loa * 0.3143)],
            c.interior.clone(),
        ),
        [0.0, well_top - well_h * 0.5, well_z],
        id_quat(),
    ));
    let inset = hull.half_beam * 0.35;
    let coam: Vec<[f32; 3]> = [COCKPIT.1 - 0.007, -0.236, -0.339, COCKPIT.0 + 0.007]
        .iter()
        .map(|&zf| {
            let z = zf * loa;
            [
                hull.half_beam_at(z) * 0.985 - inset,
                hull.sheer_z(z) + loa * 0.0079,
                z,
            ]
        })
        .collect();
    let r = dim(loa * 0.0075);
    let mut ring: Vec<([f32; 3], f32)> = coam.iter().map(|p| ([-p[0], p[1], p[2]], r)).collect();
    ring.extend(coam.iter().rev().map(|p| (*p, r)));
    kids.push(prim(
        spine(&ring, 8, c.brightwork.clone()),
        [0.0; 3],
        id_quat(),
    ));
}

// ---------------------------------------------------------------------------
// Rig
// ---------------------------------------------------------------------------

/// The gaff rig's heights, all measured from the design waterline and all
/// resolved against the air-draft cap before anything is drawn.
struct Rig {
    /// Deck height at the mast step, and the mast station.
    heel: f32,
    mast_z: f32,
    /// Masthead.
    truck: f32,
    /// Where the gaff meets the mast, where the peak ends up, and where the
    /// forestay and shrouds land.
    throat: f32,
    peak_y: f32,
    peak_z: f32,
    hounds: f32,
    /// Boom: its forward end above the waterline and its after end.
    gooseneck: f32,
    boom_aft: f32,
    /// Bowsprit end.
    sprit_z: f32,
}

impl Rig {
    fn new(hull: &HullProfile) -> Self {
        let [_, heel, mast_z] = hull.mast_step();
        // The masthead is the proportional height OR what the gateway leaves,
        // whichever is less. The cap is an absolute height above the GROUND
        // and the boat floats a quarter of a draft over it, so a bigger hull
        // gets a relatively shorter rig - which is the honest consequence of
        // sailing a model boat through a 2.86 m lintel.
        let truck = (hull.loa * TRUCK_PER_LOA)
            .min(AIR_DRAFT_CAP - hover(hull.draft) - AIR_DRAFT_MARGIN)
            .max(heel + hull.loa * 0.25);
        let span = truck - heel;
        Self {
            heel,
            mast_z,
            truck,
            throat: heel + span * 0.585,
            peak_y: heel + span * 0.936,
            peak_z: -0.143 * hull.loa,
            hounds: heel + span * 0.895,
            gooseneck: heel + hull.loa * 0.065,
            boom_aft: -0.386 * hull.loa,
            sprit_z: hull.stem_z() + hull.loa * 0.143,
        }
    }

    /// The mast's three stations, heel to truck, tapering as it goes.
    ///
    /// A method rather than three literals inside `build` because the spars
    /// that hang off it need to know how thick it is where they meet it, and
    /// a second copy of these numbers is exactly how a gaff ends up 1.4 mm
    /// clear of the mast it is supposed to hang from (#1366).
    fn mast(&self, loa: f32) -> [([f32; 3], f32); 3] {
        [
            (
                [0.0, self.heel - loa * 0.007, self.mast_z],
                dim(loa * 0.0107),
            ),
            ([0.0, self.throat, self.mast_z], dim(loa * 0.0086)),
            ([0.0, self.truck, self.mast_z], dim(loa * 0.0057)),
        ]
    }

    /// The mast's radius at height `y`, straight off [`Self::mast`]'s own
    /// stations. Linear between them, which is what an embed depth wants: the
    /// question is how deep to bury a jaw, not where a surface is.
    fn mast_radius_at(&self, loa: f32, y: f32) -> f32 {
        let m = self.mast(loa);
        for w in m.windows(2) {
            let (([_, y0, _], r0), ([_, y1, _], r1)) = (w[0], w[1]);
            if y <= y1 {
                let t = ((y - y0) / (y1 - y0).max(1e-6)).clamp(0.0, 1.0);
                return r0 + (r1 - r0) * t;
            }
        }
        m[2].1
    }

    fn build(&self, kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
        let loa = hull.loa;
        let line = |pts: &[([f32; 3], f32)], res, m: SovereignMaterialSettings| {
            prim(spine(pts, res, m), [0.0; 3], id_quat())
        };
        // Mast, tapering to the truck.
        kids.push(line(&self.mast(loa), 10, c.timber.clone()));
        // Boom and gaff, each standing clear of the canvas it carries - a spar
        // buried in its own sail is a spar nobody can see - but with its
        // inboard end INSIDE the mast it hangs from. A gaff's jaws embrace the
        // mast and a gooseneck is a fitting on it, so both are seated half a
        // mast radius abaft the spar's own axis, at the mast's thickness where
        // that spar meets it. Written as a constant offset the gaff cleared
        // the mast by 1.4 mm at the nominal size and the boat came apart at
        // the throat (#1366).
        let jaw = |y: f32, rise: f32| {
            [
                0.0,
                y + rise,
                self.mast_z - self.mast_radius_at(loa, y) * 0.5,
            ]
        };
        kids.push(line(
            &[
                (jaw(self.gooseneck, -loa * 0.011), dim(loa * 0.0068)),
                (
                    [0.0, self.gooseneck + loa * 0.007, self.boom_aft],
                    dim(loa * 0.0082),
                ),
            ],
            8,
            c.timber.clone(),
        ));
        kids.push(line(
            &[
                (jaw(self.throat, loa * 0.010), dim(loa * 0.0068)),
                (
                    [0.0, self.peak_y + loa * 0.009, self.peak_z],
                    dim(loa * 0.0054),
                ),
            ],
            8,
            c.timber.clone(),
        ));
        // Bowsprit: slim, and it finally has a job - it carries the forestay
        // and the jib's tack. The retired boat's was 5 cm thick on a 1.3 m
        // hull and read as a tank gun.
        let [_, stem_y, _] = hull.bow_fitting();
        let tack_y = stem_y + loa * 0.011;
        kids.push(line(
            &[
                ([0.0, stem_y - loa * 0.025, 0.42 * loa], dim(loa * 0.0071)),
                ([0.0, tack_y, self.sprit_z], dim(loa * 0.0046)),
            ],
            8,
            c.timber.clone(),
        ));
        self.sails(kids, hull, c, tack_y);
        // Standing rigging. NO backstay: the gaff peak swings through where
        // one would be, which is why a gaff boat carries runners instead.
        let masthead = [0.0, self.hounds, self.mast_z];
        let chain = |side: f32| hull.deck_edge(0.09, side * 0.93);
        for foot in [[0.0, tack_y, self.sprit_z], chain(1.0), chain(-1.0)] {
            kids.push(line(
                &[(masthead, dim(loa * 0.004)), (foot, dim(loa * 0.004))],
                5,
                c.rigging.clone(),
            ));
        }
        // Masthead burgee, flying just under the truck so the cap holds. The
        // highest thing on the boat and one of her three identity slots, so it
        // is the seeded accent (#1365).
        kids.push(prim(
            cuboid(
                [dim(loa * 0.0043), dim(loa * 0.0196), dim(loa * 0.0607)],
                c.pennant.clone(),
            ),
            [0.0, self.truck - loa * 0.0125, self.mast_z - loa * 0.0357],
            id_quat(),
        ));
    }

    /// Mainsail and jib.
    ///
    /// A gaff mainsail is TWO cuboids, and that is a mesher fact rather than a
    /// choice: a cuboid's torture can taper and shear its top edge but cannot
    /// TILT it, so a one-piece four-sided sail with a peaked head does not
    /// exist. The body carries a horizontal head at the throat and the peak
    /// panel sits on it, its forward edge running up the gaff.
    fn sails(&self, kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours, tack_y: f32) {
        let loa = hull.loa;
        let x = loa * 0.0107;
        let mut sail = |cloth: &SovereignMaterialSettings,
                        thick: f32,
                        height: f32,
                        foot: f32,
                        at: [f32; 3],
                        taper: f32,
                        shear: f32,
                        bend: f32| {
            kids.push(prim(
                with_shape(
                    cuboid([dim(thick), dim(height), dim(foot)], cloth.clone()),
                    [0.0, taper],
                    [bend, 0.0, 0.0],
                    [0.0, shear],
                ),
                at,
                id_quat(),
            ));
        };
        let foot = self.mast_z - self.boom_aft;
        let head = self.mast_z - self.peak_z - loa * 0.054;
        let taper = 1.0 - head / foot;
        let body_z = (self.mast_z + self.boom_aft) * 0.5;
        sail(
            &c.canvas,
            loa * 0.0043,
            self.throat - self.gooseneck,
            foot,
            [x, (self.throat + self.gooseneck) * 0.5, body_z],
            taper,
            foot * 0.5 * taper,
            0.09,
        );
        let head_z = body_z + foot * 0.5 * taper;
        sail(
            &c.canvas,
            loa * 0.0039,
            self.peak_y - self.throat,
            head,
            [x, (self.peak_y + self.throat) * 0.5, head_z],
            0.97,
            self.peak_z - head_z,
            0.07,
        );
        let clew_z = 0.29 * loa;
        let jib_foot = self.sprit_z - clew_z;
        let jib_z = (self.sprit_z + clew_z) * 0.5;
        // The jib is the boat's third identity slot: the one sail small enough
        // to be trim rather than mass, dyed in the seeded accent (#1365).
        sail(
            &c.jib,
            loa * 0.0039,
            self.hounds - tack_y,
            jib_foot,
            [x, (self.hounds + tack_y) * 0.5, jib_z],
            0.97,
            self.mast_z + loa * 0.018 - jib_z,
            0.10,
        );
    }
}
