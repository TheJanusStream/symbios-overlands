//! What the rover carries on her deck: the instrument mast and its lit
//! camera head on every variant, and then the one mass that says which
//! machine she is - the surveyor's tilted solar panel and dish, the
//! carapace's chitin shell, or the monolith's upright lit slab.
//!
//! # The camera head is the fleet's one lit pane
//!
//! A boxy housing with a band of `window_material(window_light(tertiary))`
//! right ROUND it, so it reads lit from astern - which is where the chase
//! camera is - and not only from ahead. Her identity trim is the primary and
//! the pane is the tertiary: two light colours on one small machine, and at
//! 12 m they read as what they are, a lit instrument and a lit livery.
//!
//! Its top is held under rule 6's cap BY CONSTRUCTION
//! ([`RoverPlan::cap_y`]), never checked against it after the fact.

use crate::pds::avatar::livery::RoverColours;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{ParticleAura, RoverVariant};

use super::{
    MAST, NEAR, NO_TURN, RoverPlan, id_quat, line, plate, prim, quat_x, solid, superellipsoid,
    tapered_plate,
};

// ---------------------------------------------------------------------------
// The mast and the camera head
// ---------------------------------------------------------------------------

/// The mast's foot: its x over the deck's half-width on the NEAR side, and
/// its station over the length.
const MAST_AT: (f32, f32) = (0.58, 0.235);

/// The camera head's box `[x, y, z]` (of the length).
const HEAD: [f32; 3] = [0.085, 0.040, 0.060];

/// The camera head's TOP over the datum: [`MAST`] of the length over the
/// ground, or what rule 6's cap leaves - whichever is lower.
fn mast_top(plan: &RoverPlan) -> f32 {
    (MAST * plan.length - plan.datum_height()).min(plan.cap_y())
}

/// The instrument mast and its lit camera head.
///
/// The monolith's is an OBELISK instead: one tapered plate, hard-edged, with
/// only a pane for a head - the same instrument in her own vocabulary, which
/// is why she has no separate housing.
pub(super) fn mast(kids: &mut Vec<Generator>, plan: &RoverPlan, c: &RoverColours) {
    let l = plan.length;
    let (x, z) = (NEAR * plan.half_w() * MAST_AT.0, MAST_AT.1 * l);
    let [hx, hy, hz] = HEAD.map(|v| v * l);
    let yh = mast_top(plan) - hy * 0.5;
    if plan.variant == RoverVariant::Monolith {
        let h = yh - plan.top() + plan.at(0.010);
        kids.push(tapered_plate(
            [plan.at(0.058), h, plan.at(0.058)],
            &c.body,
            [x, plan.top() - plan.at(0.010) + h * 0.5, z],
            NO_TURN,
            0.06,
            [0.42, 0.42],
            [0.0; 2],
        ));
        kids.push(plate(
            [plan.at(0.070), hy * 0.62, plan.at(0.070)],
            &c.glass,
            [x, yh, z],
            NO_TURN,
            0.06,
        ));
        return;
    }
    kids.push(line(
        &[
            ([x, 0.0, z], plan.at(0.016)),
            ([x, yh * 0.5, z], plan.at(0.014)),
            ([x, yh, z], plan.at(0.012)),
        ],
        8,
        &c.body,
    ));
    kids.push(plate([hx, hy, hz], &c.body, [x, yh, z], NO_TURN, 0.18));
    kids.push(plate(
        [hx * 1.10, hy * 0.46, hz * 1.12],
        &c.glass,
        [x, yh, z],
        NO_TURN,
        0.18,
    ));
}

/// The variant's own mass, and the dead cell a battered surveyor carries.
pub(super) fn variant_mass(
    kids: &mut Vec<Generator>,
    plan: &RoverPlan,
    c: &RoverColours,
    dead_cell: Option<usize>,
) {
    match plan.variant {
        RoverVariant::Surveyor => {
            solar_panel(kids, plan, c, dead_cell);
            dish(kids, plan, c);
        }
        RoverVariant::Carapace => carapace(kids, plan, c),
        RoverVariant::Monolith => monolith(kids, plan, c),
    }
}

// ---------------------------------------------------------------------------
// The surveyor: a tilted solar panel and a dish
// ---------------------------------------------------------------------------

