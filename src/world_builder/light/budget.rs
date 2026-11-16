//! Which authored lights are drawn this frame (#1921): the nearest, as many
//! as the count allows and WebGL2's cluster index list holds, with room
//! between letting a light in and putting one out, so lights do not trade
//! places or snap off as the camera sways and turns.
//!
//! [`Planner::plan`] gives each light a [`Verdict`]. The list's places are
//! counted as Bevy counts them ([`ClusterView::count`]), from the pose Bevy
//! clusters with (the drawing runs after propagation), at both ends of the
//! last depth slice a drawn set meets: where Bevy ends it THIS frame - at
//! the farthest of the lights drawn last frame, from where the camera stood
//! then, which the planner remembers - and where it will end it next frame,
//! at the farthest of the lights drawn now. A light far ahead stretches the
//! slices and makes every light cheaper, but only from the frame after it
//! is drawn; costed at the dearer end of the two, a set that fits now fits
//! then too. Three lines:
//!
//! - [`ADMIT_LIMIT`]: a light not drawn yet is let in only while the lights
//!   drawn - those fading out among them - and it stay under this.
//! - [`KEEP_LIMIT`]: a wanted light stays wanted while the nearer lights
//!   and it stay under this, a band above the first, so a light let in is
//!   not sent away by the next frame's sway. Past it the farthest fade out,
//!   and while what is drawn stays past it they fade quickly
//!   ([`Verdict::Hurry`]), so lights turned into view find room.
//! - [`CUT_LIMIT`]: what is drawn - fading lights too - must stay under
//!   this, with room under [`INDEX_LIST_LEN`] for what the count cannot see
//!   coming (a light's node shown or hidden the same frame). Only past it
//!   is a light put out at once ([`Verdict::Cut`]): a light waiting to come
//!   in first, then the lights fading out, then the one that frees the most
//!   places for its distance. A fade does the work wherever it keeps the
//!   list whole; a cut is for a camera turned toward more light than the
//!   list holds.
//!
//! A light out of view takes no place, but it will as the camera turns:
//! once a light in view does not fit, one out of view that ranks after it
//! is not wanted (unless wanted already), so what waits behind the camera
//! is no more than the view would hold, and the count is not filled with
//! lights that would overflow the list as they came into view.
//!
//! A light already wanted ranks as if a share nearer ([`HOLD_SHARE`]), so
//! two at nearly one distance do not trade places either. Only the nearest
//! few lights are ranked at all: a set of the nearest twice the count, and
//! what is drawn, is sorted, the rest left where they are.

use bevy::math::Vec3;

#[cfg(doc)]
use super::clusters::INDEX_LIST_LEN;
use super::clusters::{ClusterView, Slices};

/// What is drawn - lights fading out too - must take no more of WebGL2's
/// cluster index list than this ([`INDEX_LIST_LEN`] is 16,384): the rest is
/// room for what the count cannot see coming, and for an embedder that
/// runs the drawing a frame behind the camera (the critic's simulation of
/// that kept every scene under the list too, #1921).
pub const CUT_LIMIT: u32 = 15_200;

/// A drawn light stays wanted while the nearer lights and it take no more
/// than this.
pub const KEEP_LIMIT: u32 = 12_800;

/// A light not drawn yet is let in while what is drawn and it take no more
/// than this.
pub const ADMIT_LIMIT: u32 = 12_000;

/// How long a light hurried out takes to fade (s), against a whole fade's
/// 0.6 s: quick enough that lights turned into view find room before the
/// list is past [`CUT_LIMIT`], slow enough to read as a fade.
pub const HURRY_FADE_S: f32 = 0.15;

/// A wanted light ranks as if this share of its distance away, so another
/// must be this much nearer to take its place.
pub const HOLD_SHARE: f32 = 0.9;

/// How many lights past the count are ranked: the nearest this many times
/// the count, enough for the count to be filled when the nearest are
/// too costly to draw.
const RANKED_PER_PLACE: usize = 2;

/// How many lights in view may fail to fit before the wanted set is
/// closed: the nearest were tried, and a farther one that would still fit
/// is a cheap one the count can do without.
const MISSES: usize = 8;

