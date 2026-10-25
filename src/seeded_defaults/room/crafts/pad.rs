//! The places that stand on the ground as a building does (#1782): a pad
//! for a helicopter and a mast for an airship - the cross-theme helipad and
//! mooring mast, or the theme's own (a space outpost's landing pad, a
//! steampunk airship dock) - each on dry, flat ground near the arrival, with
//! room round it for its craft to come and go.

use super::{
    BESIDE_THE_VIEW, FARTHEST, FREEBOARD, Ground, MARGIN, Siting, Stands, own_entry, placed,
};
use crate::catalogue::{CatalogueEntry, Craft};
use crate::seeded_defaults::room::settlement::SITE_SLOPE;

/// How far a pad's and a mast's ground may rise and fall under them, on
/// the probe (m): a pad's skirt and a mast's base close a little more, the
/// probe under-reading relief (#1773).
const PAD_RELIEF: f32 = 0.3;
const MAST_RELIEF: f32 = 0.5;

/// How much room a craft takes round its place (m): a helicopter's rotor,
/// an airship swinging round its mast in the wind.
const ROTOR: f32 = 6.0;
const SWING: f32 = 14.0;

/// How much farther a place in front of the view reads than one beside it
/// (m): it stands there only where nothing that much nearer beside does.
const AHEAD: f32 = 60.0;

/// A place that stands on the ground: its craft, its entry's slug, where it
/// stands and its ground's reach.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Standing {
    pub craft: Craft,
    pub slug: &'static str,
    pub at: [f32; 2],
    pub radius: f32,
    /// Its yaw: its front to the landing.
    pub yaw: f32,
}

/// The cross-theme entry a craft stands on, where its theme has none.
fn cross_theme(craft: Craft) -> Option<&'static dyn CatalogueEntry> {
    let slug = match craft {
        Craft::Helicopter => "helipad",
        Craft::Airship => "mooring_mast",
        Craft::Boat | Craft::Airplane | Craft::Car => return None,
    };
    crate::catalogue::by_slug(slug)
}

/// Where `craft`'s place stands best (see the module docs): the nearest
/// lattice point to the landing whose ground takes it, clear by its
/// craft's room of all that is kept; one in front of the view only where
/// none beside it [`AHEAD`] nearer does.
pub(super) fn site(siting: &Siting, craft: Craft) -> Option<Standing> {
    let entry = own_entry(siting.theme, craft).or_else(|| cross_theme(craft))?;
    let radius = entry.footprint().clearance;
    let (relief, room) = match craft {
        Craft::Helicopter => (PAD_RELIEF, ROTOR),
        _ => (MAST_RELIEF, SWING),
    };
    // Nearest first, those in front of the view as if AHEAD farther.
    let mut points = siting.lattice(1.0, FARTHEST, radius);
    let key = |at: [f32; 2], far: f32| {
        far + if siting.off_the_view(at) > BESIDE_THE_VIEW {
            AHEAD
        } else {
            0.0
        }
    };
    points.sort_by(|a, b| key(a.0, a.1).total_cmp(&key(b.0, b.1)));
    let at = points.into_iter().map(|(at, _)| at).find(|&at| {
        // The cheap test first: what it keeps clear of.
        siting
            .kept
            .clear(&Ground::disc(at, radius), room.max(MARGIN))
            && {
                let ground = siting.probe.footprint(at, radius);
                ground.above_water >= FREEBOARD
                    && ground.relief <= relief
                    && ground.steepest <= SITE_SLOPE
            }
    })?;
    let landing = siting.view.landing;
    Some(Standing {
        craft,
        slug: entry.slug(),
        at,
        radius,
        yaw: super::facing([landing[0] - at[0], landing[1] - at[1]]),
    })
}

impl Standing {
    /// Its ground and its craft's room round it.
    pub(super) fn ground(&self) -> Vec<Ground> {
        let room = match self.craft {
            Craft::Helicopter => ROTOR,
            _ => SWING,
        };
        vec![Ground::disc(self.at, self.radius + room)]
    }

    /// Its generator and its placement: built as a building is, off the
    /// water and on the highest ground under it.
    pub(super) fn wire(&self, wiring: &mut super::Wiring) {
        let name = match self.craft {
            Craft::Helicopter => "craft_pad",
            _ => "craft_mast",
        };
        let Some(entry) = crate::catalogue::by_slug(self.slug) else {
            return;
        };
        wiring
            .generators
            .push((name.to_string(), entry.build(&wiring.did)));
        wiring.placements.push(placed(
            name,
            self.at,
            0.0,
            self.yaw,
            Stands::Built(self.radius),
        ));
    }
}
