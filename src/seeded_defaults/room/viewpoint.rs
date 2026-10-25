//! Arrival at a viewpoint (#1777) - where a seeded world's visitor lands:
//! out at its settlement's edge, on level ground, looking at its landmark
//! across a clear view.
//!
//! The landing used to stand in the landmark's lap - a median 22 m from it,
//! the gate 4.5 m ahead on the sightline - so the first frame held the gate
//! and the landmark's foot. Now the settlement plan picks a viewpoint as
//! soon as its primary landmark stands, before anything else is sited, so
//! the secondaries and props keep off the forecourt and off the view
//! ([`Viewpoint::keep_outs`]):
//!
//! - **At a distance scaled to the landmark** - far enough that it fills
//!   about a third of the frame's height and half its width - asked for
//!   within [`MIN_VIEW`] to [`MAX_VIEW`] of its middle, and tried at 0.7 to
//!   1.4 of that where the ground asks ([`DISTANCE_STEPS`]).
//! - **On gentle, dry ground**: the visitor's spot level, the forecourt
//!   round it - the gate and the monument on their plinths - no cliff,
//!   read on the probe as a member's ground is; a gentler tier of the two
//!   ([`TIERS`]) only where no site is level enough.
//! - **In sight of it**: the probe's ground stays [`SIGHT_MARGIN`] under the
//!   line from the arrival camera to the landmark's middle - the full map's
//!   ground wanders between the probe's samples - and the way there crosses
//!   no water.
//! - **Scored** on that distance, on standing above the landmark rather than
//!   under it, on level ground, on seeing its top as well as its middle, and
//!   on water off the axis of the view, a second thing to look at.
//!
//! A world's candidate settlements are compared by their viewpoints
//! ([`Viewpoint::score`]) and by what their first frames show
//! ([`Viewpoint::frames`], #1778).
//!
//! It also sets the arrival camera's pitch ([`arrival_pitch`]): the orbit
//! camera's rest pitch frames nothing above its own eye, so a landmark more
//! than about six metres tall was cut off at any distance. The landing
//! carries the pitch that shows the landmark's top under the frame's edge.
//!
//! The probe cannot see everything the compile does: a landmark or a
//! landing can stand where the full map has water, or a steep spot the
//! probe smooths away, and the compile walks it off. So the primary landmark
//! keeps a freeboard over the water (`settlement::LANDMARK_FREEBOARD`: over
//! seeds 0-299 the compile walked 6 landmarks in 211, against 17 without
//! it), the landing walks off water and steep ground as the landmark does,
//! and where the compile walks either it turns the landing to face the
//! landmark where it stands (`world_builder::compile::landing_on`).
//!
//! Every decision here reads the probe with plain arithmetic and libm's
//! trigonometry (#1132), and the landmark's height comes in rounded to the
//! decimetre (`settlement::landmark_top`), so every peer picks the same
//! viewpoint - but for a landmark whose measured height falls within a hair
//! of a rounding step.

use super::siting::{RIM, TerrainProbe};

/// The nearest distance from its landmark's middle a viewpoint asks for
/// (m), which it stands at 0.7 of where the ground asks ([`DISTANCE_STEPS`]):
/// a poor world's landmark - a hovel, a lean-to, a crash shelter - stands 2
/// to 4 m tall and read as a speck from 60.
pub const MIN_VIEW: f32 = 30.0;

/// The farthest it asks for (m), which it stands at 1.4 of at the most.
pub const MAX_VIEW: f32 = 150.0;

/// How far a viewpoint stands per metre of its landmark's height: it shows
/// the landmark about a third of the lens's 45 degrees tall (one over the
/// tangent of 15 degrees).
pub(super) const VIEW_PER_HEIGHT: f32 = 3.7;

/// And per metre of the landmark's ground radius: about half the frame's
/// width.
const VIEW_PER_RADIUS: f32 = 3.1;

/// The distances tried, as shares of the one the landmark asks for, the
/// preferred first.
const DISTANCE_STEPS: [f32; 5] = [1.0, 0.85, 1.2, 0.7, 1.4];

