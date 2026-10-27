//! Cross-theme family monuments (#1796) - owner monuments that read right in
//! several themes rather than one.
//!
//! Every seeded world stands one identity monument carrying its owner's
//! portrait beside its gate, drawn from a small pool: the theme's own two
//! times in three, else a kindred theme's
//! (`seeded_defaults::room::monument::kindred_monuments`). These entries
//! widen the pools of the themes that held their own alone. Each is
//! themeless, as `civic_monument` is - a themed one would become its
//! theme's first monument and displace the theme's own - and joins a pool
//! only by being named in the kindred tables.
//!
//! They borrow the palettes and surfaces of the themes they stand in, and
//! follow the rules [`civic::monument`](super::civic::monument) sets for the
//! whole family: the portrait is [`pfp_panel`](super::util::pfp_panel), the
//! frame carries the design with the panel blank, and a backing plate stands
//! behind it.

pub mod figurehead;
pub mod holo_pillar;
pub mod living_frame;
pub mod market_cross;
pub mod serpent_altar;
pub mod stone_lantern;
pub mod trophy;
pub mod works_plaque;
