//! The travel pose every seeded craft's visual root takes.
//!
//! Kept apart from [`super::common`] (pure primitive geometry) because it
//! owns the root's placement rather than any shape. It used to hold the
//! assembly layer too - the outfit and part-registry helpers the airship's
//! assembler mounted its slots with - which retired with that assembler
//! when the airship became a craft-type family (#1657).

use std::f32::consts::PI;

use crate::pds::generator::Generator;
use crate::pds::types::Fp3;

use super::common::{quat_xyzw, quat_y};

/// Set the family's travel pose on the visual root: craft are authored
/// front-`+Z`, so yaw 180° to travel nose-first, then drop by `drop_y` onto
/// the craft's ground or hover line. The family builder *owns* the root's
/// placement - this overwrites whatever transform its root was built with.
///
/// **No scale.** It used to take one: the airship-class bridge of #1361, a
/// single uniform factor that drew a legacy boat or skiff at the size owner
/// decision 1 wanted without disturbing a mount. Both families are authored at
/// the size they are drawn at now (#1363, #1364), so every seeded craft's root
/// is scale-free and that is worth being structural rather than a convention -
/// a scaled root is what the root-scale discipline exists to keep out of the
/// tree, and the bridge was the one sanctioned exception.
pub(super) fn apply_travel_pose(root: &mut Generator, drop_y: f32) {
    root.transform.rotation = quat_xyzw(quat_y(PI));
    root.transform.translation = Fp3([0.0, -drop_y, 0.0]);
    root.transform.scale = Fp3([1.0; 3]);
}