/// The radius of the forecourt's ground (m): the landing, its gate and its
/// monument ([`super::GatewaySpot::at_viewpoint`],
/// [`super::MonumentSpot::at_viewpoint`]) and the room to stand about them.
pub const FORECOURT: f32 = 14.0;

/// The radius of the spot the visitor is set down on (m).
const LANDING_SPOT: f32 = 4.0;

/// How level the ground must be, tier by tier, on the probe. The gentler
/// tier is tried only where nothing passes the level one: a landmark on a
/// crest is ringed by slopes.
const TIERS: [Tier; 2] = [
    Tier {
        landing_relief: 0.8,
        landing_slope: 0.15,
        forecourt_relief: 4.0,
    },
    Tier {
        landing_relief: 1.5,
        landing_slope: 0.25,
        forecourt_relief: 6.0,
    },
];

/// One of [`TIERS`].
struct Tier {
    /// The most relief the landing's spot may have (m).
    landing_relief: f32,
    /// The steepest the landing's spot may be at its middle and rim
    /// (rise/run): a body set down on a slope stands tilted with it.
    landing_slope: f32,
    /// The most relief the forecourt's ground may have (m): its gate and
    /// its monument stand on plinths that take up a fall of their own
    /// (#1009).
    forecourt_relief: f32,
}

/// How far the probe's ground stays under a line of sight (m): the full
/// map's ground wanders a metre or two either side of the probe's between
/// its samples, and up to several metres on broken ground.
const SIGHT_MARGIN: f32 = 2.0;

/// How far along a line of sight from either end [`SIGHT_MARGIN`] grows
/// to its whole (m): the line starts at the camera's eye a few metres over
/// the ground and comes down to the landmark's own.
const SIGHT_RAMP: f32 = 15.0;

/// What a site on the gentler of the [`TIERS`] scores under one on the
/// level tier, per tier ([`Viewpoint::score`]).
const TIER_COST: f32 = 1.0;

/// How far off the view's heading a building may stand and still show in
/// the first frame ([`Viewpoint::frames`], rad): 30 degrees, inside the
/// 16:9 frame's half-width of about 36.
const FRAME_HALF: f32 = std::f32::consts::FRAC_PI_6;

/// How far from the camera a building shows in the first frame for
/// anything ([`Viewpoint::frames`], m).
const FRAME_REACH: f32 = 200.0;

/// How high over its ground a building is looked for in the first frame
/// ([`Viewpoint::frames`], m): its first storey.
const FRAMED_UP: f32 = 3.0;

/// Half the width of the view down to the landmark (m): the gate and the
/// monument stand outside it, and the settlement's buildings keep off it.
pub const CORRIDOR_HALF: f32 = 3.5;

/// How far below level the arrival camera may look at the least (rad): its
/// eye 0.6 m over the body at the orbit's rest radius.
const MIN_PITCH: f32 = 0.05;

/// The steps the arrival pitch is lowered in from the orbit's rest (rad).
const PITCH_STEP: f32 = 0.01;

/// How far under the frame's top edge the landmark's top stays (rad, 2
/// degrees).
const TOP_MARGIN: f32 = 0.035;

/// How far up from the ground the body the camera looks at stands (m) -
/// the spawn's own set-down ([`crate::config::rover::SPAWN_HEIGHT_OFFSET`]).
const FOCUS_UP: f32 = crate::config::rover::SPAWN_HEIGHT_OFFSET;

/// A seeded world's arrival (#1777).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewpoint {
    /// Where the visitor lands, centred world XZ.
    pub landing: [f32; 2],
    /// The way the landing looks, a unit vector on the ground: at the
    /// landmark.
    pub toward: [f32; 2],
    /// The arrival camera's pitch, radians below level ([`arrival_pitch`]).
    pub pitch: f32,
    /// How the site scored against the others round its landmark (see the
    /// module docs), [`TIER_COST`] off for each tier gentler than the level
    /// one: what a settlement's layouts compare their arrivals by (#1778).
    pub score: f32,
}

