//! Shared colour / seeded-choice helpers every default-part family file
//! uses. The universal default parts are ordinary
//! [`PartDef`](super::super::PartDef) table rows (with empty styles and
//! `ANY` bands) alongside the styled kits — one table idiom for every part
//! (#798).

/// Multiply a colour toward black by `f` (`0` = black, `1` = unchanged) —
/// the local "darker shade of the same hue" used for trousers / skirts /
/// bumpers so a second large surface stays tonally related to the primary.
/// Shared across the whole part catalogue (the styled vehicle kits darken
/// with it too), hence visible to all of `parts`.
pub(in crate::pds::avatar::parts) fn shade(c: [f32; 3], f: f32) -> [f32; 3] {
    [c[0] * f, c[1] * f, c[2] * f]
}

/// A hard darken to 40 % — the shorthand for a shadowed underside / lining /
/// tyre / bumper that the humanoid and vehicle kits both reach for.
pub(in crate::pds::avatar::parts) fn darken(c: [f32; 3]) -> [f32; 3] {
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
pub(super) fn luma(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// Linear mix of `c` toward `target` by `t` (clamped to gamut).
pub(super) fn mix(c: [f32; 3], target: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        (c[0] * (1.0 - t) + target[0] * t).clamp(0.0, 1.0),
        (c[1] * (1.0 - t) + target[1] * t).clamp(0.0, 1.0),
        (c[2] * (1.0 - t) + target[2] * t).clamp(0.0, 1.0),
    ]
}

/// Retint `c` to hit `target_l` luma by mixing toward white (to brighten) or
/// black (to darken) — keeps the hue, moves only the value.
pub(super) fn to_value(c: [f32; 3], target_l: f32) -> [f32; 3] {
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
pub(super) fn floor_value(c: [f32; 3], min_l: f32) -> [f32; 3] {
    if luma(c) < min_l {
        to_value(c, min_l)
    } else {
        c
    }
}

/// Push `c`'s value away from `ref_l` until they differ by at least
/// `min_delta` (staying on whichever side `c` already sits) — keeps two
/// adjacent vehicle surfaces (hull/deck, body/glass) from merging into one
/// mass on a low-contrast seed.
pub(super) fn ensure_delta(c: [f32; 3], ref_l: f32, min_delta: f32) -> [f32; 3] {
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

/// Deepen + saturate a colour toward its dominant channel — a running-light
/// glow wants to be a saturated jewel, not a pastel.
pub(super) fn saturate(c: [f32; 3]) -> [f32; 3] {
    let l = luma(c);
    [
        (c[0] + (c[0] - l) * 0.6).clamp(0.0, 1.0),
        (c[1] + (c[1] - l) * 0.6).clamp(0.0, 1.0),
        (c[2] + (c[2] - l) * 0.6).clamp(0.0, 1.0),
    ]
}

// ---------------------------------------------------------------------------
// Humanoid
// ---------------------------------------------------------------------------
