//! The sloop's hull, deck and deck furniture - everything below the rig, and
//! all of it read off the [`HullProfile`].

use std::f32::consts::PI;

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;

use super::super::super::common::{cuboid, id_quat, prim, spine, with_cut};
use super::super::profile::{HullProfile, Station};
use super::super::{BoatColours, dim};

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

/// How far forward of the transom the antifoul sweep stops, as a fraction of
/// the overall length.
///
/// `path_cut` closes an opened sweep with flat radial caps, and when the
/// topsides and the antifoul both ended AT the transom their two caps lay in
/// one plane - white and red fans interleaved, which from astern at play
/// distance read as a striped umbrella (#1366 defect 2, transom.png). Stopping
/// the antifoul a hair short buries its cap inside the hull, so the transom
/// is one face in the topsides' own finish, as a painted transom is. The stem
/// is left alone: both sweeps end there on a point, and a cap with no area
/// has nothing to stripe.
const ANTIFOUL_SHORT_OF_TRANSOM: f32 = 0.004;

/// Deck camber, cabin crown and side-deck crown, as section-depth factors on
/// their own sweeps.
pub(super) const DECK_CROWN: f32 = 0.14;
pub(super) const TRUNK_CROWN: f32 = 0.55;
const SIDE_DECK_CROWN: f32 = 0.26;

/// The laid deck's radius as a fraction of the hull's at the same station -
/// just inside the sheer, so the deck edge meets the rub rail rather than
/// overhanging it.
const DECK_INSET: f32 = 0.985;

/// The laid deck is set down into the hull by the LESSER of these two: a
/// fraction of the freeboard, and a fraction of the deck's own crown at that
/// station.
///
/// It has to be set down - at zero the deck's underside lies in the
/// topsides' cut face down the whole length, which is the coplanar trap of
/// the transom again. But a CONSTANT drop is #1363's leftover item 6, which
/// was blamed on the jib and is not the jib (#1366 defect 3): the cut face is
/// a flat plane at the sheer, and wherever the deck's crown is shallower than
/// the drop - both ends, where the beam goes to nothing - the white cut face
/// shows over the brown deck as a stub at the stemhead. Capping the drop at a
/// quarter of the crown keeps the band of cut face that can show to the same
/// small fraction of the beam at every station, which the rub rail covers.
const DECK_DROP: f32 = 0.04;
const DECK_DROP_OF_CROWN: f32 = 0.25;

/// The cockpit's ends, as z fractions of the overall length. The deck is
/// broken here rather than drawn over: a sweep cannot be holed, so a sunk sole
/// under an unbroken deck is simply invisible.
pub(super) const COCKPIT: (f32, f32) = (-0.421, -0.121);

/// How far the laid deck's centreline sits under the sheer at `z` (m) - see
/// [`DECK_DROP`]. Everything that stands ON the deck reads it, so a fitting
/// cannot be seated on a deck that is not there.
pub(super) fn deck_drop(hull: &HullProfile, z: f32) -> f32 {
    let crown = DECK_CROWN * DECK_INSET * hull.half_beam_at(z);
    (hull.freeboard * DECK_DROP).min(crown * DECK_DROP_OF_CROWN)
}

/// The height of the laid deck's EDGE at `z` (m).
pub(super) fn deck_y(hull: &HullProfile, z: f32) -> f32 {
    hull.sheer_z(z) - deck_drop(hull, z)
}

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
pub(super) fn sweep(
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

/// A plain spine - a spar, a stay, a rail - in the root frame.
pub(super) fn line(
    points: &[([f32; 3], f32)],
    resolution: u32,
    material: SovereignMaterialSettings,
) -> Generator {
    prim(spine(points, resolution, material), [0.0; 3], id_quat())
}

/// A plain box at `at`.
pub(super) fn block(
    size: [f32; 3],
    material: SovereignMaterialSettings,
    at: [f32; 3],
) -> Generator {
    prim(cuboid(size.map(dim), material), at, id_quat())
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
            out.push(line(&pts, 8, material.clone()));
        }
    }
    out
}