impl Viewpoint {
    /// The viewpoint over the landmark standing at `landmark` on ground of
    /// `radius`, its top `top` metres over that ground, and keeping `clear`
    /// metres clear round it: the best-scored site of the
    /// [`DISTANCE_STEPS`] round each of the 24 bearings that fits (see the
    /// module docs), its forecourt outside that clearance - the first of
    /// equals, on the first of the [`TIERS`] any site passes, `None` where
    /// none fits.
    pub fn choose(
        probe: &TerrainProbe,
        landmark: [f32; 2],
        radius: f32,
        top: f32,
        clear: f32,
    ) -> Option<Self> {
        TIERS.iter().enumerate().find_map(|(gentler, tier)| {
            Self::choose_on(probe, landmark, radius, top, clear, tier).map(|found| Self {
                score: found.score - TIER_COST * gentler as f32,
                ..found
            })
        })
    }

    /// [`Self::choose`] on the ground `tier` allows.
    fn choose_on(
        probe: &TerrainProbe,
        landmark: [f32; 2],
        radius: f32,
        top: f32,
        clear: f32,
        tier: &Tier,
    ) -> Option<Self> {
        let want = (top * VIEW_PER_HEIGHT)
            .max(radius * VIEW_PER_RADIUS)
            .clamp(MIN_VIEW, MAX_VIEW);
        let base = probe.height_between(landmark);
        let edge = probe.half_extent() - FORECOURT - probe.cell_size();
        let mut best: Option<Viewpoint> = None;
        for share in DISTANCE_STEPS {
            let distance = want * share;
            if distance < clear + FORECOURT {
                continue;
            }
            for (k, &out) in RIM.iter().enumerate() {
                let landing = [
                    landmark[0] + out[0] * distance,
                    landmark[1] + out[1] * distance,
                ];
                if landing[0].abs() > edge || landing[1].abs() > edge {
                    continue;
                }
                let spot = probe.footprint(landing, LANDING_SPOT);
                if spot.relief > tier.landing_relief
                    || spot.steepest > tier.landing_slope
                    || !spot.is_dry()
                {
                    continue;
                }
                let ground = probe.footprint(landing, FORECOURT);
                if ground.relief > tier.forecourt_relief || !ground.is_dry() {
                    continue;
                }
                let toward = [-out[0], -out[1]];
                if !dry_way(probe, landing, landmark) {
                    continue;
                }
                let pitch = arrival_pitch(probe, landing, toward, landmark, base + top);
                let eye = camera_eye(probe, landing, toward, pitch);
                if probe.height_between([eye[0], eye[2]]) > eye[1] - 0.5 {
                    continue;
                }
                if !in_sight(probe, eye, landmark, base + top * 0.5, radius) {
                    continue;
                }
                let mut score = -((distance - want).abs() / want) * 2.0;
                score += (probe.height_between(landing) - base).clamp(-10.0, 15.0) * 0.1;
                score -= ground.relief * 0.25 + spot.relief;
                if in_sight(probe, eye, landmark, base + top, radius) {
                    score += 0.5;
                }
                if water_aside(probe, landing, k) {
                    score += 1.0;
                }
                if best.is_none_or(|kept: Viewpoint| score > kept.score) {
                    best = Some(Viewpoint {
                        landing,
                        toward,
                        pitch,
                        score,
                    });
                }
            }
        }
        best
    }

    /// The landing's facing in the spawn's degrees: a pose facing
    /// `(dx, dz)` turns by `atan2(-dx, -dz)` (`player::spawn_facing`).
    pub fn yaw_deg(&self) -> f32 {
        libm::atan2f(-self.toward[0], -self.toward[1]).to_degrees()
    }

    /// Whether a building standing at `at` on ground of `radius` shows in
    /// the arrival's first frame (#1778): no more than [`FRAME_HALF`] off the
    /// view's heading from the camera's eye, within [`FRAME_REACH`] of it,
    /// and in sight of [`FRAMED_UP`] metres over its ground, read as the
    /// landmark is.
    pub fn frames(&self, probe: &TerrainProbe, at: [f32; 2], radius: f32) -> bool {
        self.shows(probe, at, probe.height_between(at) + FRAMED_UP, radius)
    }

