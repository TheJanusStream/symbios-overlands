//! A person's head measure - the hat line a fitted headband seats against
//! (#1089) - taken where the whole body is (#1815).
//!
//! The measure reads the closed surface, `Parts::body`, which the engine's
//! `serde-avatar` feature skips: a body built on the web's worker arrives in
//! the page without it, so a page that measured there would find no head and
//! wear a fitted circlet at its authored size. The worker measures instead,
//! before the body crosses, and the answer crosses beside it
//! ([`crate::GenResult::MeasuredAvatar`]). It lives here, and not in the app,
//! so that the worker and a native client run the one function on the one
//! surface: the app's dressing (`player::attachments`) calls [`hat_line`]
//! itself on a native body, and takes the worker's answer for a web one.
//!
//! Not the drawn skin: its body vertices are the closed surface's, but the
//! ears merged into it widen the perimeter low on the head and pull the
//! widest line down toward the eyes - measured over 80 seeded persons, up to
//! 37 mm lower than the closed surface's, a circlet over the eyes.

use serde::{Deserialize, Serialize};

/// Height samples walked between the eye line and the dome cap when hunting
/// the hat line ([`hat_line`]). The vault's perimeter varies
/// slowly in height - [`symbios_avatar::face::Skull`] itself holds a
/// handful of bands - so a dense scan buys nothing.
const HAT_LINE_STEPS: usize = 16;

/// Azimuth samples the perimeter polygon is summed over. At 64 the chord
/// shortfall of a circle is under 0.1%, far inside the wire's own
/// millimetre quantisation.
const HAT_LINE_AZIMUTHS: usize = 64;

/// The measured hat line of one built head: what a fitted headband seats
/// against. Heights are head-local metres (relative to the head joint), the
/// unit every [`symbios_avatar::face::Skull`] profile speaks.
///
/// Serialisable because it crosses the worker boundary beside the body it
/// measures ([`crate::GenResult::MeasuredAvatar`]).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct HatLine {
    /// The head joint the measurement is anchored to, an index into the
    /// body's `rig.joints`.
    pub head: usize,
    /// Horizontal perimeter of the vault at the hat line, metres.
    pub circumference: f32,
    /// Height of the hat line above the head joint, metres.
    pub height: f32,
}

/// Whether a dressing reads a person's head on this rig: no wings and a human
/// trunk, the app's `RiggedKind::of_rig` answering `Person`. A beast and a
/// winged body are seated by their own rules and never measured, so the
/// worker spends nothing on them.
pub fn reads_a_persons_head(rig: &symbios_avatar::Rig) -> bool {
    rig.wings.is_none() && rig.anatomy.human_trunk()
}

/// The wearer's hat line - where it sits and how far round it is - measured
/// from the built body through the engine's public measure surface.
///
/// The hat line is found the way a hatter finds it: the largest horizontal
/// perimeter of the cranial vault between the eye line and the dome's cap -
/// the line that runs just above the brow ridge and around the occiput.
/// Hunted rather than pinned to a landmark because the vault's widest line
/// moves with the head-breadth and face-length axes, and a band seated
/// anywhere narrower would slide down to it anyway.
///
/// Instruments, all public engine API: [`symbios_avatar::face::Skull`]
/// measures the built head (the same measured-not-planned argument as
/// `rig::Surface` - the mesh sits well inside the node radius);
/// [`symbios_avatar::Canon`] hands back the eye line so its constant is not
/// copied here (its docs record how copies drift); `Skull::surface_at`
/// answers "where is the surface at this height, in this direction", and
/// the perimeter is that polygon's length. The scan stops one band short of
/// the crown: the top band is closed to a pole and a perimeter there is
/// noise, not a hat line.
///
/// `None` for a body with no measurable head - hair never counts, the
/// engine measures the bare skull - and the caller falls back to authored
/// size; a body that crossed the worker, whose closed surface stayed behind,
/// is one. The eye-line ruler ignores its `EyeParams` argument for the
/// landmark read here (only pupil spacing reads it), so the default params
/// are not a guess smuggled in.
pub fn hat_line(avatar: &symbios_avatar::Avatar) -> Option<HatLine> {
    let skull = symbios_avatar::face::Skull::measure(&avatar.parts.body, &avatar.rig)?;
    let canon =
        symbios_avatar::Canon::measure(&avatar.rig, &skull, &symbios_avatar::EyeParams::default());
    let floor = canon.level;
    let (_, crown) = skull.throat_and_crown();
    let ceiling = crown - skull.crown_band();
    if ceiling <= floor {
        return None;
    }
    let mut best: Option<(f32, f32)> = None;
    for step in 0..=HAT_LINE_STEPS {
        let height = floor + (ceiling - floor) * step as f32 / HAT_LINE_STEPS as f32;
        let perimeter = perimeter_at(&skull, height);
        if best.is_none_or(|(widest, _)| perimeter > widest) {
            best = Some((perimeter, height));
        }
    }
    let (circumference, height) = best?;
    (circumference > f32::EPSILON && circumference.is_finite()).then_some(HatLine {
        head: skull.head,
        circumference,
        height,
    })
}

/// The head's horizontal perimeter at one height: the closed polygon of
/// [`symbios_avatar::face::Skull::surface_at`] samples around the full
/// turn. Every sample shares the height, so each chord is horizontal by
/// construction.
fn perimeter_at(skull: &symbios_avatar::face::Skull, height: f32) -> f32 {
    let mut total = 0.0;
    let mut prev = skull.surface_at(height, 0.0);
    for step in 1..=HAT_LINE_AZIMUTHS {
        let azimuth = std::f32::consts::TAU * step as f32 / HAT_LINE_AZIMUTHS as f32;
        let next = skull.surface_at(height, azimuth);
        total += (next - prev).length();
        prev = next;
    }
    total
}