/// The most times the wanted set is chosen again for the far end it sets.
const FAR_ROUNDS: usize = 8;

/// One light as [`Planner::plan`] sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    /// Where it stands from the world camera, in camera space.
    pub view: Vec3,
    /// Bevy's range (m).
    pub range: f32,
    /// On enough to show: lit (at night, if dusk to dawn) and not hidden
    /// with its node.
    pub eligible: bool,
    /// The budget wanted it last frame.
    pub held: bool,
    /// Drawn now: faded in at all and showing.
    pub drawn: bool,
}

/// What the budget does with one light this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Draw it, fading it in if it is not in yet.
    Draw,
    /// Fade it out (or keep it out).
    Fade,
    /// Fade it out quickly, over [`HURRY_FADE_S`]: what is drawn is past
    /// [`KEEP_LIMIT`], and lights come into view faster than a whole fade
    /// would leave them room.
    Hurry,
    /// Put it out at once: the camera turned toward more light than the
    /// cluster index list holds, and a fade would overflow it meanwhile.
    Cut,
}

/// A count not taken yet.
const UNKNOWN: u32 = u32::MAX;

/// Each light's places in the list, counted once a frame for each far end
/// asked about, and its reach.
#[derive(Debug, Default)]
struct Costs {
    /// Where each light's sphere reaches: `NaN` not asked yet, `-inf` out of
    /// the view.
    reach: Vec<f32>,
    /// Places at this frame's far end.
    now: Vec<u32>,
    now_slices: Slices,
    /// Places at the far end asked about last.
    then: Vec<u32>,
    then_slices: Slices,
    then_far: f32,
    /// The lights with a count in `then`.
    asked: Vec<usize>,
}

impl Costs {
    fn reset(&mut self, len: usize) {
        self.reach.clear();
        self.reach.resize(len, f32::NAN);
        self.now.clear();
        self.now.resize(len, UNKNOWN);
        self.then.clear();
        self.then.resize(len, UNKNOWN);
        self.then_far = f32::NAN;
        self.asked.clear();
    }

    fn reach(&mut self, slots: &[Slot], view: &ClusterView, i: usize) -> f32 {
        if self.reach[i].is_nan() {
            self.reach[i] = view
                .reach(slots[i].view, slots[i].range)
                .unwrap_or(f32::NEG_INFINITY);
        }
        self.reach[i]
    }

    /// Where Bevy ends the slices after drawing `set`: its farthest reach
    /// in view, 0 for none.
    fn end(&mut self, slots: &[Slot], view: &ClusterView, set: &[usize]) -> f32 {
        set.iter()
            .map(|&i| self.reach(slots, view, i))
            .fold(0.0, f32::max)
    }

    fn set_now(&mut self, view: &ClusterView, far: f32) {
        self.now_slices = view.slices(far);
    }

    /// Count from now on at the far end `far` as well.
    fn set_then(&mut self, view: &ClusterView, far: f32) {
        if far == self.then_far {
            return;
        }
        self.then_far = far;
        self.then_slices = view.slices(far);
        for &i in &self.asked {
            self.then[i] = UNKNOWN;
        }
        self.asked.clear();
    }

    /// Light `i`'s places at the dearer of the two far ends.
    fn cost(&mut self, slots: &[Slot], view: &ClusterView, i: usize) -> u32 {
        if self.reach(slots, view, i) == f32::NEG_INFINITY {
            return 0;
        }
        let Slot {
            view: at, range, ..
        } = slots[i];
        if self.now[i] == UNKNOWN {
            self.now[i] = view.count(&self.now_slices, at, range);
        }
        if self.then[i] == UNKNOWN {
            self.then[i] = view.count(&self.then_slices, at, range);
            self.asked.push(i);
        }
        self.now[i].max(self.then[i])
    }

    fn total(&mut self, slots: &[Slot], view: &ClusterView, set: &[usize]) -> u32 {
        set.iter().map(|&i| self.cost(slots, view, i)).sum()
    }
}