    /// Whether the point `y` metres up, over `at` on ground of `radius`,
    /// shows in the arrival's first frame: [`Self::frames`] at the height
    /// asked - a sea stack shows from the water up (#1781).
    pub fn shows(&self, probe: &TerrainProbe, at: [f32; 2], y: f32, radius: f32) -> bool {
        let eye = camera_eye(probe, self.landing, self.toward, self.pitch);
        let (dx, dz) = (at[0] - eye[0], at[1] - eye[2]);
        let far = (dx * dx + dz * dz).sqrt();
        far > radius
            && far <= FRAME_REACH
            && dx * self.toward[0] + dz * self.toward[1] >= far * libm::cosf(FRAME_HALF)
            && in_sight(probe, eye, at, y, radius)
    }

    /// What the settlement keeps clear for it, as `(position, radius)`: the
    /// forecourt's ground, and the view down to the landmark standing at
    /// `landmark` on ground of `radius` - discs [`CORRIDOR_HALF`] round,
    /// overlapping, from the forecourt's edge to the landmark's.
    pub fn keep_outs(&self, landmark: [f32; 2], radius: f32) -> Vec<([f32; 2], f32)> {
        let mut keep = vec![(self.landing, FORECOURT)];
        let (dx, dz) = (landmark[0] - self.landing[0], landmark[1] - self.landing[1]);
        let length = (dx * dx + dz * dz).sqrt();
        let run = length - radius - FORECOURT;
        if run > 0.0 {
            let steps = (run / CORRIDOR_HALF).ceil() as usize;
            for step in 0..=steps {
                let along = FORECOURT + run * step as f32 / steps as f32;
                keep.push((
                    [
                        self.landing[0] + self.toward[0] * along,
                        self.landing[1] + self.toward[1] * along,
                    ],
                    CORRIDOR_HALF,
                ));
            }
        }
        keep
    }
}

/// The arrival camera's eye over a landing at `landing` looking `toward`
/// at `pitch`, `[x, y, z]`: the orbit's rest radius behind the body and up
/// by the pitch, the body [`FOCUS_UP`] over the probe's ground.
fn camera_eye(probe: &TerrainProbe, landing: [f32; 2], toward: [f32; 2], pitch: f32) -> [f32; 3] {
    let radius = crate::config::camera::ORBIT_RADIUS;
    let back = radius * libm::cosf(pitch);
    [
        landing[0] - toward[0] * back,
        probe.height_between(landing) + FOCUS_UP + radius * libm::sinf(pitch),
        landing[1] - toward[1] * back,
    ]
}

/// The pitch the arrival camera looks down at from `landing`, looking
/// `toward` the landmark at `landmark` whose top stands at `top_y` (#1777):
/// the orbit's rest pitch where the top shows there [`TOP_MARGIN`] under
/// the frame's top edge, else a step lower until it does, else
/// [`MIN_PITCH`] - a landmark that tall shows cut. The frame's top edge is
/// half the lens's height above the camera's line of sight.
pub fn arrival_pitch(
    probe: &TerrainProbe,
    landing: [f32; 2],
    toward: [f32; 2],
    landmark: [f32; 2],
    top_y: f32,
) -> f32 {
    let half_lens = crate::camera::perspective_lens().fov * 0.5;
    let mut pitch = crate::config::camera::ORBIT_PITCH;
    while pitch > MIN_PITCH {
        let eye = camera_eye(probe, landing, toward, pitch);
        let (dx, dz) = (landmark[0] - eye[0], landmark[1] - eye[2]);
        let across = (dx * dx + dz * dz).sqrt();
        if libm::atan2f(top_y - eye[1], across) <= half_lens - pitch - TOP_MARGIN {
            return pitch;
        }
        pitch -= PITCH_STEP;
    }
    MIN_PITCH
}

