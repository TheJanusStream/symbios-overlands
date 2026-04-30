//! Sanitiser for [`SignSource`] (URL / atproto blob / DID-pfp variants)
//! and the paired `Sign` generator clamp. Defends against megabyte
//! URLs, NaN / negative panel sizes, and UV repeat factors so high they
//! pin the fragment shader on a sub-pixel texel pattern.

use super::Sanitize;
use super::common::clamp_finite;
use super::limits;
use crate::pds::generator::{AlphaModeKind, SignSource};
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::{Fp2, truncate_on_char_boundary};

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

/// Clamp every numeric field on a `Sign` generator and bound its source
/// strings. Mirrors the inline-fields layout of `GeneratorKind::Sign` so
/// the dispatcher can pass each field through.
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

    let r_lo = limits::MIN_SIGN_UV_REPEAT;
    let r_hi = limits::MAX_SIGN_UV_REPEAT;
    uv_repeat.0[0] = clamp_finite(uv_repeat.0[0], r_lo, r_hi, 1.0);
    uv_repeat.0[1] = clamp_finite(uv_repeat.0[1], r_lo, r_hi, 1.0);

    let o = limits::MAX_SIGN_UV_OFFSET;
    uv_offset.0[0] = clamp_finite(uv_offset.0[0], -o, o, 0.0);
    uv_offset.0[1] = clamp_finite(uv_offset.0[1], -o, o, 0.0);

    material.sanitize();

    if let AlphaModeKind::Mask { cutoff } = alpha_mode {
        cutoff.0 = clamp_finite(cutoff.0, 0.0, 1.0, 0.5);
    }
}
