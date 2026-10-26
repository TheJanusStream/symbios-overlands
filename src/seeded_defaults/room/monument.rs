//! Seeded owner-monument spot (#975) - where a room's identity monument
//! stands relative to its social gateway.
//!
//! Every seeded room gets exactly one, for the same reason it gets exactly one
//! gateway: it is the room's identity, not settlement dressing. The monument
//! carries the owner's profile picture on a square panel, so it wants to be
//! *seen on arrival* rather than discovered later - which fixes its placement
//! to the one spot the engine already knows a visitor will be standing in and
//! looking from.
//!
//! It stands **beside the gate and turned toward the landing**: laterally clear
//! of the gatehouse by both footprints plus a margin, set forward to the
//! midpoint of the gate→landing walk so it is inside the arrival view cone, and
//! yawed to present its face to the person who just materialised there. The
//! gate is straight ahead; the monument is off to one side, facing you.
//!
//! Which side is seeded from the owner's DID, so a street of rooms does not
//! read as a template - but it is *stable* for a given room, because a
//! monument that moved between sessions would be a bug, not variety.
//!
//! Facing convention matches [`GatewaySpot`]: a
//! pose facing world-direction `(dx, dz)` has `yaw = atan2(-dx, -dz)`.

use super::gateway::GatewaySpot;

/// Extra gap between the gate's footprint edge and the monument's, on top of
/// both clearances, so the two read as a composed pair rather than a collision.
const MONUMENT_GATE_MARGIN: f32 = 3.0;

/// Floor on the lateral offset, so a room whose gate and monument are both
/// small still separates them enough to read as two structures.
const MIN_LATERAL: f32 = 5.0;

/// The monument a world of `theme` stands (#975): one of its
/// [pool](monument_pool) drawn by `seed` (#1783), or its own with none -
/// selected exactly like its gate
/// ([`gateway_entry`](super::gateway::gateway_entry)), from a stream of
/// its own.
pub fn monument_entry(
    theme: crate::seeded_defaults::ThemeArchetype,
    seed: Option<u64>,
) -> Option<&'static dyn crate::catalogue::CatalogueEntry> {
    super::gateway::draw_from_pool(&monument_pool(theme), seed, MONUMENT_POOL_SALT)
}

const MONUMENT_POOL_SALT: u64 = 0x1783_3070_1783_3070;

/// The monuments a world of `theme` may stand (#1783): the theme's own
/// first, else the cross-theme `civic_monument`, then those of kindred
/// themes that read right in it ([`kindred_monuments`]).
pub fn monument_pool(
    theme: crate::seeded_defaults::ThemeArchetype,
) -> Vec<&'static dyn crate::catalogue::CatalogueEntry> {
    let own = crate::catalogue::entries_for(theme, crate::catalogue::StructureRole::Monument)
        .next()
        .or_else(|| crate::catalogue::by_slug("civic_monument"));
    own.into_iter()
        .chain(
            kindred_monuments(theme)
                .iter()
                .filter_map(|slug| crate::catalogue::by_slug(slug)),
        )
        .collect()
}

/// The monuments of kindred themes a world of `theme` may stand besides its
/// own (#1783), by slug, chosen from a sheet of every monument for reading
/// right there: the civic stele in a classical, a campus or a city world,
/// timber frames among the farm, the frontier and the roadside, the
/// industrial frame among ruins, a garden planter in a suburb, the
/// gallows banner in a gothic or a fantasy world.
pub(super) fn kindred_monuments(
    theme: crate::seeded_defaults::ThemeArchetype,
) -> &'static [&'static str] {
    use crate::seeded_defaults::ThemeArchetype::*;
    match theme {
        AncientClassical | ModernCity => &["civic_monument"],
        CivicCampus => &["civic_monument", "modern_city_monument"],
        WildWest => &["rural_farmland_monument"],
        RuralFarmland | Roadside => &["wild_west_monument"],
        PostApoc => &["industrial_park_monument"],
        Suburban => &["solarpunk_monument"],
        GothicHorror | Fantasy => &["medieval_monument"],
        Medieval | Nordic | FeudalJapan | Mesoamerican | IndustrialPark | CoastalResort
        | SportsRec | Steampunk | Solarpunk | SpaceOutpost | Cyberpunk | AlienOrganic
        | AlienMonolithic | Pirate => &[],
    }
}

/// Derived monument placement for one room.
#[derive(Clone, Copy, Debug)]
pub struct MonumentSpot {
    /// World XZ of the monument's origin.
    pub offset: [f32; 2],
    /// Structure yaw (radians around Y), facing the gateway's landing.
    pub yaw_rad: f32,
}

