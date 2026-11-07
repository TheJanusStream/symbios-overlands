//! A rigged winged body in the air (#1877): what its chassis' flight
//! ([`crate::player::Flight`], the prototype's arcade model of #1861) hands
//! the engine's airborne motion ([`Drive::flight`](bevy_symbios_avatar::Drive::flight),
//! the engine's `Inputs::flight`), frame by frame.
//!
//! **The modes map one to one**, as the engine's docs table them
//! (docs/winged-anatomy.md, "From Overlands' FlightMode"): `Ground` is no
//! flight at all, every other mode its namesake. The engine's own `Soar` is
//! not flown. The air past the body is `None` - the engine reads the
//! driver's velocity, which is the same thing in still air.
//!
//! **What only the owner's client knows goes only from it.** The beat's
//! phase is handed in `Flap` alone, off the chassis' own count of beats
//! ([`Flight::beat_phase`]) half a downstroke on: the chassis pushes on each
//! beat, and the engine's `Flight::phase` asks a host that pushes at
//! mid-downstroke for half its downstroke there - handed the count alone,
//! every push was drawn at the top of a downstroke, the wings still raised.
//! In every other mode, and on every peer, the engine keeps its own clock.
//! The time to contact is handed in `Flare`
//! alone ([`Flight::contact`]): the engine flares whatever the mode says once
//! contact is under its 0.45 s, and the chassis does not flare while its flap
//! key is held, so handed in `Flap` or `Glide` a pass over a roof with the
//! key held was posed as a flare.
//!
//! **Handing on is the engine's to say** (the engine's flight module,
//! "Handing on"), so no duration is guessed here:
//!
//! * **A takeoff into flapping** once the engine says the feet are off and
//!   its beat has grown by two since ([`symbios_avatar::Flown::lifted`]),
//!   while the chassis is still in its take-off - and at once to whatever the
//!   chassis flies next if that comes sooner.
//! * **A takeoff from a held landing** after a frame of no flight at all: the
//!   engine plays a takeoff's crouch and push only into a flight it starts
//!   afresh, so Space while the landing below is held, turned straight into
//!   a takeoff, left the ground with neither.
//! * **A landing back to the ground** only once the engine says it has
//!   settled ([`symbios_avatar::Flown::settled`]): the chassis is on its feet
//!   a quarter of a second after it lands (`flight::LAND_S`), and the
//!   engine's absorb and fold take about twice that, so handed back with the
//!   chassis every landing was cut mid-fold. The hold is let go at once by a
//!   body set off walking (faster than the engine's idle over the slowest it
//!   has gone since - the landing's own slide, still braking as the chassis
//!   goes to its feet, only slows - so walking out of a landing does not
//!   skate), by water, and by a body set down somewhere -
//!   a respawn, whose chassis goes to its feet from the air without landing,
//!   or a placement.
//!
//! **A peer's mode is taken as its body is drawn there**, the playout's delay
//! after it was heard ([`PeerFlight::next_due`]), one word a frame.

use symbios_avatar::{DriverConfig, Flown};

use crate::pds::FlightParams;
use crate::player::flight::PeerWord;
use crate::player::{Flight, FlightMode, PeerFlight};

/// The engine's own names, beside this app's.
type Asked = symbios_avatar::Flight;
type Posed = symbios_avatar::FlightMode;

/// How far the engine's beat must have grown since the feet left before a
/// takeoff is handed on into flapping (the engine's "Handing on": two of the
/// takeoff's big beats).
const TAKEOFF_BEATS: f32 = 2.0;

/// The engine's beat clock wraps every this many beats ([`Flown::phase`]).
const BEAT_CLOCK: f32 = 64.0;

/// The share of a beat its downstroke takes, before the engine has said
/// ([`Flown::downstroke`]): the engine's own for a body it cannot read.
const DOWNSTROKE_UNSAID: f32 = 0.5;

/// What a winged body remembers of its flight from frame to frame (#1877),
/// kept in its [`super::RiggedTrail`] and carried across a rebuild with it.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Aloft {
    /// The chassis' mode as last read: the owner's own, or a peer's as
    /// played out.
    chassis: FlightMode,
    /// What the engine was handed last frame.
    handed: Option<Posed>,
    /// The engine's beat clock on the frame its takeoff lifted, while the
    /// chassis is in its take-off.
    lifted_at: Option<f32>,
    /// The takeoff handed on into flapping, while the chassis is still in
    /// its take-off.
    handed_on: bool,
    /// The last of a peer's words taken ([`PeerFlight`]).
    taken: Option<PeerWord>,
    /// The slowest the body has gone over the ground since its landing was
    /// held (m/s), while it is.
    slowest: Option<f32>,
}

