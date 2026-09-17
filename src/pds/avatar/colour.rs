//! Colour maths and the lit-window vocabulary shared across the avatar
//! families - the one home for "make this readable", so a boat's lit port
//! band and an airship's gondola glazing are the same light rather than two
//! guesses at it.
//!
//! It lived in `parts/defaults/common.rs` and `parts/defaults/airship.rs`,
//! visible only inside the part catalogue. The redesigned boat families
//! (`default_visuals::boats`) assemble their own geometry with no
//! parts behind them (#1363), so they could not reach any of it; moving it up
//! one level is the whole change, and `parts/defaults/common.rs` re-exports
//! every name so no call site inside the catalogue moved.

use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::{Fp, Fp3};

/// Multiply a colour toward black by `f` (`0` = black, `1` = unchanged) -
/// the local "darker shade of the same hue" used for trousers / skirts /
/// bumpers so a second large surface stays tonally related to the primary.
/// Shared across the whole part catalogue (the styled vehicle kits darken
/// with it too), hence visible to all of `parts`.
pub(crate) fn shade(c: [f32; 3], f: f32) -> [f32; 3] {
    [c[0] * f, c[1] * f, c[2] * f]
}

/// A hard darken to 40 % - the shorthand for a shadowed underside / lining /
/// tyre / bumper that the humanoid and vehicle kits both reach for.
pub(crate) fn darken(c: [f32; 3]) -> [f32; 3] {
    shade(c, 0.4)
}

// ---------------------------------------------------------------------------
// Value-contrast colour maths (#786/#787)
// ---------------------------------------------------------------------------
//
// Shared by the vehicle families (boat / skiff) to spread the palette triad
// across their surfaces and floor/separate the *values* so a dark or
// low-contrast seed still keeps readable part boundaries.

/// Perceptual-ish sRGB luma for value-contrast bookkeeping.
pub(crate) fn luma(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// Linear mix of `c` toward `target` by `t` (clamped to gamut).
pub(crate) fn mix(c: [f32; 3], target: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        (c[0] * (1.0 - t) + target[0] * t).clamp(0.0, 1.0),
        (c[1] * (1.0 - t) + target[1] * t).clamp(0.0, 1.0),
        (c[2] * (1.0 - t) + target[2] * t).clamp(0.0, 1.0),
    ]
}

/// Retint `c` to hit `target_l` luma by mixing toward white (to brighten) or
/// black (to darken) - keeps the hue, moves only the value.
pub(crate) fn to_value(c: [f32; 3], target_l: f32) -> [f32; 3] {
    let l = luma(c);
    if (l - target_l).abs() < 1e-3 {
        return c;
    }
    if target_l > l {
        mix(
            c,
            [1.0, 1.0, 1.0],
            ((target_l - l) / (1.0 - l).max(1e-3)).clamp(0.0, 0.85),
        )
    } else {
        mix(
            c,
            [0.0, 0.0, 0.0],
            ((l - target_l) / l.max(1e-3)).clamp(0.0, 0.85),
        )
    }
}

/// Raise `c`'s value to at least `min_l` (a dark seed's body never collapses
/// to an unreadable near-black block).
pub(crate) fn floor_value(c: [f32; 3], min_l: f32) -> [f32; 3] {
    if luma(c) < min_l {
        to_value(c, min_l)
    } else {
        c
    }
}

/// Push `c`'s value away from `ref_l` until they differ by at least
/// `min_delta` (staying on whichever side `c` already sits) - keeps two
/// adjacent surfaces (hull/deck, body/glass, coat/facing) from merging into
/// one mass on a low-contrast seed.
///
/// Widened past the vehicle families for the styled humanoid kit: a
/// justaucorps' turned-back lapels are the garment's signature and they are
/// cut from the *secondary* accent, which on plenty of seeds sits within a
/// few percent of the primary the coat is made of. Same failure, same fix.
pub(crate) fn ensure_delta(c: [f32; 3], ref_l: f32, min_delta: f32) -> [f32; 3] {
    let l = luma(c);
    if (l - ref_l).abs() >= min_delta {
        return c;
    }
    if l >= ref_l {
        to_value(c, (ref_l + min_delta).min(0.92))
    } else {
        to_value(c, (ref_l - min_delta).max(0.04))
    }
}

/// Deepen + saturate a colour toward its dominant channel - a running-light
/// glow wants to be a saturated jewel, not a pastel.
pub(crate) fn saturate(c: [f32; 3]) -> [f32; 3] {
    let l = luma(c);
    [
        (c[0] + (c[0] - l) * 0.6).clamp(0.0, 1.0),
        (c[1] + (c[1] - l) * 0.6).clamp(0.0, 1.0),
        (c[2] + (c[2] - l) * 0.6).clamp(0.0, 1.0),
    ]
}

// ---------------------------------------------------------------------------
// Lit windows (#789, moved here by #1363)
// ---------------------------------------------------------------------------

/// Normalize a raw accent into an interior-light window colour: saturate it to
/// a jewel (a greyed accent still reads as *coloured* light), floor its value
/// (a dark accent lights up instead of reading as a dead pane), and cap it
/// below white (a near-white accent doesn't blow the pane out to a featureless
/// slab). Standardizes the gondola glazing that used to inherit the raw
/// tertiary at a fixed glow strength - dead on dark seeds, blown out on pale
/// ones, only right when the tertiary happened to be cyan (#789, absorbing the
/// #781 window item; seed 12 is the target look).
pub(crate) fn window_light(accent: [f32; 3]) -> [f32; 3] {
    // Floor the value so a dark accent lights up, saturate to a jewel so even a
    // pale low-chroma tertiary reads as *coloured* light, then cap well below
    // white (pulling a light pane back down *raises* its chroma) so a pale
    // accent doesn't wash to a featureless slab (#789 review: seeds 45/48).
    let c = saturate(floor_value(accent, 0.44));
    if luma(c) > 0.7 { to_value(c, 0.7) } else { c }
}

/// A disciplined self-lit window material toned to a pre-[`window_light`]
/// -normalized colour: emissive, but at a running-light strength (not the
/// fixed `glow` 5.0 that blew pale panes out), so a cabin reads lit and warm
/// at any seed.
///
/// The only way a seeded craft gets a window at all: `SovereignMaterialSettings`
/// has no alpha, so a glass volume renders as a dark crate (#1359 rule 4).
pub(crate) fn window_material(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        metallic: Fp(0.3),
        roughness: Fp(0.35),
        emission_color: Fp3(color),
        emission_strength: Fp(3.6),
        ..Default::default()
    }
}
