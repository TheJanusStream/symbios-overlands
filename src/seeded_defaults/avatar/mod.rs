//! Avatar-scope DID-seeded derivers.
//!
//! Mirrors [`super::room`] in shape: each submodule owns one
//! parameter group (palette, body proportions, gait) and is fully
//! independent — the avatar deriver doesn't read the room deriver
//! (a user's avatar looks the same regardless of which room they
//! visit) and the per-submodule outputs don't depend on each other.
//!
//! What's wired today vs. computed-but-unused:
//!
//! - [`AvatarPalette`] — primary / secondary / tertiary accents are
//!   applied to the default hover-boat visuals. Skin, hair, and eye
//!   tones are computed but unused (no humanoid surface yet).
//! - [`AvatarBody`] — `height_scale`, `shoulder_width_scale`,
//!   `head_scale`, `limb_thickness_scale` map onto the hover-boat's
//!   cuboid / capsule / cylinder / sphere sizes. `torso_leg_ratio`
//!   is computed but unused.
//! - [`AvatarGait`] — fully unused; surface defined so a future
//!   humanoid spawn path can read it without extending the deriver.

pub mod body;
pub mod gait;
pub mod palette;

pub use body::{AvatarBody, BodyArchetype};
pub use gait::AvatarGait;
pub use palette::AvatarPalette;