/// The panel's tilt (rad), its run along the machine (of the length), its
/// half-width over the deck's, how far its low aft edge stands over the
/// deck's top (of the length), and how many cell plates it carries.
const PANEL_TILT: f32 = 0.24;
const PANEL_RUN: (f32, f32) = (-0.490, -0.040);
const PANEL_W: f32 = 1.34;
const PANEL_LIFT: f32 = 0.030;
pub(super) const CELLS: usize = 3;

/// The solar panel's frame, as the five numbers every part of it reads.
///
/// A landmark struct because they travel together: the wing panel, the
/// pedestal and the aura's own mount are all placed in the panel's frame
/// rather than in the machine's.
#[derive(Clone, Copy, Debug)]
pub(super) struct Panel {
    /// The frame's centre (m, root-local).
    at: [f32; 3],
    /// Its face turned up and AFT (rad).
    tilt: f32,
    /// Half its width athwart the machine (m).
    pub(super) half_w: f32,
    /// Its length along its own slope (m).
    pub(super) run: f32,
    /// The frame's thickness, and each cell plate's (m).
    pub(super) thick: f32,
}

impl Panel {
    pub(super) fn of(plan: &RoverPlan) -> Self {
        let l = plan.length;
        let (z0, z1) = (PANEL_RUN.0 * l, PANEL_RUN.1 * l);
        let run = (z1 - z0) / PANEL_TILT.cos();
        Self {
            at: [
                0.0,
                plan.top() + PANEL_LIFT * l + PANEL_TILT.sin() * run * 0.5,
                (z0 + z1) * 0.5,
            ],
            tilt: PANEL_TILT,
            half_w: plan.half_w() * PANEL_W,
            run,
            thick: plan.at(0.014),
        }
    }

    /// A point in the panel's OWN frame: `dx` athwart, `dn` out of its face
    /// along the normal, `dz` along its slope (forward and UP).
    ///
    /// The face normal is `(0, cos t, -sin t)` and the slope `(0, sin t,
    /// cos t)`, which is what puts a cell plate proud of the frame rather
    /// than through it.
    pub(super) fn on(&self, dx: f32, dn: f32, dz: f32) -> [f32; 3] {
        let (s, co) = (self.tilt.sin(), self.tilt.cos());
        [
            self.at[0] + dx,
            self.at[1] + dn * co + dz * s,
            self.at[2] - dn * s + dz * co,
        ]
    }

    /// The turn that lays a plate in that frame.
    ///
    /// NEGATIVE: [`quat_x`] carries `+y` toward `+z`, so `-tilt` faces the
    /// panel up AND AFT - at the chase camera, which looks down 22.9 degrees
    /// from astern. Turned the other way she shows the camera her back.
    pub(super) fn rot(&self) -> [f32; 4] {
        quat_x(-self.tilt)
    }
}

/// A tilted solar panel on a pedestal: a frame plate and [`CELLS`] dark cell
/// plates a hair proud of it, so the frame shows between them as lines.
///
/// `dead` is a battered machine's failed cell, drawn in primer - a slot
/// rather than a node, which is why her Battered tier adds no geometry at
/// all.
fn solar_panel(kids: &mut Vec<Generator>, plan: &RoverPlan, c: &RoverColours, dead: Option<usize>) {
    let p = Panel::of(plan);
    kids.push(plate(
        [p.half_w * 2.0, p.thick, p.run],
        &c.frame,
        p.at,
        p.rot(),
        0.04,
    ));
    let cw = p.half_w * 2.0 / CELLS as f32;
    for k in 0..CELLS {
        let dx = (k as f32 - (CELLS - 1) as f32 * 0.5) * cw;
        let slot = if dead == Some(k) { &c.primer } else { &c.cell };
        kids.push(plate(
            [cw * 0.90, p.thick, p.run * 0.94],
            slot,
            p.on(dx, plan.at(0.004), 0.0),
            p.rot(),
            0.04,
        ));
    }
    // The pedestal, from inside the deck to inside the frame under its
    // middle: both ends reach IN, so touch reads a joint at each.
    kids.push(line(
        &[
            ([0.0, plan.top() - plan.at(0.010), p.at[2]], plan.at(0.030)),
            (p.on(0.0, -plan.at(0.002), 0.0), plan.at(0.024)),
        ],
        8,
        &c.arm,
    ));
}

/// The dish's x over the deck's half-width on the FAR side, its station over
/// the length, its radius over the length, and how far its axis is turned
/// aft toward the chase camera (rad).
const DISH_AT: (f32, f32) = (0.46, 0.150);
const DISH_R: f32 = 0.115;
const DISH_TILT: f32 = 0.62;

