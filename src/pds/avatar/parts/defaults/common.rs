//! Shared seeded-choice helpers every default-part family file uses, over a
//! re-export of the avatar-wide colour vocabulary.
//!
//! The colour maths itself moved up to [`crate::pds::avatar::colour`] with
//! #1363, so the redesigned boat families - which assemble their own geometry
//! and have no parts behind them - can reach it too. Every name is re-exported
//! here at its old visibility, so no call site in the catalogue moved.
//!
//! The universal default parts are ordinary
//! [`PartDef`](super::super::PartDef) table rows (with empty styles and
//! `ANY` bands) alongside the styled kits - one table idiom for every part
//! (#798).

pub(in crate::pds::avatar::parts) use crate::pds::avatar::colour::{
    darken, ensure_delta, luma, shade,
};
pub(super) use crate::pds::avatar::colour::{floor_value, saturate};

// ---------------------------------------------------------------------------
// Humanoid
// ---------------------------------------------------------------------------