/// Topsides, antifouled underbody, and the three lines that run along them.
pub(super) fn skin(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
    let st = hull.stations();
    let topsides: Vec<([f32; 3], f32)> = st
        .iter()
        .map(|s| ([0.0, s.sheer, s.z], s.half_beam))
        .collect();
    kids.push(sweep(
        &topsides,
        24,
        [1.0, hull.section, 1.0],
        [0.5, 1.0],
        c.topsides.clone(),
    ));
    // The underbody is a second sweep of the SAME stations, a hair proud so it
    // cannot z-fight the hull it lies on, cut to a band about the keel - and
    // stopped just short of the transom so its cap is buried rather than
    // interleaved with the topsides' (see [`ANTIFOUL_SHORT_OF_TRANSOM`]).
    const PROUD: f32 = 1.008;
    let mut underbody: Vec<([f32; 3], f32)> = st
        .iter()
        .map(|s| ([0.0, s.sheer, s.z], s.half_beam * PROUD))
        .collect();
    let aft = hull.transom_z() + hull.loa * ANTIFOUL_SHORT_OF_TRANSOM;
    underbody[0] = (
        [0.0, hull.sheer_z(aft), aft],
        hull.half_beam_at(aft) * PROUD,
    );
    kids.push(sweep(
        &underbody,
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
pub(super) fn underbody(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
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
    // Rudder hung on the stern, and a tiller over it into the cockpit. Hung
    // on the profile's own after end, so a canoe stern carries it on her
    // sternpost without being told she has one.
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
    kids.push(line(
        &[
            ([0.0, head + loa * 0.014, transom], dim(loa * 0.0068)),
            (
                [0.0, head + loa * 0.036, transom + loa * 0.171],
                dim(loa * 0.005),
            ),
        ],
        8,
        c.brightwork.clone(),
    ));
}

/// A laid deck, broken at the cockpit, with side decks down each side of the
/// well.
///
/// Its own sweep rather than the topsides' cut face, and that is the whole
/// reason it exists: left as the cut face the deck is one flat plane in the
/// hull's own paint, and at the chase camera's range the boat reads as a bar
/// of soap.
pub(super) fn deck(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
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
            .map(|&z| ([0.0, deck_y(hull, z), z], hull.half_beam_at(z) * DECK_INSET))
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
                        side * (hull.half_beam_at(z) * DECK_INSET - side_r),
                        deck_y(hull, z),
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
/// forward-going, the same idiom as the plan form.
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
pub(super) fn trunk_path(hull: &HullProfile) -> Vec<([f32; 3], f32)> {
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
/// cabin, which is luck rather than a rule (#1366). Sampling the same
/// Catmull-Rom `sweeps.rs` draws cannot drift, because there is nothing left
/// to restate - and it has to be bevy's own curve, because bevy MIRRORS a
/// Catmull-Rom's end control points (`P[-1] = 2 P[0] - P[1]`) rather than
/// duplicating them, and a hand-rolled spline that duplicated them put the
/// first port 17.6 mm off.
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
pub(super) fn deck_furniture(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
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
            kids.push(block(
                [loa * 0.005, loa * 0.0214, loa * 0.0375],
                c.window.clone(),
                [side * x, y + h, z],
            ));
        }
    }
    // Companionway in the trunk's after face, so the cockpit leads somewhere.
    // The inside of the boat, in her interior shadow rather than her trim -
    // a companionway painted the boot stripe's colour spends the seed's
    // identity on a hatchway nobody sees at play distance.
    kids.push(block(
        [loa * 0.0857, loa * 0.05, loa * 0.0071],
        c.interior.clone(),
        [
            0.0,
            hull.sheer_at(-0.115) + loa * 0.0271,
            -0.115 * loa - loa * 0.0043,
        ],
    ));
    // Foredeck hatch: the fore deck is the largest flat on the boat and reads
    // as a blank without one.
    let hatch_z = 0.307 * loa;
    kids.push(block(
        [loa * 0.0929, loa * 0.015, loa * 0.1071],
        c.brightwork.clone(),
        [0.0, hull.sheer_z(hatch_z) + loa * 0.0107, hatch_z],
    ));
    cockpit(kids, hull, c);
}

/// The cockpit: a solid well plugging the gap in the deck, and a coaming round
/// its edge.
///
/// Solid rather than a thin sole, because through a bare gap you see the
/// inside of the topsides shell and a pale hole does not read as a cockpit.
/// And SWEPT through the cockpit's own stations at a radius read off the hull
/// there (#1366 defect 1): as a cuboid on the MAXIMUM half-beam it fitted the
/// hull it was drawn on with room to spare, so nothing ever showed it, but a
/// canoe stern closes her run aft and the box's after end walked straight out
/// through the planking as a grey slab under the counter. Read off the
/// profile, the well narrows with whatever stern she has.
fn cockpit(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
    let loa = hull.loa;
    let [_, _, well_z] = hull.cockpit();
    let well_h = loa * 0.1214;
    let half = loa * 0.3143 * 0.5;
    // The well's width as a fraction of the hull's at each station.
    const WELL_OF_BEAM: f32 = 0.80;
    let mid = hull.half_beam_at(well_z) * WELL_OF_BEAM;
    let well: Vec<([f32; 3], f32)> = [-1.0f32, -0.45, 0.45, 1.0]
        .iter()
        .map(|&f| {
            let z = well_z + half * f;
            (
                [0.0, hull.sheer_z(z) - loa * 0.0057 - well_h * 0.5, z],
                hull.half_beam_at(z) * WELL_OF_BEAM,
            )
        })
        .collect();
    kids.push(sweep(
        &well,
        14,
        [1.0, well_h / (2.0 * mid), 1.0],
        [0.0, 1.0],
        c.interior.clone(),
    ));
    let inset = hull.half_beam * 0.35;
    let coam: Vec<[f32; 3]> = [COCKPIT.1 - 0.007, -0.236, -0.339, COCKPIT.0 + 0.007]
        .iter()
        .map(|&zf| {
            let z = zf * loa;
            [
                hull.half_beam_at(z) * DECK_INSET - inset,
                hull.sheer_z(z) + loa * 0.0079,
                z,
            ]
        })
        .collect();
    let r = dim(loa * 0.0075);
    let mut ring: Vec<([f32; 3], f32)> = coam.iter().map(|p| ([-p[0], p[1], p[2]], r)).collect();
    ring.extend(coam.iter().rev().map(|p| (*p, r)));
    kids.push(line(&ring, 8, c.brightwork.clone()));
}
