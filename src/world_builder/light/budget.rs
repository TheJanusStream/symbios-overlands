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
//!
//! # A body's lights go together (#1925)
//!
//! The lights on one avatar's body, or on one prop it wears, are one
//! group ([`Slot::group`]): a car's headlights and tail lights together. A
//! group ranks by its nearest light, takes as much of the count as it has
//! lights and as much of the list as they cost together, and is let in
//! only when all of it fits; held, kept, faded out and cut, it is held,
//! kept, faded out and cut whole. So a body near the count's end is lit
//! whole or not at all - never one lamp of a pair, which ranked alone
//! would fall either side of the cut half a metre apart. A room's lights
//! go one by one, and so does a group that could not come in whole even
//! to an empty count and list this frame - more lights than the count,
//! or costing more than its line (an owner's three 20 m lamps round their
//! own avatar, the eye inside each): its nearest lights are drawn, as an
//! ungrouped light's are, rather than none.
//!
//! # Counting at many far ends (#1926)
//!
//! The planner asks about several far ends a frame - this frame's, the
//! wanted set's as it settles, what is drawn, what will be - and each
//! light's places at each are counted once a frame. Each far end's slices
//! are kept in a small pool with the cluster spheres they have made
//! ([`Slices`]), from frame to frame too: this frame's far end is the one
//! the last frame settled on ([`Planner::settle`]), so its spheres are made
//! already. The wanted set is first chosen at the far end of what is drawn,
//! where the newcomers and the guard are counted too, so a frame whose
//! drawn set stays as it was counts at two far ends, not four.

use bevy::math::Vec3;

#[cfg(doc)]
use super::clusters::INDEX_LIST_LEN;
use super::clusters::{ClusterView, Cone, Slices};

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

/// How many far ends' slices the planner keeps, with their cluster spheres
/// and the frame's counts at each: enough for the far ends of a frame and
/// the one the next frame starts at.
const ENDS: usize = 8;

/// One light as [`Planner::plan`] sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    /// Where it stands from the world camera, in camera space.
    pub view: Vec3,
    /// Bevy's range (m).
    pub range: f32,
    /// A spot light's cone, which Bevy culls its clusters by too; `None`
    /// for a point light.
    pub cone: Option<Cone>,
    /// The body or worn prop the light hangs on, by any key the caller
    /// gives each (#1925): the lights of one group are drawn all or none -
    /// see the module docs. `None` for a light that goes alone, as a
    /// room's do.
    pub group: Option<u64>,
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

/// One far end's slices, and each light's places under them this frame.
#[derive(Debug, Default)]
struct End {
    slices: Slices,
    /// Places by light, [`UNKNOWN`] for one not counted this frame.
    counts: Vec<u32>,
    /// The lights with a count, to forget them at the next frame.
    asked: Vec<usize>,
    /// When last asked for, on [`Costs::tick`]'s count.
    used: u64,
}

impl End {
    fn forget(&mut self, len: usize) {
        for &i in &self.asked {
            self.counts[i] = UNKNOWN;
        }
        self.asked.clear();
        self.counts.resize(len, UNKNOWN);
    }
}

/// Each light's places in the list, counted once a frame for each far end
/// asked about, and its reach.
#[derive(Debug, Default)]
struct Costs {
    /// Where each light's sphere reaches: `NaN` not asked yet, `-inf` out of
    /// the view.
    reach: Vec<f32>,
    /// The far ends asked about, at most [`ENDS`], kept from frame to frame.
    ends: Vec<End>,
    /// This frame's far end, and the one asked about last, in `ends`.
    now: usize,
    then: usize,
    tick: u64,
}