/// What a body's chassis says of its flight this frame (#1877).
pub(super) struct Said<'a> {
    /// The owner's own flight, on the owner's client.
    pub(super) local: Option<&'a Flight>,
    /// A peer's words, on everyone else's.
    pub(super) peer: Option<&'a PeerFlight>,
    /// The record's flight section, if it has one.
    pub(super) params: Option<&'a FlightParams>,
    /// The app's elapsed seconds.
    pub(super) now: f64,
}

/// What a body is doing on the ground this frame that lets a landing go
/// (#1877).
pub(super) struct Footing {
    /// Its speed over the ground (m/s).
    pub(super) planar: f32,
    /// In water deep enough to swim.
    pub(super) swimming: bool,
    /// Set down somewhere this frame rather than moved there.
    pub(super) placed: bool,
}

impl Aloft {
    /// The mode a peer's body plays this frame: the next of its words due,
    /// if one is, or the one it played last - on its feet with no word.
    fn played(&mut self, peer: Option<&PeerFlight>, now: f64) -> FlightMode {
        let Some(peer) = peer else {
            self.taken = None;
            return FlightMode::Ground;
        };
        match peer.next_due(self.taken, now) {
            Some(word) => {
                self.taken = Some(word);
                word.mode
            }
            None => self.chassis,
        }
    }

    /// What the engine is handed this frame, from what the chassis says,
    /// what the engine `flown` last frame and the body's `footing` - see the
    /// module doc. Asked only of a body with wings.
    pub(super) fn feed(
        &mut self,
        said: Said<'_>,
        flown: Option<Flown>,
        footing: Footing,
    ) -> Option<Asked> {
        let chassis = match said.local {
            Some(flight) => flight.mode(),
            None => self.played(said.peer, said.now),
        };
        let was = std::mem::replace(&mut self.chassis, chassis);
        if chassis != FlightMode::Ground {
            self.slowest = None;
        }
        if chassis != FlightMode::Takeoff {
            self.lifted_at = None;
            self.handed_on = false;
        }
        let flaps = said.params.is_none_or(FlightParams::can_flap);
        let cruise = if flaps { Posed::Flap } else { Posed::Glide };
        let mode = match chassis {
            FlightMode::Ground => {
                // Landed by the chassis, not set down from the air.
                let landing = self.handed == Some(Posed::Land)
                    && matches!(was, FlightMode::Land | FlightMode::Ground);
                let settled = flown.is_none_or(|flown| flown.settled);
                // Set off walking: faster than the idle over the slowest it
                // has gone since it was held - a landing's own slide, which
                // the chassis is still braking off as it goes to its feet,
                // only slows.
                let slowest = self
                    .slowest
                    .map_or(footing.planar, |was| was.min(footing.planar));
                let walking = footing.planar > slowest + DriverConfig::default().idle_below;
                let let_go = walking || footing.swimming || footing.placed;
                let held = landing && !settled && !let_go;
                self.slowest = held.then_some(slowest);
                held.then_some(Posed::Land)
            }
            // Taken off while its landing is held: a frame of no flight, so
            // the engine's next is a fresh takeoff, crouch and push.
            FlightMode::Takeoff
                if was == FlightMode::Ground && self.handed == Some(Posed::Land) =>
            {
                None
            }
            FlightMode::Takeoff => {
                if let Some(flown) =
                    flown.filter(|flown| flown.mode == Posed::Takeoff && flown.lifted)
                {
                    let at = *self.lifted_at.get_or_insert(flown.phase);
                    self.handed_on |= (flown.phase - at).rem_euclid(BEAT_CLOCK) >= TAKEOFF_BEATS;
                }
                Some(if self.handed_on {
                    cruise
                } else {
                    Posed::Takeoff
                })
            }
            FlightMode::Flap => Some(Posed::Flap),
            FlightMode::Glide => Some(Posed::Glide),
            FlightMode::Dive => Some(Posed::Dive),
            FlightMode::Flare => Some(Posed::Flare),
            FlightMode::Land => Some(Posed::Land),
        };
        self.handed = mode;
        mode.map(|mode| Asked {
            mode,
            air: None,
            phase: said
                .local
                .filter(|_| mode == Posed::Flap && chassis == FlightMode::Flap)
                .zip(said.params)
                .map(|(flight, params)| {
                    let downstroke = flown.map_or(DOWNSTROKE_UNSAID, |flown| flown.downstroke);
                    (flight.beat_phase(params.wingbeat_rate.0) + 0.5 * downstroke)
                        .rem_euclid(BEAT_CLOCK)
                }),
            contact: said
                .local
                .filter(|_| mode == Posed::Flare)
                .and_then(Flight::contact),
        })
    }
}