/// A dish is a lamp shell turned inside out: the profile STARTS and ENDS ON
/// THE AXIS, or the Lathe caps its open end with a full disc and the dish is
/// a drum (the disc-cap trap, #1364).
fn dish_profile(r: f32) -> [(f32, f32); 8] {
    [
        (0.0, 0.0),
        (r * 0.45, r * 0.03),
        (r * 0.80, r * 0.14),
        (r, r * 0.30),
        (r * 0.97, r * 0.33),
        (r * 0.76, r * 0.19),
        (r * 0.42, r * 0.10),
        (0.0, r * 0.075),
    ]
}

/// The dish on a stem off the far side, its mouth turned up and aft.
fn dish(kids: &mut Vec<Generator>, plan: &RoverPlan, c: &RoverColours) {
    let l = plan.length;
    let (x, z) = (-NEAR * plan.half_w() * DISH_AT.0, DISH_AT.1 * l);
    let r = DISH_R * l;
    // Under the cap by its own radius, so the dish's rim is what lands under
    // rule 6 rather than its centre.
    let y = (plan.top() + plan.at(0.120)).min(plan.cap_y() - r);
    kids.push(line(
        &[
            ([x, 0.0, z], plan.at(0.014)),
            ([x, y + plan.at(0.004), z], plan.at(0.012)),
        ],
        8,
        &c.arm,
    ));
    kids.push(solid(
        &dish_profile(r),
        20,
        true,
        &c.body,
        [x, y, z],
        quat_x(-DISH_TILT),
    ));
}

// ---------------------------------------------------------------------------
// The carapace: a chitin shell over a dark running deck
// ---------------------------------------------------------------------------

/// The shell's half-width over the deck's, its half-length over the deck's
/// run, its half-height (of the length) and its centre over the deck's top
/// (of the length).
const SHELL_W: f32 = 1.34;
const SHELL_RUN: f32 = 0.545;
const SHELL_H: f32 = 0.082;
const SHELL_LIFT: f32 = 0.018;

/// The shell's box, as the two things laid on it read.
#[derive(Clone, Copy, Debug)]
pub(super) struct Shell {
    /// Its centre (m, root-local).
    pub(super) at: [f32; 3],
    /// Its half extents (m).
    pub(super) half: [f32; 3],
}

impl Shell {
    pub(super) fn of(plan: &RoverPlan) -> Self {
        let (_, _, zc, run) = plan.deck_run();
        Self {
            at: [0.0, plan.top() + SHELL_LIFT * plan.length, zc],
            half: [
                plan.half_w() * SHELL_W,
                SHELL_H * plan.length,
                run * SHELL_RUN,
            ],
        }
    }

    /// The shell's CROWN at station `z` (m over the datum) - the ellipsoid's
    /// own, which is what the exponent-1.0 form draws. The dorsal ridge, the
    /// primer patch and a carapace's antenna whip all stand off this rather
    /// than off the box's flat top, or they float at the ends.
    pub(super) fn crown(&self, z: f32) -> f32 {
        let t = ((z - self.at[2]).abs() / self.half[2]).min(0.999);
        self.at[1] + self.half[1] * (1.0 - t * t).sqrt()
    }
}

/// The one soft form on any skiff, and even here not a BlobGroup: a
/// Superellipsoid at exponent 1.0, in [`MaterialKit::skin`]'s CHITIN.
///
/// The Lathe form under a z scale draws the same silhouette for 219 B more
/// and a node scale she otherwise does not carry, and was rejected.
///
/// [`MaterialKit::skin`]: crate::seeded_defaults::MaterialKit::skin
fn carapace(kids: &mut Vec<Generator>, plan: &RoverPlan, c: &RoverColours) {
    let s = Shell::of(plan);
    kids.push(prim(
        superellipsoid(s.half, 1.0, 1.0, 14, 24, c.shell.clone()),
        s.at,
        id_quat(),
    ));
    // **Identity**, on a machine whose deck edge is under her shell: a
    // dorsal RIDGE along the shell's crown, PROUD of it - a ridge sunk into
    // the shell is a hairline at 12 m.
    let pts: Vec<([f32; 3], f32)> = (0..7)
        .map(|k| {
            let z = s.at[2] + s.half[2] * (-0.80 + 1.60 * k as f32 / 6.0);
            ([0.0, s.crown(z) - plan.at(0.004), z], plan.at(0.0150))
        })
        .collect();
    kids.push(line(&pts, 8, &c.strip));
}

