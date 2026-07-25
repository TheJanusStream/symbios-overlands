//! Sanitiser for [`SignSource`] (URL / atproto blob / DID-pfp variants)
//! and the paired `Sign` generator clamp. Defends against megabyte
//! URLs, NaN / negative panel sizes, and UV repeat factors so high they
//! pin the fragment shader on a sub-pixel texel pattern.

use super::Sanitize;
use super::common::clamp_finite;
use super::limits;
use crate::pds::generator::{AlphaModeKind, SignSource};
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::{Fp, Fp2, truncate_on_char_boundary};

impl Sanitize for SignSource {
    fn sanitize(&mut self) {
        match self {
            SignSource::Url { url } => {
                truncate_on_char_boundary(url, limits::MAX_SIGN_URL_BYTES);
            }
            SignSource::AtprotoBlob { did, cid } => {
                truncate_on_char_boundary(did, limits::MAX_SIGN_DID_BYTES);
                truncate_on_char_boundary(cid, limits::MAX_SIGN_CID_BYTES);
            }
            SignSource::DidPfp { did } => {
                truncate_on_char_boundary(did, limits::MAX_SIGN_DID_BYTES);
            }
            SignSource::Unknown => {}
        }
    }
}

/// Clamp every numeric field on a `Sign` generator, bound its source
/// strings, and fold the legacy UV window into the material (#964). Mirrors
/// the inline-fields layout of `GeneratorKind::Sign` so the dispatcher can
/// pass each field through.
pub(super) fn sanitize_sign(
    source: &mut SignSource,
    size: &mut Fp2,
    uv_repeat: &mut Fp2,
    uv_offset: &mut Fp2,
    material: &mut SovereignMaterialSettings,
    alpha_mode: &mut AlphaModeKind,
) {
    source.sanitize();

    let s = limits::MAX_SIGN_SIZE;
    size.0[0] = clamp_finite(size.0[0], 0.01, s, 1.0);
    size.0[1] = clamp_finite(size.0[1], 0.01, s, 1.0);

    migrate_legacy_uv_window(uv_repeat, uv_offset, material);

    // After the fold, so the migrated values go through the same clamps as
    // an authored one.
    material.sanitize();

    if let AlphaModeKind::Mask { cutoff } = alpha_mode {
        cutoff.0 = clamp_finite(cutoff.0, 0.0, 1.0, 0.5);
    }
}

/// Fold a pre-#964 Sign's `uv_repeat` / `uv_offset` into the material's UV
/// transform and reset them to the identity.
///
/// The two used to be baked into the panel mesh's four vertex UVs
/// (`uv = offset + repeat · t`); they are now the material's
/// `uv_scale` / `uv_offset`, applied by the same affine every other surface
/// in the app goes through. Matching the old look is arithmetic: with the
/// mesh now emitting a plain `0..1` quad, `sovereign_uv_transform` samples
/// `scale · t + scale · material_offset`, so `scale = repeat` and
/// `material_offset = offset / repeat`.
///
/// **Idempotent by construction** — the reset to the identity is what makes
/// a second sanitize pass a no-op, which the sanitize fixpoint requires.
///
/// The one lossy case is a legacy *anisotropic* window: a single `uv_scale`
/// cannot say "twice across U, once across V". The larger repeat wins, so an
/// axis may show *less* of the image than before but never more — a crop is
/// recoverable by eye, invented content is not.
fn migrate_legacy_uv_window(
    uv_repeat: &mut Fp2,
    uv_offset: &mut Fp2,
    material: &mut SovereignMaterialSettings,
) {
    let r_lo = limits::MIN_SIGN_UV_REPEAT;
    let r_hi = limits::MAX_SIGN_UV_REPEAT;
    let ru = clamp_finite(uv_repeat.0[0], r_lo, r_hi, 1.0);
    let rv = clamp_finite(uv_repeat.0[1], r_lo, r_hi, 1.0);
    let o = limits::MAX_SIGN_UV_OFFSET;
    let ou = clamp_finite(uv_offset.0[0], -o, o, 0.0);
    let ov = clamp_finite(uv_offset.0[1], -o, o, 0.0);

    let repeat = ru.max(rv);
    // Compose rather than assign: a material that already carries a scale
    // (authored after the unification) keeps it, and the legacy identity
    // leaves it untouched.
    let scale = material.uv_scale.0 * repeat;
    material.uv_scale = Fp(scale);
    if scale.abs() > f32::EPSILON {
        material.uv_offset = Fp2([
            material.uv_offset.0[0] + ou / scale,
            material.uv_offset.0[1] + ov / scale,
        ]);
    }

    *uv_repeat = Fp2([1.0, 1.0]);
    *uv_offset = Fp2([0.0, 0.0]);
}