impl Costs {
    fn reset(&mut self, len: usize) {
        self.reach.clear();
        self.reach.resize(len, f32::NAN);
        for end in &mut self.ends {
            end.forget(len);
        }
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

    /// The pool's slices for the far end `far`, laid out for it if none
    /// are - in the store asked for longest ago, never this frame's.
    fn end_for(&mut self, view: &ClusterView, far: f32) -> usize {
        self.tick += 1;
        let k = match self
            .ends
            .iter()
            .position(|end| view.laid_out(&end.slices, far))
        {
            Some(k) => k,
            None => {
                let k = if self.ends.len() < ENDS {
                    self.ends.push(End::default());
                    self.ends.len() - 1
                } else {
                    (0..self.ends.len())
                        .filter(|&k| k != self.now)
                        .min_by_key(|&k| self.ends[k].used)
                        .expect("the pool holds more than this frame's far end")
                };
                let len = self.reach.len();
                let end = &mut self.ends[k];
                view.reslice(&mut end.slices, far);
                end.forget(len);
                k
            }
        };
        self.ends[k].used = self.tick;
        k
    }

    fn set_now(&mut self, view: &ClusterView, far: f32) {
        self.now = self.end_for(view, far);
        self.then = self.now;
    }

    /// Count from now on at the far end `far` as well.
    fn set_then(&mut self, view: &ClusterView, far: f32) {
        self.then = self.end_for(view, far);
    }

    /// Light `i`'s places under the far end `end` of the pool.
    fn count(&mut self, slots: &[Slot], view: &ClusterView, end: usize, i: usize) -> u32 {
        let end = &mut self.ends[end];
        if end.counts[i] == UNKNOWN {
            let Slot {
                view: at,
                range,
                cone,
                ..
            } = slots[i];
            end.counts[i] = view.count(&end.slices, at, range, cone);
            end.asked.push(i);
        }
        end.counts[i]
    }

    /// Light `i`'s places at the dearer of the two far ends.
    fn cost(&mut self, slots: &[Slot], view: &ClusterView, i: usize) -> u32 {
        if self.reach(slots, view, i) == f32::NEG_INFINITY {
            return 0;
        }
        let now = self.count(slots, view, self.now, i);
        if self.then == self.now {
            return now;
        }
        now.max(self.count(slots, view, self.then, i))
    }

    fn total(&mut self, slots: &[Slot], view: &ClusterView, set: &[usize]) -> u32 {
        set.iter().map(|&i| self.cost(slots, view, i)).sum()
    }
}

/// The groups lights go in ([`Slot::group`]), each light in one: a group's
/// lights, or a light alone. Built again each frame.
#[derive(Debug, Default)]
struct Units {
    /// (group, light) for the grouped lights, sorted.
    grouped: Vec<(u64, u32)>,
    /// The lights of every unit, unit by unit.
    members: Vec<u32>,
    /// Each unit's first member and how many it has, in `members`.
    spans: Vec<(u32, u32)>,
    /// Each light's unit.
    of: Vec<u32>,
}

impl Units {
    fn build(&mut self, slots: &[Slot]) {
        self.grouped.clear();
        self.members.clear();
        self.spans.clear();
        self.of.clear();
        self.of.resize(slots.len(), 0);
        for (i, slot) in slots.iter().enumerate() {
            match slot.group {
                Some(group) => self.grouped.push((group, i as u32)),
                None => {
                    self.of[i] = self.spans.len() as u32;
                    self.spans.push((self.members.len() as u32, 1));
                    self.members.push(i as u32);
                }
            }
        }
        self.grouped.sort_unstable();
        let mut k = 0;
        while k < self.grouped.len() {
            let group = self.grouped[k].0;
            let unit = self.spans.len() as u32;
            let first = self.members.len() as u32;
            while k < self.grouped.len() && self.grouped[k].0 == group {
                let i = self.grouped[k].1;
                self.of[i as usize] = unit;
                self.members.push(i);
                k += 1;
            }
            self.spans.push((first, self.members.len() as u32 - first));
        }
    }

    fn len(&self) -> usize {
        self.spans.len()
    }

    /// Break unit `u` up: each of its lights a unit of its own, as a light
    /// in no group is, and `u` left with none.
    fn split(&mut self, u: usize) {
        let (first, len) = self.spans[u];
        self.spans[u] = (first, 0);
        for k in first..first + len {
            let i = self.members[k as usize];
            self.of[i as usize] = self.spans.len() as u32;
            self.spans.push((self.members.len() as u32, 1));
            self.members.push(i);
        }
    }