/// Whether the probe's ground stays [`SIGHT_MARGIN`] under the line from
/// `eye` to the point `y` metres up over `target`, short of the target's own
/// ground of `radius` (#1777): read every half cell along the line, the
/// margin drawn in over [`SIGHT_RAMP`] at either end, where the line leaves
/// the camera and comes down to the target.
fn in_sight(probe: &TerrainProbe, eye: [f32; 3], target: [f32; 2], y: f32, radius: f32) -> bool {
    let (dx, dz) = (target[0] - eye[0], target[1] - eye[2]);
    let length = (dx * dx + dz * dz).sqrt();
    let reach = length - radius;
    if reach <= 0.0 {
        return true;
    }
    let steps = (reach / (probe.cell_size() * 0.5)).ceil() as usize;
    (1..=steps).all(|step| {
        let along = reach * step as f32 / steps as f32;
        let t = along / length;
        let at = [eye[0] + dx * t, eye[2] + dz * t];
        let margin = SIGHT_MARGIN * (along.min(reach - along) / SIGHT_RAMP).min(1.0);
        probe.height_between(at) + margin <= eye[1] + (y - eye[1]) * t
    })
}

/// Whether the way from `landing` to `landmark` stays out of the water,
/// read every half cell on the probe (#1777): a visitor walks it.
fn dry_way(probe: &TerrainProbe, landing: [f32; 2], landmark: [f32; 2]) -> bool {
    let (dx, dz) = (landmark[0] - landing[0], landmark[1] - landing[1]);
    let length = (dx * dx + dz * dz).sqrt();
    let steps = (length / (probe.cell_size() * 0.5)).ceil().max(1.0) as usize;
    (0..=steps).all(|step| {
        let t = step as f32 / steps as f32;
        probe.height_between([landing[0] + dx * t, landing[1] + dz * t]) > probe.water_y()
    })
}