// ---------------------------------------------------------------------------
// The monolith: an upright lit slab
// ---------------------------------------------------------------------------

/// The slab's half-width over the deck's, its run along the machine (of the
/// length) and its height (of the length).
pub(super) const SLAB_W: f32 = 0.74;
const SLAB_RUN: (f32, f32) = (-0.270, 0.090);
const SLAB_H: f32 = 0.185;

/// How far the slab's flanks are drawn in toward its roof, per axis
/// `[x, z]`, and how far up its own taper the flank seam is bedded.
const SLAB_TAPER: [f32; 2] = [0.16, 0.10];
const SEAM_AT: f32 = 0.55;

/// The slab's roof (m over the datum).
pub(super) fn slab_top(plan: &RoverPlan) -> f32 {
    plan.top() + SLAB_H * plan.length - plan.at(0.010)
}

/// An upright SLAB standing on the deck: plain, hard-edged and TALLER THAN
/// IT IS LONG, which is what makes it a standing stone rather than the crate
/// the first render drew.
///
/// Three lit seams, on the two surfaces a chase camera at 22.9 degrees of
/// look-down can see: one down the middle of its roof and one along each
/// flank. The flank seams lie ON the DRAWN flank at their own height - the
/// slab's taper has already drawn it in above the deck, and a seam cut to
/// the widest line hangs in the air (the armoured car's arches, again).
fn monolith(kids: &mut Vec<Generator>, plan: &RoverPlan, c: &RoverColours) {
    let l = plan.length;
    let (z0, z1) = (SLAB_RUN.0 * l, SLAB_RUN.1 * l);
    let (hw, h, run) = (plan.half_w() * SLAB_W, SLAB_H * l, z1 - z0);
    let zc = (z0 + z1) * 0.5;
    let yc = plan.top() - plan.at(0.010) + h * 0.5;
    kids.push(tapered_plate(
        [hw * 2.0, h, run],
        &c.body,
        [0.0, yc, zc],
        NO_TURN,
        0.03,
        SLAB_TAPER,
        [0.0; 2],
    ));
    kids.push(plate(
        [plan.at(0.030), plan.at(0.014), run * 0.82],
        &c.strip,
        [0.0, slab_top(plan) - plan.at(0.004), zc],
        NO_TURN,
        0.10,
    ));
    for s in [-1.0f32, 1.0] {
        let x = hw * (1.0 - SLAB_TAPER[0] * SEAM_AT);
        kids.push(plate(
            [plan.at(0.016), h * 0.30, run * 0.78],
            &c.strip,
            [s * x, yc + h * 0.05, zc],
            NO_TURN,
            0.10,
        ));
    }
}

// ---------------------------------------------------------------------------
// Where an aura issues from
// ---------------------------------------------------------------------------

/// Where a seeded aura issues from (root-local, m).
///
/// A flourish hovers over the deck, clear of the mast and the dish and over
/// whatever the variant stands on it, never inside a volume (#1367): over
/// the carapace's crown, over the monolith's slab, and off the surveyor's
/// panel face. Her 25 SpaceOutpost seeds carry no emitter at all - their
/// Thruster floors to Exhaust on a skiff and her servo drops it - so the
/// surveyor's mount is drawn by the guards and by nothing else.
///
/// An exhaust or a steam wisp leaves the ground between the rear tyres. Her
/// drive drops both, so no live seed reaches it; it is here because the
/// mount is total over [`ParticleAura`], and dust off the rear tyres is what
/// the wisp would be if #1381 ever wanted one. (It would not read: at 22.9
/// degrees of look-down nothing under the machine is ever in frame.)
pub(super) fn aura_mount(aura: ParticleAura, plan: &RoverPlan) -> [f32; 3] {
    if matches!(aura, ParticleAura::Exhaust | ParticleAura::Steam) {
        let a = plan.axle(-1.0);
        return [0.0, a.y - a.r * 0.80, a.z - a.r * 1.10];
    }
    let l = plan.length;
    match plan.variant {
        RoverVariant::Carapace => {
            let s = Shell::of(plan);
            let z = plan.deck_run().2;
            [0.0, s.crown(z) + 0.050 * l, z]
        }
        RoverVariant::Monolith => {
            let z = (SLAB_RUN.0 + SLAB_RUN.1) * 0.5 * l;
            [0.0, slab_top(plan) + 0.050 * l, z]
        }
        RoverVariant::Surveyor => Panel::of(plan).on(0.0, 0.060 * l, 0.0),
    }
}