    /// The lights of unit `u`.
    fn of_unit(&self, u: usize) -> &[u32] {
        let (first, len) = self.spans[u];
        &self.members[first as usize..(first + len) as usize]
    }
}

/// What the planner reads of one unit this frame: of its lights on enough
/// to show, how many, and whether any is in view, held or drawn.
struct UnitLook {
    size: usize,
    in_view: bool,
    held: bool,
    drawn: bool,
}

fn look(
    units: &Units,
    costs: &mut Costs,
    slots: &[Slot],
    view: &ClusterView,
    u: usize,
) -> UnitLook {
    let mut look = UnitLook {
        size: 0,
        in_view: false,
        held: false,
        drawn: false,
    };
    for &i in units.of_unit(u) {
        let i = i as usize;
        if !slots[i].eligible {
            continue;
        }
        look.size += 1;
        look.in_view |= costs.reach(slots, view, i) != f32::NEG_INFINITY;
        look.held |= slots[i].held;
        look.drawn |= slots[i].drawn;
    }
    look
}

/// The lights of unit `u` on enough to show, and what they cost together
/// at the dearer far end.
fn unit_cost(
    units: &Units,
    costs: &mut Costs,
    slots: &[Slot],
    view: &ClusterView,
    u: usize,
) -> u32 {
    units
        .of_unit(u)
        .iter()
        .map(|&i| i as usize)
        .filter(|&i| slots[i].eligible)
        .map(|i| costs.cost(slots, view, i))
        .sum()
}

/// The budget's working memory, kept from frame to frame so a frame
/// allocates nothing once the lists have grown.
#[derive(Debug, Default)]
pub struct Planner {
    verdicts: Vec<Verdict>,
    costs: Costs,
    units: Units,
    /// (squared rank, unit) of the units ranked.
    keys: Vec<(f32, u32)>,
    /// The units ranked, nearest first.
    order: Vec<usize>,
    /// The lights drawn now.
    drawn: Vec<usize>,
    /// The units wanted, and their lights on enough to show.
    wanted: Vec<usize>,
    wanted_lights: Vec<usize>,
    /// The lights drawn this frame, newcomers too.
    live: Vec<usize>,
    /// The units with newcomers let in this frame, nearest first.
    admitted: Vec<usize>,
    /// The guard's pass each unit was last looked at in, and the pass.
    seen: Vec<u32>,
    pass: u32,
    /// Where what this planner left drawn last frame reached, as Bevy saw
    /// it: where Bevy ends this frame's slices.
    last_end: Option<f32>,
}

/// The order units rank in: by distance, a wanted one [`HOLD_SHARE`]
/// nearer (squared, as the keys are), and by place on a tie.
fn ranked(a: &(f32, u32), b: &(f32, u32)) -> std::cmp::Ordering {
    a.0.total_cmp(&b.0).then(a.1.cmp(&b.1))
}

/// Unit `u`'s rank: its nearest light's distance (squared), as if a share
/// nearer if any of it is wanted; `None` for a unit with no light on
/// enough to show.
fn rank_key(units: &Units, slots: &[Slot], u: usize) -> Option<f32> {
    let mut nearest = None::<f32>;
    let mut held = false;
    for &i in units.of_unit(u) {
        let slot = &slots[i as usize];
        if slot.eligible {
            let d = slot.view.length_squared();
            nearest = Some(nearest.map_or(d, |n| n.min(d)));
            held |= slot.held;
        }
    }
    nearest.map(|d| if held { d * HOLD_SHARE * HOLD_SHARE } else { d })
}

impl Planner {
    /// What the budget does with each slot: at most `count` lights drawn,
    /// within the cluster index list of `view` - see the module docs.
    pub fn plan(&mut self, slots: &[Slot], count: usize, view: &ClusterView) -> &[Verdict] {
        let len = slots.len();
        self.verdicts.clear();
        self.verdicts.resize(len, Verdict::Fade);
        self.costs.reset(len);
        self.units.build(slots);
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

        // The nearest few, ranked: units none of whose lights is drawn yet,
        // the nearest of them, and every unit with a light drawn.
        let units = &self.units;
        self.keys.clear();
        if count > 0 {
            for u in 0..units.len() {
                let fresh = units.of_unit(u).iter().all(|&i| !slots[i as usize].drawn);
                if fresh && let Some(key) = rank_key(units, slots, u) {
                    self.keys.push((key, u as u32));
                }
            }
            let ranked_len = RANKED_PER_PLACE * count;
            if self.keys.len() > ranked_len {
                self.keys.select_nth_unstable_by(ranked_len, ranked);
                self.keys.truncate(ranked_len);
            }
            for u in 0..units.len() {
                let drawn = units.of_unit(u).iter().any(|&i| slots[i as usize].drawn);
                if drawn && let Some(key) = rank_key(units, slots, u) {
                    self.keys.push((key, u as u32));
                }
            }
            self.keys.sort_unstable_by(ranked);
        }
        self.order.clear();
        self.order
            .extend(self.keys.iter().map(|&(_, u)| u as usize));

        // The wanted set: the nearest that fit, chosen again for the far
        // end it sets until the two agree (a nearer end costs more and fits
        // no more, so it settles). It starts where what is drawn ends: a
        // set that stays as it is settles there at once, at the far end the
        // newcomers and the guard ask about too. With nothing drawn it
        // starts where the nearest the count holds would end.
        let mut far = self.costs.end(slots, view, &self.drawn);
        if self.drawn.is_empty() {
            self.wanted_lights.clear();
            for &u in &self.order {
                if self.wanted_lights.len() >= count {
                    break;
                }
                self.wanted_lights.extend(
                    units
                        .of_unit(u)
                        .iter()
                        .map(|&i| i as usize)
                        .filter(|&i| slots[i].eligible),
                );
            }
            far = self.costs.end(slots, view, &self.wanted_lights);
        }
        for _ in 0..FAR_ROUNDS {
            self.costs.set_then(view, far);
            self.wanted.clear();
            self.wanted_lights.clear();
            let mut spent = 0;
            // Once a light in view does not fit, one out of view that ranks
            // after it is not wanted either - it would not fit as it came
            // into view - and after a few misses nothing more is asked. One
            // out of view that is fading out goes on fading: brought back
            // there, it would only be sent away again as it came into view.
            // A group too big for what is left of the count waits whole.
            // A group that could not come in whole even to an empty count
            // and an empty list - more lights than the count, or costing
            // more than its line alone - goes light by light from here on,
            // as a room's lights do, so its nearest are drawn rather than
            // none ([`Planner::split`]).
            let mut misses = 0;
            let mut k = 0;
            while k < self.order.len() {
                if self.wanted_lights.len() >= count || misses >= MISSES {
                    break;
                }
                let u = self.order[k];
                let unit = look(&self.units, &mut self.costs, slots, view, u);
                if unit.size == 0 {
                    k += 1;
                    continue;
                }
                let limit = if unit.held { KEEP_LIMIT } else { ADMIT_LIMIT };
                if unit.size > 1
                    && (unit.size > count
                        || unit_cost(&self.units, &mut self.costs, slots, view, u) > limit)
                {
                    self.split(slots, k);
                    continue;
                }
                k += 1;
                if self.wanted_lights.len() + unit.size > count {
                    continue;
                }
                if !unit.in_view && !unit.held && (misses > 0 || unit.drawn) {
                    continue;
                }
                let c = unit_cost(&self.units, &mut self.costs, slots, view, u);
                if spent + c <= limit {
                    spent += c;
                    self.wanted.push(u);
                    self.wanted_lights.extend(
                        self.units
                            .of_unit(u)
                            .iter()
                            .map(|&i| i as usize)
                            .filter(|&i| slots[i].eligible),
                    );
                } else {
                    misses += 1;
                }
            }
            let end = self.costs.end(slots, view, &self.drawn).max(self.costs.end(
                slots,
                view,
                &self.wanted_lights,
            ));
            if end >= far {
                break;
            }
            far = end;
        }

        let units = &self.units;
        // What is drawn stays drawn this frame, those fading out too; a
        // wanted group's newcomers come in, all of them, while they leave
        // it room.
        let far = self.costs.end(slots, view, &self.drawn);
        self.costs.set_then(view, far);
        let mut spent = self.costs.total(slots, view, &self.drawn);
        let mut lit = self.drawn.len();
        self.admitted.clear();
        let mut waiting = false;
        for k in 0..self.wanted.len() {
            let u = self.wanted[k];
            let (mut need, mut c) = (0, 0);
            for &i in units.of_unit(u) {
                let i = i as usize;
                if !slots[i].eligible {
                    continue;
                }
                if slots[i].drawn {
                    self.verdicts[i] = Verdict::Draw;
                } else {
                    need += 1;
                    c += self.costs.cost(slots, view, i);
                }
            }
            if need == 0 {
                continue;
            }
            if waiting || lit + need > count || spent + c > ADMIT_LIMIT {
                waiting = true;
                continue;
            }
            lit += need;
            spent += c;
            for &i in units.of_unit(u) {
                let i = i as usize;
                if slots[i].eligible && !slots[i].drawn {
                    self.verdicts[i] = Verdict::Draw;
                }
            }
            self.admitted.push(u);
        }

        // The guard: what will be drawn, at both ends of its slices, within
        // the list - else a group of newcomers waits, or a drawn group is
        // cut.
        self.seen.clear();
        self.seen.resize(units.len(), 0);
        self.pass = 0;
        loop {
            self.live.clear();
            self.live.extend(
                self.drawn
                    .iter()
                    .copied()
                    .filter(|&i| self.verdicts[i] != Verdict::Cut),
            );
            for &u in &self.admitted {
                self.live.extend(
                    units
                        .of_unit(u)
                        .iter()
                        .map(|&i| i as usize)
                        .filter(|&i| slots[i].eligible && !slots[i].drawn),
                );
            }
            let far = self.costs.end(slots, view, &self.live);
            self.costs.set_then(view, far);
            if self.costs.total(slots, view, &self.live) <= CUT_LIMIT {
                break;
            }
            if let Some(u) = self.admitted.pop() {
                for &i in units.of_unit(u) {
                    let i = i as usize;
                    if !slots[i].drawn {
                        self.verdicts[i] = Verdict::Fade;
                    }
                }
                continue;
            }
            // The group fading out already, else the one that frees the
            // most places for how near it is.
            self.pass += 1;
            let mut victim = None;
            let mut best = (false, 0.0);
            for k in 0..self.live.len() {
                let u = units.of[self.live[k]] as usize;
                if self.seen[u] == self.pass {
                    continue;
                }
                self.seen[u] = self.pass;
                let (mut c, mut nearest, mut fading) = (0, f32::INFINITY, false);
                for &i in units.of_unit(u) {
                    let i = i as usize;
                    if slots[i].drawn && self.verdicts[i] != Verdict::Cut {
                        c += self.costs.cost(slots, view, i);
                        nearest = nearest.min(slots[i].view.length());
                        fading |= self.verdicts[i] == Verdict::Fade;
                    }
                }
                if c == 0 {
                    continue;
                }
                let score = (fading, c as f32 * nearest);
                let ahead = match (score.0, best.0) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => score.1 > best.1,
                };
                if victim.is_none() || ahead {
                    victim = Some(u);
                    best = score;
                }
            }
            let Some(u) = victim else {
                break;
            };
            for &i in units.of_unit(u) {
                if slots[i as usize].drawn {
                    self.verdicts[i as usize] = Verdict::Cut;
                }
            }
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

    /// Tell the planner which lights are lit once this frame's verdicts
    /// have been drawn - `lit` of each slot of the frame just planned - so
    /// it ends next frame's slices where Bevy will: at the farthest of
    /// those. A light fading out that reached nothing this frame is no
    /// longer clustered, and the slices end short of it. Without this call
    /// the planner reckons with every light its verdicts left drawn.
    pub fn settle(&mut self, slots: &[Slot], view: &ClusterView, lit: impl Fn(usize) -> bool) {
        self.last_end = Some(
            (0..slots.len())
                .filter(|&i| lit(i))
                .filter_map(|i| view.reach(slots[i].view, slots[i].range))
                .fold(0.0, f32::max),
        );
    }

    /// Break the unit at `order[k]` up into a unit for each of its lights,
    /// each ranked where its own distance puts it among the units after it
    /// (a light drawn among them too), the unit itself left empty in its
    /// place.
    fn split(&mut self, slots: &[Slot], k: usize) {
        let u = self.order[k];
        let first = self.units.len();
        self.units.split(u);
        for single in first..self.units.len() {
            let Some(key) = rank_key(&self.units, slots, single) else {
                continue;
            };
            let entry = (key, single as u32);
            let at = k + 1 + self.keys[k + 1..].partition_point(|e| ranked(e, &entry).is_lt());
            self.keys.insert(at, entry);
            self.order.insert(at, single);
        }
    }

    /// Where the planner reckons Bevy ends next frame's slices: see
    /// [`Self::settle`]. For tests.
    #[cfg(test)]
    pub(crate) fn last_end(&self) -> Option<f32> {
        self.last_end
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