impl MonumentSpot {
    /// Beside `gate`, on the side chosen by `did`, facing the gate's landing.
    ///
    /// `gate_clearance` and `monument_clearance` are the two footprint radii;
    /// the lateral offset clears both plus [`MONUMENT_GATE_MARGIN`].
    pub fn beside_gate(
        gate: &GatewaySpot,
        gate_clearance: f32,
        monument_clearance: f32,
        did: &str,
    ) -> Self {
        Self::on_side(gate, gate_clearance, monument_clearance, side_bit(did))
    }

    /// [`beside_gate`](Self::beside_gate) on a given side: the one `did`
    /// picks there, or each in turn for a settlement keeping both clear
    /// before its DID is known (#1774).
    pub fn on_side(
        gate: &GatewaySpot,
        gate_clearance: f32,
        monument_clearance: f32,
        side: bool,
    ) -> Self {
        // The approach bearing, origin → gate. Falls back to +Z for a gate
        // sitting improbably on the origin, so the geometry stays finite.
        let d = (gate.offset[0].powi(2) + gate.offset[1].powi(2)).sqrt();
        let bearing = if d > 1e-3 {
            [gate.offset[0] / d, gate.offset[1] / d]
        } else {
            [0.0, 1.0]
        };
        // Perpendicular to the approach, and the side to put it on.
        let side = if side { 1.0 } else { -1.0 };
        let lateral = [-bearing[1] * side, bearing[0] * side];
        let lat_dist =
            (gate_clearance + monument_clearance + MONUMENT_GATE_MARGIN).max(MIN_LATERAL);

        // Forward to the middle of the gate→landing walk, so the monument is
        // inside the view cone of someone standing on the landing looking at
        // the gate rather than behind their shoulder.
        let base = [
            (gate.offset[0] + gate.landing[0]) * 0.5,
            (gate.offset[1] + gate.landing[1]) * 0.5,
        ];
        let offset = [
            base[0] + lateral[0] * lat_dist,
            base[1] + lateral[1] * lat_dist,
        ];

        // Face the landing: to face direction `f`, yaw = atan2(-f.x, -f.z).
        let to_landing = [gate.landing[0] - offset[0], gate.landing[1] - offset[1]];
        let yaw_rad = (-to_landing[0]).atan2(-to_landing[1]);
        Self { offset, yaw_rad }
    }
}

/// How far ahead of a viewpoint's landing its monument stands (m, #1777):
/// nearer than the gate, so the two step back from the view in turn.
const MONUMENT_AHEAD: f32 = 3.0;

impl MonumentSpot {
    /// The monument at a seeded world's viewpoint (#1777): beside the view
    /// on the side its gate is not ([`GatewaySpot::at_viewpoint`] with
    /// `!gate_side`), a little ahead of the landing and nearer it than the
    /// gate, outside the view's corridor, turned to face the landing.
    pub fn at_viewpoint(
        viewpoint: &super::Viewpoint,
        monument_clearance: f32,
        gate_side: bool,
    ) -> Self {
        let [tx, tz] = viewpoint.toward;
        let right = if gate_side { [tz, -tx] } else { [-tz, tx] };
        let aside = super::viewpoint::CORRIDOR_HALF + monument_clearance;
        let [lx, lz] = viewpoint.landing;
        let offset = [
            lx + tx * MONUMENT_AHEAD + right[0] * aside,
            lz + tz * MONUMENT_AHEAD + right[1] * aside,
        ];
        let yaw_rad = libm::atan2f(offset[0] - lx, offset[1] - lz);
        Self { offset, yaw_rad }
    }

    /// The world's banner at a seeded world's viewpoint (#1783): on its
    /// monument's side of the view, across it from the gate, [`BANNER_AHEAD`]
    /// along it - past the monument, so the two read as a pair from the
    /// landing - its footprint just outside the view's corridor, turned to
    /// face the landing.
    pub fn banner_at_viewpoint(
        viewpoint: &super::Viewpoint,
        banner_clearance: f32,
        gate_side: bool,
    ) -> Self {
        let [tx, tz] = viewpoint.toward;
        let right = if gate_side { [tz, -tx] } else { [-tz, tx] };
        let aside = super::viewpoint::CORRIDOR_HALF + banner_clearance + BANNER_MARGIN;
        let [lx, lz] = viewpoint.landing;
        let offset = [
            lx + tx * BANNER_AHEAD + right[0] * aside,
            lz + tz * BANNER_AHEAD + right[1] * aside,
        ];
        let yaw_rad = libm::atan2f(offset[0] - lx, offset[1] - lz);
        Self { offset, yaw_rad }
    }
}