/// The budget's working memory, kept from frame to frame so a frame
/// allocates nothing once the lists have grown.
#[derive(Debug, Default)]
pub struct Planner {
    verdicts: Vec<Verdict>,
    costs: Costs,
    /// (squared rank, light) of the lights ranked.
    keys: Vec<(f32, u32)>,
    /// The lights ranked, nearest first.
    order: Vec<usize>,
    drawn: Vec<usize>,
    wanted: Vec<usize>,
    live: Vec<usize>,
    /// Newcomers let in this frame, nearest first.
    admitted: Vec<usize>,
    /// Where what this planner left drawn last frame reached, as Bevy saw
    /// it: where Bevy ends this frame's slices.
    last_end: Option<f32>,
}

/// The order lights rank in: by distance, a wanted one [`HOLD_SHARE`]
/// nearer (squared, as the keys are), and by place in the list on a tie.
fn ranked(a: &(f32, u32), b: &(f32, u32)) -> std::cmp::Ordering {
    a.0.total_cmp(&b.0).then(a.1.cmp(&b.1))
}

fn rank_key(slot: &Slot) -> f32 {
    let d = slot.view.length_squared();
    if slot.held {
        d * HOLD_SHARE * HOLD_SHARE
    } else {
        d
    }
}

impl Planner {
    /// What the budget does with each slot: at most `count` lights drawn,
    /// within the cluster index list of `view` - see the module docs.
    pub fn plan(&mut self, slots: &[Slot], count: usize, view: &ClusterView) -> &[Verdict] {
        let len = slots.len();
        self.verdicts.clear();
        self.verdicts.resize(len, Verdict::Fade);
        self.costs.reset(len);
        self.drawn.clear();
        self.drawn.extend((0..len).filter(|&i| slots[i].drawn));

        // Bevy ends this frame's slices where what it drew last frame
        // reached, from where the camera stood then (on a first call, from
        // where it stands now).
        let now = match self.last_end {
            Some(end) => end,
            None => self.costs.end(slots, view, &self.drawn),
        };
        self.costs.set_now(view, now);

        // The nearest few, ranked.
        self.keys.clear();
        if count > 0 {
            self.keys.extend(
                (0..len)
                    .filter(|&i| slots[i].eligible && !slots[i].drawn)
                    .map(|i| (rank_key(&slots[i]), i as u32)),
            );
            let ranked_len = RANKED_PER_PLACE * count;
            if self.keys.len() > ranked_len {
                self.keys.select_nth_unstable_by(ranked_len, ranked);
                self.keys.truncate(ranked_len);
            }
            self.keys.extend(
                self.drawn
                    .iter()
                    .filter(|&&i| slots[i].eligible)
                    .map(|&i| (rank_key(&slots[i]), i as u32)),
            );
            self.keys.sort_unstable_by(ranked);
        }
        self.order.clear();
        self.order
            .extend(self.keys.iter().map(|&(_, i)| i as usize));

        // The wanted set: the nearest that fit, chosen again for the far
        // end it sets until the two agree (a nearer end costs more and fits
        // no more, so it settles).
        let first = &self.order[..count.min(self.order.len())];
        let mut far = self
            .costs
            .end(slots, view, &self.drawn)
            .max(self.costs.end(slots, view, first));
        for _ in 0..FAR_ROUNDS {
            self.costs.set_then(view, far);
            self.wanted.clear();
            let mut spent = 0;
            // Once a light in view does not fit, one out of view that ranks
            // after it is not wanted either - it would not fit as it came
            // into view - and after a few misses nothing more is asked. One
            // out of view that is fading out goes on fading: brought back
            // there, it would only be sent away again as it came into view.
            let mut misses = 0;
            for k in 0..self.order.len() {
                if self.wanted.len() >= count || misses >= MISSES {
                    break;
                }
                let i = self.order[k];
                let in_view = self.costs.reach(slots, view, i) != f32::NEG_INFINITY;
                if !in_view && !slots[i].held && (misses > 0 || slots[i].drawn) {
                    continue;
                }
                let c = self.costs.cost(slots, view, i);
                let limit = if slots[i].held {
                    KEEP_LIMIT
                } else {
                    ADMIT_LIMIT
                };
                if spent + c <= limit {
                    spent += c;
                    self.wanted.push(i);
                } else {
                    misses += 1;
                }
            }
            let end = self.costs.end(slots, view, &self.drawn).max(self.costs.end(
                slots,
                view,
                &self.wanted,
            ));
            if end >= far {
                break;
            }
            far = end;
        }

        // What is drawn stays drawn this frame, those fading out too; a
        // wanted newcomer comes in while they leave it room.
        let far = self.costs.end(slots, view, &self.drawn);
        self.costs.set_then(view, far);
        let mut spent = self.costs.total(slots, view, &self.drawn);
        let mut lit = self.drawn.len();
        self.admitted.clear();
        let mut waiting = false;
        for k in 0..self.wanted.len() {
            let i = self.wanted[k];
            if slots[i].drawn {
                self.verdicts[i] = Verdict::Draw;
                continue;
            }
            let c = self.costs.cost(slots, view, i);
            if waiting || lit >= count || spent + c > ADMIT_LIMIT {
                waiting = true;
                continue;
            }
            lit += 1;
            spent += c;
            self.verdicts[i] = Verdict::Draw;
            self.admitted.push(i);
        }

        // The guard: what will be drawn, at both ends of its slices, within
        // the list - else a newcomer waits, or a drawn light is cut.
        loop {
            self.live.clear();
            self.live.extend(
                self.drawn
                    .iter()
                    .copied()
                    .filter(|&i| self.verdicts[i] != Verdict::Cut),
            );
            self.live.extend_from_slice(&self.admitted);
            let far = self.costs.end(slots, view, &self.live);
            self.costs.set_then(view, far);
            if self.costs.total(slots, view, &self.live) <= CUT_LIMIT {
                break;
            }
            if let Some(i) = self.admitted.pop() {
                self.verdicts[i] = Verdict::Fade;
                continue;
            }
            // The light fading out already, else the one that frees the
            // most places for how near it is.
            let mut victim = None;
            let mut best = (false, 0.0);
            for k in 0..self.live.len() {
                let i = self.live[k];
                let c = self.costs.cost(slots, view, i);
                if c == 0 {
                    continue;
                }
                let score = (
                    self.verdicts[i] == Verdict::Fade,
                    c as f32 * slots[i].view.length(),
                );
                let ahead = match (score.0, best.0) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => score.1 > best.1,
                };
                if victim.is_none() || ahead {
                    victim = Some(i);
                    best = score;
                }
            }
            let Some(i) = victim else {
                break;
            };
            self.verdicts[i] = Verdict::Cut;
        }
        self.last_end = Some(self.costs.end(slots, view, &self.live));
        // Past the keeping line, what is leaving leaves quickly.
        if self.costs.total(slots, view, &self.live) > KEEP_LIMIT {
            for &i in &self.live {
                if self.verdicts[i] == Verdict::Fade {
                    self.verdicts[i] = Verdict::Hurry;
                }
            }
        }
        &self.verdicts
    }

    /// The places the lights drawn after `verdicts` were given take, at the
    /// dearer of this frame's far end and the next - what the guard
    /// holds under [`CUT_LIMIT`]. For tests and measurement.
    pub fn drawn_cost(slots: &[Slot], verdicts: &[Verdict], view: &ClusterView) -> u32 {
        let live: Vec<usize> = (0..slots.len())
            .filter(|&i| match verdicts[i] {
                Verdict::Draw => true,
                Verdict::Fade | Verdict::Hurry => slots[i].drawn,
                Verdict::Cut => false,
            })
            .collect();
        let drawn: Vec<usize> = (0..slots.len()).filter(|&i| slots[i].drawn).collect();
        let mut costs = Costs::default();
        costs.reset(slots.len());
        let now = costs.end(slots, view, &drawn);
        costs.set_now(view, now);
        let then = costs.end(slots, view, &live);
        costs.set_then(view, then);
        costs.total(slots, view, &live)
    }
}

/// [`Planner::plan`] with a planner of its own: for tests and the bench.
pub fn plan(slots: &[Slot], count: usize, view: &ClusterView) -> Vec<Verdict> {
    Planner::default().plan(slots, count, view).to_vec()
}
