//! Berlin's detached house in the gothic horror theme's dress (#1600): a
//! Victorian gothic villa standing alone in its overgrown garden - the
//! rectory or the widow's house at the end of the lane, soot-dark brick or
//! stone, pointed windows under hoods on every side, a stoop to a pointed
//! door, and a steep slate roof with iron cresting.
//!
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its steep roof reaches out over them to the lot's edges and
//!   no further.
//! - **One villa, one masonry**, rolled once at the lot; its pale dressings
//!   (sills, pointed hoods, the string course, the stoop) name their own,
//!   as the town house's do.
//! - **Pointed windows all round**: each a tall window under a little gable
//!   of dressed stone, on the front, the garden side and both flanks.
//! - **A stoop and a porch.** The door stands up a flight of steps under a
//!   stained fanlight and a pointed hood, or (`Pick`ed once) under a gabled
//!   porch on brackets.
//! - **A steep roof, a gable or a tower**, `Pick`ed once: slate along the
//!   lane under iron cresting, a tall chimney stack on its flank; the same
//!   with a corner tower under a needle spire; or a steep gable turned to
//!   the lane, a hooded lancet in its peak and cresting along its ridge.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::wood;

/// The villa (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "gothic_horror_street_detached",
    name: "Gothic Villa",
    description: "A Victorian gothic villa alone in its garden: soot-dark brick or stone, pointed \
                  windows on every side, a stoop to a pointed door, and a steep slate roof with \
                  iron cresting, a front gable or a spired corner tower.",
    themes: &[ThemeArchetype::GothicHorror],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A raised ground storey, and the tall rooms over it.
    storey_m: (3.6, 3.2),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BB2_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Iron", "Chimney"],
};

/// The theme's street palette - soot-dark brick and stone, moss, pale
/// dressings, slate, black iron, the doors and a little stained glass -
/// and the dark timber of a porch's bargeboards.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.insert("Barge".to_string(), wood([0.18, 0.13, 0.12]));
    m
}