/// How far ahead of a viewpoint's landing the world's banner stands (m,
/// #1783): past the monument ([`MONUMENT_AHEAD`]).
const BANNER_AHEAD: f32 = 10.0;

/// The gap between the banner's footprint and the view's corridor (m).
const BANNER_MARGIN: f32 = 0.5;

/// Which side of a viewpoint's view its gate stands on (#1777): the
/// monument's side bit ([`MonumentSpot::beside_gate`]) turned over, so a
/// world whose monument stood on the left now has it there still.
pub fn gate_side(did: &str) -> bool {
    !side_bit(did)
}

/// One stable bit from the DID - which side of the approach the monument
/// stands on.
///
/// Uses the room seed's own [`fnv1a_64`], which is bit-exact across platforms
/// by construction: every peer visiting a room derives the same side locally,
/// with no authority to ask. A bit from the high half, because the low bits of
/// FNV-1a barely move between DIDs sharing a `did:plc:` prefix.
///
/// [`fnv1a_64`]: crate::seeded_defaults::hash::fnv1a_64
fn side_bit(did: &str) -> bool {
    (crate::seeded_defaults::hash::fnv1a_64(did) >> 33) & 1 == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spot(did: &str) -> (GatewaySpot, MonumentSpot) {
        let gate = GatewaySpot::for_landmark([24.0, 32.0], 6.0, 3.5);
        let mon = MonumentSpot::beside_gate(&gate, 3.5, 4.0, did);
        (gate, mon)
    }

    /// The monument clears the gate by both footprints plus the margin, so the
    /// two never intersect however the landmark bearing falls.
    #[test]
    fn the_monument_stands_clear_of_the_gate() {
        for did in ["did:plc:aaa", "did:plc:zzz", "did:web:example.com"] {
            let (gate, mon) = spot(did);
            let dx = mon.offset[0] - gate.offset[0];
            let dz = mon.offset[1] - gate.offset[1];
            let sep = (dx * dx + dz * dz).sqrt();
            assert!(
                sep > 3.5 + 4.0,
                "{did}: monument {sep} m from the gate - inside their combined footprints"
            );
        }
    }

    /// It faces the landing, which is the whole reason it is placed off the
    /// approach rather than on it: the arriving visitor sees its face, not its
    /// edge.
    #[test]
    fn the_monument_faces_the_landing() {
        for did in ["did:plc:aaa", "did:plc:zzz"] {
            let (gate, mon) = spot(did);
            let forward = [-mon.yaw_rad.sin(), -mon.yaw_rad.cos()];
            let to_landing = [
                gate.landing[0] - mon.offset[0],
                gate.landing[1] - mon.offset[1],
            ];
            let len = (to_landing[0].powi(2) + to_landing[1].powi(2)).sqrt();
            let dot = (forward[0] * to_landing[0] + forward[1] * to_landing[1]) / len;
            assert!(
                dot > 0.999,
                "{did}: monument yaw does not face the landing (dot {dot})"
            );
        }
    }

    /// It stands *off* the approach axis, so it never blocks the walk from the
    /// landing to the gate.
    #[test]
    fn the_monument_is_off_the_approach_axis() {
        let (gate, mon) = spot("did:plc:aaa");
        let d = (gate.offset[0].powi(2) + gate.offset[1].powi(2)).sqrt();
        let bearing = [gate.offset[0] / d, gate.offset[1] / d];
        // Distance from the origin→gate line.
        let lateral = (mon.offset[0] * bearing[1] - mon.offset[1] * bearing[0]).abs();
        assert!(
            lateral >= MIN_LATERAL - 1e-3,
            "monument only {lateral} m off the axis"
        );
    }

    /// The side is stable for a DID and varies between DIDs. Stability is the
    /// important half: a monument that changed sides between sessions would
    /// read as a bug.
    #[test]
    fn the_side_is_stable_per_did_and_varies_across_dids() {
        assert_eq!(side_bit("did:plc:abcdef"), side_bit("did:plc:abcdef"));
        let dids = [
            "did:plc:aaa",
            "did:plc:bbb",
            "did:plc:ccc",
            "did:plc:ddd",
            "did:plc:eee",
            "did:plc:fff",
            "did:web:a.example",
            "did:web:b.example",
        ];
        let left = dids.iter().filter(|d| side_bit(d)).count();
        assert!(
            left > 0 && left < dids.len(),
            "every one of {} sample DIDs picked the same side",
            dids.len()
        );
    }
}
