//! OkLab / OkLCH ↔ sRGB color conversion.
//!
//! OkLab (Björn Ottosson, 2020) is a perceptually-uniform color space —
//! equal-magnitude movements in L, C, h produce roughly equal perceived
//! changes. The seeded palette deriver works in OkLCH so coordinated
//! colors (terrain, water, sky, cloud) can be sampled by perturbing
//! around a shared hue anchor without producing the muddy or clashing
//! results that uniform sRGB sampling gives.
//!
//! Matrices and transfer functions are from the original article:
//! <https://bottosson.github.io/posts/oklab/>.

/// sRGB nonlinear → linear (IEC 61966-2-1 transfer function).
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear → sRGB nonlinear.
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Linear sRGB → OkLab.
//
// Coefficients are copied verbatim from Ottosson's published article
// (linked above) so anyone cross-referencing the implementation against
// the paper sees an exact match. The extra decimals past f32's ~7-digit
// limit are silently rounded at compile time — keeping them lets the
// compiler do a single round-to-nearest from the published values
// instead of inheriting whatever shorter literal we'd hand-truncate to.
#[allow(clippy::excessive_precision)]
pub fn linear_srgb_to_oklab([r, g, b]: [f32; 3]) -> [f32; 3] {
    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();
    [
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    ]
}

/// OkLab → linear sRGB. May produce out-of-gamut negative values for
/// high-chroma inputs — the public sRGB helpers clamp after the inverse
/// transfer function.
//
// As above: coefficients are kept at paper precision so the compiler
// performs a single round-to-nearest to f32 rather than inheriting a
// hand-truncated approximation.
#[allow(clippy::excessive_precision)]
pub fn oklab_to_linear_srgb([l, a, b]: [f32; 3]) -> [f32; 3] {
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;
    let l_lin = l_ * l_ * l_;
    let m_lin = m_ * m_ * m_;
    let s_lin = s_ * s_ * s_;
    [
        4.0767416621 * l_lin - 3.3077115913 * m_lin + 0.2309699292 * s_lin,
        -1.2684380046 * l_lin + 2.6097574011 * m_lin - 0.3413193965 * s_lin,
        -0.0041960863 * l_lin - 0.7034186147 * m_lin + 1.7076147010 * s_lin,
    ]
}

/// OkLab → OkLCH (cylindrical form). `h` is in degrees `[0, 360)`.
pub fn oklab_to_oklch([l, a, b]: [f32; 3]) -> [f32; 3] {
    let c = (a * a + b * b).sqrt();
    let mut h = b.atan2(a).to_degrees();
    if h < 0.0 {
        h += 360.0;
    }
    [l, c, h]
}

/// OkLCH → OkLab.
pub fn oklch_to_oklab([l, c, h_deg]: [f32; 3]) -> [f32; 3] {
    let h = h_deg.to_radians();
    [l, c * h.cos(), c * h.sin()]
}

/// OkLCH → sRGB (`[0, 1]` per channel), gamut-clamped after the inverse
/// transfer function. Clipping near pure colors is unavoidable in any
/// perceptual → device-RGB pipeline; we lean on it as a hard guarantee
/// that every returned triple is a valid `Color::srgb` argument.
pub fn oklch_to_srgb(oklch: [f32; 3]) -> [f32; 3] {
    let lab = oklch_to_oklab(oklch);
    let lin = oklab_to_linear_srgb(lab);
    [
        linear_to_srgb(lin[0]).clamp(0.0, 1.0),
        linear_to_srgb(lin[1]).clamp(0.0, 1.0),
        linear_to_srgb(lin[2]).clamp(0.0, 1.0),
    ]
}

/// sRGB → OkLCH. Used to lift the existing constant palette into LCH
/// anchors the derivers perturb around.
pub fn srgb_to_oklch([r, g, b]: [f32; 3]) -> [f32; 3] {
    let lin = [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b)];
    let lab = linear_srgb_to_oklab(lin);
    oklab_to_oklch(lab)
}

/// Wrap a hue in degrees back into `[0, 360)`. Convenience for
/// composing hue offsets (e.g. `base_hue + 120.0` for a triadic colour).
pub fn wrap_hue_deg(h: f32) -> f32 {
    let mut x = h % 360.0;
    if x < 0.0 {
        x += 360.0;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn round_trip_mid_grey() {
        let grey = [0.5, 0.5, 0.5];
        let back = oklch_to_srgb(srgb_to_oklch(grey));
        for i in 0..3 {
            assert!(approx_eq(grey[i], back[i], 1e-3));
        }
    }

    #[test]
    fn round_trip_pure_red() {
        let red = [1.0, 0.0, 0.0];
        let back = oklch_to_srgb(srgb_to_oklch(red));
        for i in 0..3 {
            assert!(approx_eq(red[i], back[i], 1e-3));
        }
    }

    #[test]
    fn round_trip_typical_grass() {
        // The existing grass_dry constant — a deeply useful sanity check
        // because the palette deriver perturbs around this exact triple.
        let grass = [0.07, 0.12, 0.03];
        let back = oklch_to_srgb(srgb_to_oklch(grass));
        for i in 0..3 {
            assert!(
                approx_eq(grass[i], back[i], 2e-3),
                "channel {i}: {} → {}",
                grass[i],
                back[i]
            );
        }
    }

    #[test]
    fn hue_always_positive() {
        for ch in 0..8 {
            let rgb = [(ch as f32) / 8.0, 0.3, 0.7];
            let lch = srgb_to_oklch(rgb);
            assert!((0.0..360.0).contains(&lch[2]), "hue out of range: {lch:?}");
        }
    }

    #[test]
    fn zero_chroma_is_grey() {
        let rgb = oklch_to_srgb([0.5, 0.0, 0.0]);
        assert!(approx_eq(rgb[0], rgb[1], 1e-3));
        assert!(approx_eq(rgb[1], rgb[2], 1e-3));
    }

    #[test]
    fn wrap_hue_handles_negative_and_overflow() {
        assert!(approx_eq(wrap_hue_deg(-30.0), 330.0, 1e-4));
        assert!(approx_eq(wrap_hue_deg(420.0), 60.0, 1e-4));
        assert!(approx_eq(wrap_hue_deg(0.0), 0.0, 1e-4));
    }
}