/// Whether water lies off the axis of the view from `landing`, which looks
/// back along bearing `out` of [`RIM`] (#1777): along the bearings 15 and 30
/// degrees either side of the view, every 15 m from 30 m to 400 m out, as a
/// second thing in the frame to look at.
fn water_aside(probe: &TerrainProbe, landing: [f32; 2], out: usize) -> bool {
    let view = (out + RIM.len() / 2) % RIM.len();
    [1, 2, RIM.len() - 1, RIM.len() - 2].iter().any(|&turn| {
        let [dx, dz] = RIM[(view + turn) % RIM.len()];
        (2..=26).any(|step| {
            let along = 15.0 * step as f32;
            probe.height_between([landing[0] + dx * along, landing[1] + dz * along])
                <= probe.water_y()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A probe as wide as a seeded world's, in its 10 m cells, over ground
    /// of `height` at world XZ, its water at 0.
    fn probe(height: impl Fn(f32, f32) -> f32) -> TerrainProbe {
        const GRID: usize = 96;
        const CELL: f32 = 10.0;
        let half = (GRID - 1) as f32 * CELL * 0.5;
        let data = (0..GRID * GRID)
            .map(|i| {
                let (x, z) = (i % GRID, i / GRID);
                height(x as f32 * CELL - half, z as f32 * CELL - half)
            })
            .collect();
        let map = gen_jobs::HeightmapData {
            width: GRID as u32,
            height: GRID as u32,
            scale: CELL,
            data,
        };
        TerrainProbe::new(&map, 0.0, crate::seeded_defaults::BUILD_SLOPE_LIMIT)
    }

    fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
        (a[0] - b[0]).hypot(a[1] - b[1])
    }

    /// On level, dry ground the viewpoint stands at the distance its
    /// landmark asks for - [`VIEW_PER_HEIGHT`] times its height here -
    /// looking at it, and its yaw turns the spawn's body that way.
    #[test]
    fn on_level_ground_it_stands_at_the_distance_asked_looking_at_the_landmark() {
        let probe = probe(|_, _| 10.0);
        let landmark = [30.0, -20.0];
        let view = Viewpoint::choose(&probe, landmark, 8.0, 20.0, 10.0).expect("level ground fits");
        let far = distance(view.landing, landmark);
        assert!((far - 20.0 * VIEW_PER_HEIGHT).abs() < 1e-3, "{far} m");
        let to = [
            (landmark[0] - view.landing[0]) / far,
            (landmark[1] - view.landing[1]) / far,
        ];
        assert!(distance(view.toward, to) < 1e-5, "{:?}", view.toward);
        let facing = crate::player::spawn_facing(view.yaw_deg()) * bevy::math::Vec3::NEG_Z;
        assert!(distance([facing.x, facing.z], to) < 1e-4, "{facing}");
        // A landmark the minimum asks more of stands no nearer.
        let small = Viewpoint::choose(&probe, landmark, 2.0, 3.0, 4.0).expect("level ground fits");
        assert!((distance(small.landing, landmark) - MIN_VIEW).abs() < 1e-3);
    }

    /// A ridge across the view on the first-tried bearings (RIM's first
    /// points +Z) turns the viewpoint to one it does not stand behind.
    #[test]
    fn a_ridge_in_the_way_turns_it_to_a_clear_bearing() {
        let open = Viewpoint::choose(&probe(|_, _| 10.0), [0.0, 0.0], 8.0, 20.0, 10.0)
            .expect("level ground fits");
        assert!(open.landing[1] > 50.0, "{:?}", open.landing);
        let ridged = probe(|_, z| {
            if (30.0..=50.0).contains(&z) {
                40.0
            } else {
                10.0
            }
        });
        let view =
            Viewpoint::choose(&ridged, [0.0, 0.0], 8.0, 20.0, 10.0).expect("open ground fits");
        assert!(
            view.landing[1] < 30.0,
            "{:?} looks across the ridge",
            view.landing
        );
    }

    /// A way to the landmark across water is refused: the visitor walks it.
    #[test]
    fn a_way_across_water_is_refused() {
        let river = probe(|_, z| {
            if (20.0..=40.0).contains(&z) {
                -5.0
            } else {
                10.0
            }
        });
        let view = Viewpoint::choose(&river, [0.0, 0.0], 8.0, 20.0, 10.0).expect("dry ground fits");
        assert!(
            view.landing[1] < 20.0,
            "{:?} looks across the river",
            view.landing
        );
        assert!(dry_way(&river, view.landing, [0.0, 0.0]));
        // An island ringed by water has no viewpoint at all.
        let moat = probe(|x, z| {
            if (20.0..=200.0).contains(&x.hypot(z)) {
                -5.0
            } else {
                10.0
            }
        });
        assert_eq!(Viewpoint::choose(&moat, [0.0, 0.0], 8.0, 20.0, 10.0), None);
    }

    /// The gentler tier is the viewpoint only where the level one finds no
    /// site: a slope everywhere steeper than the level tier allows, and no
    /// steeper than the gentler - and its site scores [`TIER_COST`] under the
    /// level tier's.
    #[test]
    fn the_gentler_tier_is_taken_only_where_no_level_ground_is() {
        let slope = probe(|x, _| 100.0 + 0.18 * x);
        let ask = |tier: &Tier| Viewpoint::choose_on(&slope, [0.0, 0.0], 8.0, 20.0, 10.0, tier);
        assert_eq!(ask(&TIERS[0]), None);
        let gentle = ask(&TIERS[1]).expect("the gentler tier fits the slope");
        // Scored a point under a level site (#1778).
        assert_eq!(
            Viewpoint::choose(&slope, [0.0, 0.0], 8.0, 20.0, 10.0),
            Some(Viewpoint {
                score: gentle.score - TIER_COST,
                ..gentle
            })
        );
        let level = probe(|_, _| 10.0);
        assert_eq!(
            Viewpoint::choose(&level, [0.0, 0.0], 8.0, 20.0, 10.0),
            Viewpoint::choose_on(&level, [0.0, 0.0], 8.0, 20.0, 10.0, &TIERS[0])
        );
    }

    /// The keep-outs hold the forecourt and the view down to the
    /// landmark's own ground, and nothing beside the view past the
    /// forecourt.
    #[test]
    fn its_keep_outs_hold_the_forecourt_and_the_view() {
        let view = Viewpoint {
            landing: [0.0, 80.0],
            toward: [0.0, -1.0],
            pitch: 0.3,
            score: 0.0,
        };
        let keep = view.keep_outs([0.0, 0.0], 12.0);
        let kept = |at: [f32; 2]| keep.iter().any(|&(c, r)| distance(at, c) <= r);
        for [dx, dz] in RIM {
            let at = [dx * (FORECOURT - 0.5), 80.0 + dz * (FORECOURT - 0.5)];
            assert!(kept(at), "{at:?} in the forecourt is not kept");
        }
        // Discs a corridor's half apart hold all but the scallops at its
        // edges between them.
        for step in 0..=136 {
            let z = 80.0 - step as f32 * 0.5;
            for x in [-0.8, 0.0, 0.8].map(|share| share * CORRIDOR_HALF) {
                assert!(kept([x, z]), "({x}, {z}) on the view is not kept");
            }
        }
        for z in [60.0, 40.0, 20.0] {
            for x in [CORRIDOR_HALF + 1.0, -CORRIDOR_HALF - 1.0] {
                assert!(!kept([x, z]), "({x}, {z}) beside the view is kept");
            }
        }
    }

    /// The arrival pitch is the highest of the orbit's rest and the steps
    /// under it that shows the landmark's top under the frame's top edge,
    /// the lower the taller the landmark, and [`MIN_PITCH`] where none does.
    #[test]
    fn the_arrival_pitch_shows_the_top_under_the_frame() {
        let probe = probe(|_, _| 10.0);
        let (landing, toward, landmark) = ([0.0, 70.0], [0.0, -1.0], [0.0, 0.0]);
        let half_lens = crate::camera::perspective_lens().fov * 0.5;
        let shows = |pitch: f32, top_y: f32| {
            let eye = camera_eye(&probe, landing, toward, pitch);
            libm::atan2f(top_y - eye[1], eye[2] - landmark[1]) <= half_lens - pitch - TOP_MARGIN
        };
        let rest = crate::config::camera::ORBIT_PITCH;
        let mut last = rest;
        for top in [0.5, 5.0, 12.0, 20.0, 60.0] {
            let pitch = arrival_pitch(&probe, landing, toward, landmark, 10.0 + top);
            assert!(pitch <= last, "{top} m: {pitch} over {last}");
            if pitch > MIN_PITCH {
                assert!(
                    shows(pitch, 10.0 + top),
                    "{top} m: the top is cut at {pitch}"
                );
            }
            if pitch < rest {
                assert!(
                    !shows(pitch + PITCH_STEP, 10.0 + top),
                    "{top} m: {pitch} is lower than it needs"
                );
            }
            last = pitch;
        }
        assert_eq!(arrival_pitch(&probe, landing, toward, landmark, 10.5), rest);
        assert_eq!(
            arrival_pitch(&probe, landing, toward, landmark, 400.0),
            MIN_PITCH
        );
    }

    /// The line of sight clears the probe's ground by [`SIGHT_MARGIN`] along
    /// its middle, the margin drawn in where it comes down to the target.
    #[test]
    fn a_line_of_sight_keeps_its_margin_but_at_its_ends() {
        let row =
            |at: f32, high: f32| probe(move |_, z| if (z - at).abs() < 1.0 { high } else { 10.0 });
        let (eye, target) = ([5.0, 16.0, 95.0], [5.0, 5.0]);
        assert!(in_sight(&row(45.0, 13.5), eye, target, 16.0, 0.0));
        assert!(!in_sight(&row(45.0, 15.0), eye, target, 16.0, 0.0));
        assert!(!in_sight(&row(45.0, 17.0), eye, target, 16.0, 0.0));
        // Five metres short of the target's ground, a third of the margin.
        assert!(in_sight(&row(15.0, 15.0), eye, target, 16.0, 5.0));
    }

    /// #1778: a building shows in the first frame ahead of the camera, in
    /// sight and within reach - not behind it, off to the side, past its
    /// reach, nor behind a ridge.
    #[test]
    fn a_building_ahead_and_in_sight_shows_in_the_first_frame() {
        let level = probe(|_, _| 10.0);
        let view = Viewpoint {
            landing: [0.0, 80.0],
            toward: [0.0, -1.0],
            pitch: 0.3,
            score: 0.0,
        };
        assert!(view.frames(&level, [10.0, 20.0], 5.0), "ahead");
        assert!(!view.frames(&level, [0.0, 120.0], 5.0), "behind");
        assert!(!view.frames(&level, [80.0, 60.0], 5.0), "off to the side");
        assert!(!view.frames(&level, [0.0, -180.0], 5.0), "past its reach");
        let ridged = probe(|_, z| {
            if (30.0..=50.0).contains(&z) {
                40.0
            } else {
                10.0
            }
        });
        assert!(!view.frames(&ridged, [0.0, 0.0], 5.0), "behind a ridge");
    }

    /// The forecourt (#1777), for every theme's own gate and monument: the
    /// two stand either side of the view, their footprints outside its
    /// corridor, inside the forecourt and clear of each other, each turned
    /// to face the landing, and the landing looks along the view.
    #[test]
    fn every_themes_gate_and_monument_flank_the_view() {
        use super::super::{GatewaySpot, MonumentSpot, gateway, monument};
        for theme in crate::seeded_defaults::ThemeArchetype::ALL {
            let (Some(gate), Some(stone)) = (
                gateway::gateway_entry(theme),
                monument::monument_entry(theme),
            ) else {
                continue;
            };
            let (gate_clear, stone_clear) =
                (gate.footprint().clearance, stone.footprint().clearance);
            for k in [0, 5, 13, 22] {
                let [ox, oz] = RIM[k];
                let view = Viewpoint {
                    landing: [40.0 + ox * 70.0, -10.0 + oz * 70.0],
                    toward: [-ox, -oz],
                    pitch: 0.3,
                    score: 0.0,
                };
                for side in [false, true] {
                    let gate = GatewaySpot::at_viewpoint(&view, gate_clear, side);
                    let stone = MonumentSpot::at_viewpoint(&view, stone_clear, side);
                    assert_eq!(gate.landing, view.landing);
                    assert_eq!(gate.landing_yaw_deg, view.yaw_deg());
                    let aside = |at: [f32; 2]| {
                        let (dx, dz) = (at[0] - view.landing[0], at[1] - view.landing[1]);
                        dx * view.toward[1] - dz * view.toward[0]
                    };
                    let (g, s) = (aside(gate.offset), aside(stone.offset));
                    let tag = format!("{theme:?} bearing {k} side {side}");
                    assert!(
                        g.abs() >= CORRIDOR_HALF + gate_clear - 1e-3,
                        "{tag}: gate {g}"
                    );
                    assert!(
                        s.abs() >= CORRIDOR_HALF + stone_clear - 1e-3,
                        "{tag}: monument {s}"
                    );
                    assert!(g * s < 0.0, "{tag}: both on one side");
                    assert!(
                        distance(gate.offset, stone.offset) > gate_clear + stone_clear,
                        "{tag}"
                    );
                    for (at, clear, yaw) in [
                        (gate.offset, gate_clear, gate.yaw_rad),
                        (stone.offset, stone_clear, stone.yaw_rad),
                    ] {
                        let off = distance(at, view.landing);
                        assert!(
                            off + clear <= FORECOURT,
                            "{tag}: {off} m out, {clear} m round"
                        );
                        let forward = [-yaw.sin(), -yaw.cos()];
                        let to = [
                            (view.landing[0] - at[0]) / off,
                            (view.landing[1] - at[1]) / off,
                        ];
                        assert!(
                            forward[0] * to[0] + forward[1] * to[1] > 0.999,
                            "{tag}: turned away"
                        );
                    }
                }
            }
        }
    }
}
