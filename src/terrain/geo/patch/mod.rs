//! The detail patch (P4.2, #1597, epic #1580): Berlin at full detail round
//! a body that has walked or driven out past the core.
//!
//! Since P4.1 (#1596) the far field is walked, but it is the coarse ground
//! of the horizon: 17 to 74 m a pixel, its land use painted, with no
//! streets and nothing standing on it but the ring's block forms. Owner
//! decisions on #1597 (2026-10-09): past the core, a patch AS BIG AS THE
//! CORE loads round the body, a new one every ~250 m of travel, each
//! costing what a visit's core costs; round it only the coarse ground, the
//! home core's ring staying where it is; and the home core and the
//! record's content stay put, a placement past the core sitting on the
//! patch where one has loaded.
//!
//! - **Where.** A patch is a square of the core's own lattice
//!   ([`PatchPlan`]): its points are the core's points carried on past its
//!   edges, so a patch beside the core shares the core's boundary vertices
//!   exactly. It never overlaps the core: one that would is pushed out to
//!   stand against it ([`plan_for`]), on the side the body is nearest. It
//!   stays inside the far field's drawn edge, and where the ground between
//!   the core and that edge is narrower than the core, the patch is as
//!   wide as that ground; narrower than [`MIN_PATCH_M`], there is none.
//!   Its place snaps to a lattice of [`PATCH_STEP_M`], so a body that
//!   comes back to a place asks for the patch it had there, which the
//!   fetcher's store answers.
//! - **When.** A body within [`LOOKAHEAD_M`] of the core's edge, or past
//!   it, wants a patch; one deeper in the core than [`DROP_M`] lets it go
//!   ([`want`]). A patch serves until its body has moved three quarters of
//!   a step from where the patch was asked for, so a body pacing about the
//!   halfway line does not fetch two patches by turns.
//! - **What.** The patch is fetched and decoded as a core is: the terrain
//!   and the land use rendered over its box at the core's pixel, its
//!   streets and its street level ([`decode_patch`]). Its water settles to
//!   the region's one level where the region's water plane spans the far
//!   field, and its dry ground is kept above it there in any case, so the
//!   plane floods nothing the patch does not call water.
//! - **The seam.** Against the core, the patch's boundary vertices are the
//!   core's own, at the core's heights. Against the far field, the patch is
//!   cut along its edge out of the far mesh in the shader (`SPLAT_HOLE`):
//!   the far cells the edge crosses keep their colliders, and within them
//!   the patch draws the far field's own triangles at its points, so the
//!   two grounds meet along the edge and agree where both are walked. From
//!   there it eases into Berlin's own heights over [`BLEND_M`], and from
//!   the core's boundary over [`CORE_BLEND_M`] ([`blend`]).
//!
//! The patch rides in the far field ([`super::far::FarField::patch`]), so
//! every reader of the ground as drawn - the placement snap, the camera,
//! the recovery, the contact classifier, a scatter's land use - reads it
//! where it has loaded, with no reader of its own.

pub(crate) mod follow;

use std::sync::{Arc, RwLock};

use bevy::math::Rect;
use bevy::prelude::*;
use bevy_symbios_ground::{HeightMap, WeightMap};
use geodata::berlin::LandUse;
use geodata::request::Bbox;

use super::far::FarField;
use super::street_level::StreetLevel;
use super::{CoreBodies, streets};
use crate::urban::RoadParts;

/// How far a body travels between patches (m): the lattice a patch's place
/// snaps to.
pub(crate) const PATCH_STEP_M: f32 = 250.0;

/// The narrowest a patch is worth fetching (m): where the ground between
/// the core and the far field's edge is narrower, there is no patch.
pub(crate) const MIN_PATCH_M: f32 = 250.0;

/// How far inside the core's edge a body already wants a patch (m): the
/// patch beyond the edge is in before the body reaches it.
pub(crate) const LOOKAHEAD_M: f32 = 200.0;

/// How far inside the core's edge a body lets its patch go (m).
pub(crate) const DROP_M: f32 = 300.0;

/// How far from where the far field's cells end the patch eases from the
/// far field's ground into its own (m).
pub(crate) const BLEND_M: f32 = 40.0;

/// How far from the core's boundary the patch eases from the core's
/// heights into its own (m): the two read the same survey on the same
/// lattice, so only a whisker of rounding is eased away.
pub(crate) const CORE_BLEND_M: f32 = 6.0;

/// The share of a step a body may move from where its patch was asked for
/// before it asks for another.
const SERVES_STEPS: f32 = 0.75;

/// The core's lattice: its points `cell` metres apart, `cells` cells a side,
/// centred on the origin, carried on past its edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Lattice {
    pub cell: f32,
    pub cells: u32,
}

impl Lattice {
    /// The lattice of the core `core`.
    pub(crate) fn of(core: &HeightMap) -> Self {
        Lattice {
            cell: core.scale(),
            cells: (core.width().saturating_sub(1)) as u32,
        }
    }

    /// The core's half-extent (m).
    pub(crate) fn half(&self) -> f32 {
        self.cells as f32 * self.cell / 2.0
    }

    /// The world coordinate of lattice line `i`, counted from the core's
    /// north-west corner.
    pub(crate) fn world(&self, i: i32) -> f32 {
        -self.half() + i as f32 * self.cell
    }

    /// How many cells from the core's north-west corner world `w` lies.
    fn index(&self, w: f32) -> f32 {
        (w + self.half()) / self.cell
    }
}

/// Where a patch stands: a square of the core's lattice, `cells` cells a
/// side, its north-west corner `(x0, z0)` cells from the core's (east and
/// south positive).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PatchPlan {
    pub x0: i32,
    pub z0: i32,
    pub cells: u32,
}

impl PatchPlan {
    /// Points a side: its heightmap's grid, and its renders'.
    pub(crate) fn grid(&self) -> u32 {
        self.cells + 1
    }

    /// Its world `(x, z)` rectangle on `lattice`.
    pub(crate) fn rect(&self, lattice: Lattice) -> Rect {
        let end = |i: i32| lattice.world(i + self.cells as i32);
        Rect {
            min: Vec2::new(lattice.world(self.x0), lattice.world(self.z0)),
            max: Vec2::new(end(self.x0), end(self.z0)),
        }
    }

    /// The box its renders are asked over, beside the core's `core` box of
    /// `core_grid` pixels: the core's pixel, each pixel's centre on a point
    /// of the lattice, as [`super::core_bbox`] centres the core's - to the
    /// whole metre a box is asked in.
    pub(crate) fn bbox(&self, core: Bbox, core_grid: u32) -> Bbox {
        let pixel = (core.max_e - core.min_e) as f64 / f64::from(core_grid);
        let side = (f64::from(self.grid()) * pixel).round() as i64;
        let min_e = core.min_e + (f64::from(self.x0) * pixel).round() as i64;
        let max_n = core.max_n - (f64::from(self.z0) * pixel).round() as i64;
        Bbox {
            min_e,
            min_n: max_n - side,
            max_e: min_e + side,
            max_n,
        }
    }

    /// The core's boundary it stands against, if any, in lattice cells: the
    /// fixed coordinate, whether it is an X (`true`) or a Z line, and the
    /// span along it the two share. A patch that only touches the core at a
    /// corner shares a span of no length.
    fn shared_edge(&self, lattice: Lattice) -> Option<(i32, bool, (i32, i32))> {
        let n = lattice.cells as i32;
        let c = self.cells as i32;
        let along = |a0: i32| (a0.max(0), (a0 + c).min(n));
        let (edge, x_line, span) = if self.x0 + c == 0 {
            (0, true, along(self.z0))
        } else if self.x0 == n {
            (n, true, along(self.z0))
        } else if self.z0 + c == 0 {
            (0, false, along(self.x0))
        } else if self.z0 == n {
            (n, false, along(self.x0))
        } else {
            return None;
        };
        (span.0 <= span.1).then_some((edge, x_line, span))
    }
}

/// The patch a body at world `at` asks for, on the core's `lattice`, inside
/// a far field whose drawn edge is `span_half` from the origin either way;
/// `None` where the ground between the core and that edge is narrower than
/// [`MIN_PATCH_M`]. Snapped to [`PATCH_STEP_M`], kept inside the far
/// field's edge, and pushed out of the core to stand against it.
pub(crate) fn plan_for(at: Vec2, lattice: Lattice, span_half: f32) -> Option<PatchPlan> {
    let n = lattice.cells as i32;
    // The ground between the core and the far field's edge, in cells.
    let ring = (span_half - lattice.half()) / lattice.cell;
    let cells = (lattice.cells as f32).min(ring.floor());
    if !cells.is_finite() || cells * lattice.cell < MIN_PATCH_M {
        return None;
    }
    let cells = cells as i32;
    let step = (PATCH_STEP_M / lattice.cell).round().max(1.0) as i32;
    let (lo, hi) = (
        (-ring).ceil() as i32,
        (n as f32 + ring).floor() as i32 - cells,
    );
    let corner = |w: f32| {
        let asked = (lattice.index(w) - cells as f32 / 2.0) / step as f32;
        (asked.round() as i32 * step).clamp(lo, hi)
    };
    let (mut x0, mut z0) = (corner(at.x), corner(at.y));
    // Beside the core and nearer it than a step: against it, so no strip of
    // the far field is left between the two.
    let pull = |a0: i32| {
        if a0 > n && a0 - n < step {
            n
        } else if a0 + cells < 0 && -(a0 + cells) < step {
            -cells
        } else {
            a0
        }
    };
    if z0 < n && z0 + cells > 0 {
        x0 = pull(x0);
    }
    if x0 < n && x0 + cells > 0 {
        z0 = pull(z0);
    }
    // Out of the core, the shortest way: it never covers what the core
    // draws, and against it, it shares the core's boundary.
    if x0 < n && x0 + cells > 0 && z0 < n && z0 + cells > 0 {
        let moves = [
            (x0 + cells, (-cells, z0)),
            (n - x0, (n, z0)),
            (z0 + cells, (x0, -cells)),
            (n - z0, (x0, n)),
        ];
        let (_, to) = moves
            .into_iter()
            .min_by_key(|&(cost, _)| cost)
            .expect("four ways out");
        (x0, z0) = to;
    }
    Some(PatchPlan {
        x0,
        z0,
        cells: cells as u32,
    })
}

/// What a body asks of its patch ([`want`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Want {
    /// Nothing changes: its patch serves it, or it wants none and has none.
    Stay,
    /// The patch it has goes: it is deep in the core.
    Drop,
    /// A patch to load, asked for at `asked` (world `(x, z)`).
    Load { plan: PatchPlan, asked: Vec2 },
}

/// What a body at world `at` asks of its patch, given the one it has or is
/// fetching (`current`, and where that was asked for), on the core's
/// `lattice` in a far field `span_half` either way (see the module docs).
pub(crate) fn want(
    at: Vec2,
    current: Option<(PatchPlan, Vec2)>,
    lattice: Lattice,
    span_half: f32,
) -> Want {
    // How far inside the core's edge: negative past it.
    let inside = lattice.half() - at.x.abs().max(at.y.abs());
    if inside > DROP_M {
        return if current.is_some() {
            Want::Drop
        } else {
            Want::Stay
        };
    }
    if let Some((_, asked)) = current {
        let reach = SERVES_STEPS * PATCH_STEP_M;
        if (at.x - asked.x).abs() <= reach && (at.y - asked.y).abs() <= reach {
            return Want::Stay;
        }
    } else if inside > LOOKAHEAD_M {
        return Want::Stay;
    }
    match plan_for(at, lattice, span_half) {
        Some(plan) if current.is_none_or(|(now, _)| now != plan) => Want::Load { plan, asked: at },
        Some(_) => Want::Stay,
        None if current.is_some() => Want::Drop,
        None => Want::Stay,
    }
}

/// A patch's ground: its heights, metres above sea level on its piece of
/// the core's lattice, and the land use on them, row-major from its
/// north-west point - what the ground's readers read inside its rectangle.
#[derive(Clone, Debug)]
pub(crate) struct PatchGround {
    plan: PatchPlan,
    /// World `(x, z)` of its north-west point.
    min: Vec2,
    heights: HeightMap,
    cover: Vec<Option<LandUse>>,
}

impl PatchGround {
    /// Where it stands.
    pub(crate) fn plan(&self) -> PatchPlan {
        self.plan
    }

    /// Its heights.
    pub(crate) fn heights(&self) -> &HeightMap {
        &self.heights
    }

    /// Its world `(x, z)` rectangle.
    pub(crate) fn rect(&self) -> Rect {
        let extent = self.extent();
        Rect {
            min: self.min,
            max: self.min + Vec2::splat(extent),
        }
    }

    /// Its side (m): its first point to its last.
    pub(crate) fn extent(&self) -> f32 {
        (self.heights.width() - 1) as f32 * self.heights.scale()
    }

    /// Whether world `(x, z)` lies on it, its edges included.
    pub(crate) fn holds(&self, x: f32, z: f32) -> bool {
        let rect = self.rect();
        (rect.min.x..=rect.max.x).contains(&x) && (rect.min.y..=rect.max.y).contains(&z)
    }

    /// The ground's height at world `(x, z)`, between its points as the
    /// core's is read; held at its edge past it.
    pub(crate) fn height_at(&self, x: f32, z: f32) -> f32 {
        let extent = self.extent();
        self.heights.get_height_at(
            (x - self.min.x).clamp(0.0, extent),
            (z - self.min.y).clamp(0.0, extent),
        )
    }

    /// The ground's upward normal at world `(x, z)`, as the core's is read.
    pub(crate) fn normal_at(&self, x: f32, z: f32) -> Vec3 {
        let extent = self.extent();
        Vec3::from_array(self.heights.get_normal_at(
            (x - self.min.x).clamp(0.0, extent),
            (z - self.min.y).clamp(0.0, extent),
        ))
    }

    /// The land use of its point nearest world `(x, z)`.
    pub(crate) fn cover_at(&self, x: f32, z: f32) -> Option<LandUse> {
        let grid = self.heights.width();
        let cell = self.heights.scale();
        let last = (grid - 1) as f32;
        let point = |v: f32| ((v / cell).round().clamp(0.0, last)) as usize;
        self.cover[point(z - self.min.y) * grid + point(x - self.min.x)]
    }

    /// The splat layers' weights at world `(x, z)`, as the GPU samples its
    /// weight map: between its points, as the core's are read.
    pub(crate) fn weights_at(&self, x: f32, z: f32) -> [f32; 4] {
        super::ground::weights_between(
            &self.cover,
            self.heights.width(),
            self.heights.scale(),
            x - self.min.x,
            z - self.min.y,
        )
    }

    /// Its splat weight map: one texel per point, all of it on the point's
    /// layer, as the core's.
    pub(crate) fn weight_map(&self) -> WeightMap {
        super::ground::one_hot_weights(&self.cover, self.heights.width())
    }

    /// The highest of its points within `radius` of world `(x, z)`, if any:
    /// between its points the ground is read bilinearly, which keeps its
    /// maximum at a point or on the rim, as the core's footprint snap
    /// reads the core's.
    pub(crate) fn highest_point_within(&self, x: f32, z: f32, radius: f32) -> Option<f32> {
        let cell = self.heights.scale().max(1e-3);
        let last = self.heights.width() - 1;
        let index = |w: f32, min: f32| (w - min) / cell;
        let range = |centre: f32, min: f32| {
            let lo = index(centre - radius, min).ceil().max(0.0);
            let hi = index(centre + radius, min).floor().min(last as f32);
            (lo <= hi).then_some((lo as usize, hi as usize))
        };
        let ((x0, x1), (z0, z1)) = (range(x, self.min.x)?, range(z, self.min.y)?);
        let r2 = radius * radius;
        let mut highest: Option<f32> = None;
        // Bounded as the core's sweep is: a stride past 64 points a side.
        let stride = ((x1 - x0).max(z1 - z0) + 1).div_ceil(64).max(1);
        for iz in (z0..=z1).step_by(stride) {
            let dz = self.min.y + iz as f32 * cell - z;
            for ix in (x0..=x1).step_by(stride) {
                let dx = self.min.x + ix as f32 * cell - x;
                if dx * dx + dz * dz <= r2 {
                    let h = self.heights.get(ix, iz);
                    highest = Some(highest.map_or(h, |m| m.max(h)));
                }
            }
        }
        highest
    }
}

#[cfg(test)]
impl PatchGround {
    /// The patch `plan` on `lattice` with `heights` (its points, from its
    /// north-west one), every point's land use `cover`.
    pub(crate) fn from_heights(
        plan: PatchPlan,
        lattice: Lattice,
        heights: HeightMap,
        cover: Option<LandUse>,
    ) -> Self {
        let grid = plan.grid() as usize;
        assert_eq!((heights.width(), heights.height()), (grid, grid));
        PatchGround {
            plan,
            min: plan.rect(lattice).min,
            heights,
            cover: vec![cover; grid * grid],
        }
    }
}

/// The patch a far field holds over itself while one is loaded: one slot,
/// shared by every clone of the ground, so the contact classifier's copy
/// reads the patch the moment it lands. Two slots are equal when they hold
/// the same patch, or none.
#[derive(Clone, Default)]
pub(crate) struct PatchSlot(Arc<RwLock<Option<Arc<PatchGround>>>>);

impl PatchSlot {
    /// What `read` says of the patch it holds, if any.
    pub(crate) fn with<R>(&self, read: impl FnOnce(Option<&PatchGround>) -> R) -> R {
        let held = self
            .0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        read(held.as_deref())
    }

    /// The patch it holds.
    pub(crate) fn get(&self) -> Option<Arc<PatchGround>> {
        self.0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Hold `patch` in place of what it held.
    pub(crate) fn set(&self, patch: Option<Arc<PatchGround>>) {
        *self
            .0
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = patch;
    }
}

impl PartialEq for PatchSlot {
    fn eq(&self, other: &Self) -> bool {
        match (self.get(), other.get()) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(&a, &b),
            _ => false,
        }
    }
}

impl std::fmt::Debug for PatchSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.get() {
            Some(patch) => write!(f, "PatchSlot({:?})", patch.plan),
            None => write!(f, "PatchSlot(empty)"),
        }
    }
}

/// What a patch is decoded against: where it stands, the core and the far
/// field it is set into, and the level of the water plane where the plane
/// spans the far field.
pub(crate) struct PatchInputs {
    pub plan: PatchPlan,
    pub core: HeightMap,
    pub far: Arc<FarField>,
    /// The region's water level, where its water plane spans the far field
    /// (the far field took the core's water).
    pub level: Option<f32>,
}

/// A decoded patch: its ground, its streets meshed in its own frame (from
/// its north-west point), and its street level in the world frame, and why
/// each part that could not be had was not.
pub(crate) struct DecodedPatch {
    pub ground: PatchGround,
    pub streets: Option<RoadParts>,
    pub street_level: Option<StreetLevel>,
    pub lost: Vec<String>,
}

/// Paces a patch's build on the web: there the compute pool is the main
/// thread, and a patch's build - about 0.15 s natively, more on the web -
/// run whole would hold one frame for all of it. So the build waits a frame
/// between its stages ([`Pace::next`]), each frame's patch system letting
/// it on ([`Pace::tick`]): one stage a frame, the longest about 50 ms
/// natively. Natively the build runs on a thread of its own, and a stage's
/// end only yields to the pool.
#[derive(Clone, Default)]
pub(crate) struct Pace(Arc<std::sync::Mutex<(u64, Option<std::task::Waker>)>>);

impl Pace {
    /// A frame has passed: a stage waiting on it may go on.
    pub(crate) fn tick(&self) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.0 = state.0.wrapping_add(1);
        if let Some(waker) = state.1.take() {
            waker.wake();
        }
    }

    /// Wait for the next frame on the web; natively, yield to the pool.
    pub(crate) async fn next(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            let start = self.0.lock().map_or(0, |state| state.0);
            std::future::poll_fn(|cx| {
                let mut state = self
                    .0
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if state.0 != start {
                    std::task::Poll::Ready(())
                } else {
                    state.1 = Some(cx.waker().clone());
                    std::task::Poll::Pending
                }
            })
            .await;
        }
        #[cfg(not(target_arch = "wasm32"))]
        futures_lite::future::yield_now().await;
    }
}

/// Decode a patch's answers (see the module docs): its heights, settled to
/// the region's water level and eased into the core and the far field at
/// its edges; its land use; its streets; its street level. An error is
/// its terrain's or its land use's: without both there is no patch. Paced
/// between its stages ([`Pace`]).
pub(crate) async fn decode_patch(
    bodies: &CoreBodies,
    inputs: &PatchInputs,
    pace: &Pace,
) -> Result<DecodedPatch, String> {
    let lattice = Lattice::of(&inputs.core);
    let (plan, cell) = (inputs.plan, lattice.cell);
    let grid = plan.grid();
    let mut heights = super::decode_heights(&bodies.terrain_legend, &bodies.terrain, grid, cell)?;
    let (land_use_legend, land_use) = bodies.land_use.as_ref().map_err(String::clone)?;
    let cover = super::ground::decode_cover(land_use_legend, land_use, grid)?;
    let frame = streets::CoreFrame {
        bbox: bodies.bbox,
        grid,
        cell,
    };
    let mut lost = Vec::new();
    let read = match bodies.streets.as_ref() {
        Some(Ok((axes, carriageways))) => match super::read_axes(axes, carriageways) {
            Ok(read) => Some(read),
            Err(reason) => {
                lost.push(reason);
                None
            }
        },
        Some(Err(reason)) => {
            lost.push(reason.clone());
            None
        }
        None => None,
    };
    let bridges = read.as_ref().map(|read| streets::street_cells(read, frame));
    let mut raw = heights.data.clone();
    let wet = super::ground::water_mask(&cover, grid as usize, cell, bridges.as_deref());
    if let Some(level) = inputs.level {
        let settled = geodata::water::settle_to(&mut heights.data, &wet, grid, grid, cell, level);
        if settled.is_none() {
            // The plane spans the patch whether its own water takes the
            // level or not: what it does not call water stays dry.
            let crest = level + geodata::water::FREEBOARD_M;
            for h in &mut heights.data {
                *h = h.max(crest);
            }
        }
    }
    pace.next().await;
    blend(&mut heights.data, inputs);
    pace.next().await;
    blend(&mut raw, inputs);
    pace.next().await;
    let min = Vec2::new(lattice.world(plan.x0), lattice.world(plan.z0));
    let street_level = bodies.street_level.as_ref().and_then(|level_bodies| {
        let (level, gone) =
            super::street_level::decode_street_level(level_bodies, frame, &cover, read.as_ref());
        lost.extend(gone);
        let centre = min + Vec2::splat(frame.extent() / 2.0);
        (!level.is_empty()).then(|| level.moved(centre.x, centre.y))
    });
    pace.next().await;
    let streets = read.as_ref().and_then(|read| {
        let road_ground = streets::road_ground(&raw, &heights.data, grid, cell, &wet, inputs.level);
        streets::mesh_streets_within(read, frame, street_bounds(plan, lattice), &road_ground)
    });
    Ok(DecodedPatch {
        ground: PatchGround {
            plan,
            min,
            heights: super::super::heightmap::heightmap_from_data(heights),
            cover,
        },
        streets,
        street_level,
        lost,
    })
}

/// Where a patch's streets end, in its own frame: [`streets::EDGE_MARGIN_M`]
/// inside its edges, as a core's, but past the edge it shares with the
/// core as far beyond it - where the core's own streets end - so a street
/// crossing from the core onto the patch runs on unbroken. Where the patch
/// reaches past the core's corner along that edge, its streets there end
/// as far out over the far field.
fn street_bounds(plan: PatchPlan, lattice: Lattice) -> streets::StreetBounds {
    let margin = streets::EDGE_MARGIN_M;
    let extent = plan.cells as f32 * lattice.cell;
    let mut bounds = streets::StreetBounds {
        x: (margin, extent - margin),
        z: (margin, extent - margin),
    };
    let n = lattice.cells as i32;
    let c = plan.cells as i32;
    if let Some((edge, x_line, _)) = plan.shared_edge(lattice) {
        let side = if x_line { &mut bounds.x } else { &mut bounds.z };
        let start = if x_line { plan.x0 } else { plan.z0 };
        if start == n && edge == n {
            side.0 = -margin;
        } else if start + c == 0 && edge == 0 {
            side.1 = extent + margin;
        }
    }
    bounds
}

/// `smoothstep` of `t` clamped to `[0, 1]`: 0 at or below 0, 1 at or above
/// 1, and level at both ends.
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Distance from `p` to the segment `a`-`b`.
fn to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = if ab.length_squared() > 0.0 {
        ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + ab * t)
}

/// Ease the patch's `heights` (its points, row-major from its north-west
/// point) into the ground round it (see the module docs): on its boundary
/// with the core, the core's own heights at the core's own points; within
/// the far cells its edges cross, the far field's ground as drawn; and
/// from there into its own over [`BLEND_M`], from the core's boundary over
/// [`CORE_BLEND_M`].
pub(crate) fn blend(heights: &mut [f32], inputs: &PatchInputs) {
    let core = &inputs.core;
    let lattice = Lattice::of(core);
    let plan = inputs.plan;
    let rect = plan.rect(lattice);
    let (grid, cell) = (plan.grid() as usize, lattice.cell);
    let core_half = lattice.half();
    let far = &inputs.far;
    // Where the far field's cells are whole inside the patch: the first of
    // its grid lines inside each edge.
    let inner = far.inner_lines(core_half, rect);
    let shared = plan.shared_edge(lattice);
    // The core's boundary the patch stands against, and the far-facing
    // stretches of the same edge, as world segments.
    let mut core_segment: Option<(Vec2, Vec2)> = None;
    let mut far_segments: Vec<(Vec2, Vec2)> = Vec::new();
    // Which of the four edges (west, east, north, south) face the far field
    // whole, with their cells' inner line.
    let mut faces_far = [true; 4];
    if let Some((edge, x_line, (from, to))) = shared {
        let fixed = lattice.world(edge);
        let point = |along: f32| {
            if x_line {
                Vec2::new(fixed, along)
            } else {
                Vec2::new(along, fixed)
            }
        };
        let (lo, hi) = if x_line {
            (rect.min.y, rect.max.y)
        } else {
            (rect.min.x, rect.max.x)
        };
        let (a, b) = (lattice.world(from), lattice.world(to));
        core_segment = Some((point(a), point(b)));
        if lo < a {
            far_segments.push((point(lo), point(a)));
        }
        if b < hi {
            far_segments.push((point(b), point(hi)));
        }
        let side = match (x_line, fixed == rect.min.x, fixed == rect.min.y) {
            (true, true, _) => 0,
            (true, false, _) => 1,
            (false, _, true) => 2,
            (false, _, false) => 3,
        };
        faces_far[side] = false;
    }
    for j in 0..grid {
        for i in 0..grid {
            let p = Vec2::new(rect.min.x + i as f32 * cell, rect.min.y + j as f32 * cell);
            let at = j * grid + i;
            // The core's own point, on the boundary they share.
            if let Some((edge, x_line, (from, to))) = shared {
                let (fixed, along) = if x_line {
                    (plan.x0 + i as i32, plan.z0 + j as i32)
                } else {
                    (plan.z0 + j as i32, plan.x0 + i as i32)
                };
                if fixed == edge && (from..=to).contains(&along) {
                    let (cx, cz) = if x_line {
                        (fixed as usize, along as usize)
                    } else {
                        (along as usize, fixed as usize)
                    };
                    heights[at] = core.get(cx, cz);
                    continue;
                }
            }
            let into = [
                p.x - inner[0],
                inner[1] - p.x,
                p.y - inner[2],
                inner[3] - p.y,
            ];
            let mut far_d = f32::INFINITY;
            for (side, d) in into.into_iter().enumerate() {
                if faces_far[side] {
                    far_d = far_d.min(d);
                }
            }
            for &(a, b) in &far_segments {
                far_d = far_d.min(to_segment(p, a, b));
            }
            let core_d = core_segment.map_or(f32::INFINITY, |(a, b)| to_segment(p, a, b));
            let own = ease(far_d / BLEND_M) * ease(core_d / CORE_BLEND_M);
            if own < 1.0 {
                let ground = far.mesh_height_at(core, p.x, p.y);
                heights[at] = ground + (heights[at] - ground) * own;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small region: a core of 101 points 2 m apart (200 m, a slope with
    /// a ripple), and a far field of 48 pixels 10 m apart round it (470 m
    /// from first centre to last), rolling.
    fn scene() -> (HeightMap, Arc<FarField>) {
        let mut core = HeightMap::new(101, 101, 2.0);
        for z in 0..101 {
            for x in 0..101 {
                let (wx, wz) = (x as f32 * 2.0 - 100.0, z as f32 * 2.0 - 100.0);
                core.set(x, z, 34.0 + 0.02 * wx - 0.01 * wz + 0.2 * (wx * 0.3).sin());
            }
        }
        let far = FarField::from_fn(48, 10.0, |x, z| {
            34.0 + 0.02 * x - 0.01 * z + 1.5 * (x * 0.031).sin() * (z * 0.027).cos()
        });
        (core, Arc::new(far))
    }

    /// East of the scene's core, against it along the core's east edge
    /// for 40 of its 60 cells, reaching 20 cells north past its corner.
    const EAST: PatchPlan = PatchPlan {
        x0: 100,
        z0: -20,
        cells: 60,
    };

    /// Berlin's own heights for `plan` in the scene: the core's surface run
    /// on - the two read one survey where they meet - rising 3 m over the
    /// 40 m past the core's edge, with a ripple of its own, so the far
    /// field's blend has something to ease.
    fn own_heights(plan: PatchPlan, lattice: Lattice) -> Vec<f32> {
        let rect = plan.rect(lattice);
        let grid = plan.grid() as usize;
        (0..grid * grid)
            .map(|k| {
                let (x, z) = (
                    rect.min.x + (k % grid) as f32 * lattice.cell,
                    rect.min.y + (k / grid) as f32 * lattice.cell,
                );
                let past = ((x - lattice.half()) / 40.0).clamp(0.0, 1.0);
                34.0 + 0.02 * x - 0.01 * z
                    + 0.2 * (x * 0.3).sin()
                    + past * (3.0 + 0.4 * (x * 0.2).sin() * (z * 0.17).cos())
            })
            .collect()
    }

    #[test]
    fn a_patch_meets_the_core_and_the_far_field_at_its_edges() {
        let (core, far) = scene();
        let lattice = Lattice::of(&core);
        let inputs = PatchInputs {
            plan: EAST,
            core: core.clone(),
            far: far.clone(),
            level: None,
        };
        let own = own_heights(EAST, lattice);
        let mut heights = own.clone();
        blend(&mut heights, &inputs);
        let grid = EAST.grid() as usize;
        let rect = EAST.rect(lattice);
        let at = |i: usize, j: usize| {
            (
                rect.min.x + i as f32 * lattice.cell,
                rect.min.y + j as f32 * lattice.cell,
            )
        };
        let inner = far.inner_lines(lattice.half(), rect);
        assert_eq!(
            inner[0], rect.min.x,
            "its west edge is the core's: a grid line"
        );
        for j in 0..grid {
            for i in 0..grid {
                let h = heights[j * grid + i];
                let (x, z) = at(i, j);
                let lz = EAST.z0 + j as i32;
                if i == 0 && (0..=100).contains(&lz) {
                    // The core's own points, where the two share its edge.
                    assert_eq!(h, core.get(100, lz as usize), "({i}, {j})");
                } else if i == 0 || j == 0 || i == grid - 1 || j == grid - 1 {
                    // The far field's ground, round the rest of its edge.
                    let drawn = far.mesh_height_at(&core, x, z);
                    assert!((h - drawn).abs() < 1e-4, "({x}, {z}): {h} against {drawn}");
                } else if x > inner[1] {
                    // Within the far cells its east edge crosses.
                    let drawn = far.mesh_height_at(&core, x, z);
                    assert!((h - drawn).abs() < 1e-4, "({x}, {z}) in the strip");
                }
            }
        }
        // Clear of every edge by more than the blends: Berlin's own.
        let mut own_points = 0;
        for j in 0..grid {
            for i in 0..grid {
                let (x, z) = at(i, j);
                let from_far = (inner[1] - x)
                    .min(z - inner[2])
                    .min(inner[3] - z)
                    .min(Vec2::new(x, z).distance(Vec2::new(rect.min.x, rect.min.y.max(-100.0))));
                if from_far > BLEND_M && x - rect.min.x > CORE_BLEND_M {
                    assert_eq!(heights[j * grid + i], own[j * grid + i]);
                    own_points += 1;
                }
            }
        }
        assert!(own_points > 10, "{own_points} points of its own");
        // And nowhere a step: a point is never more than a metre from its
        // neighbour 2 m away.
        for j in 0..grid {
            for i in 1..grid {
                let step = (heights[j * grid + i] - heights[j * grid + i - 1]).abs();
                assert!(step < 1.0, "({i}, {j}): {step}");
            }
        }
    }

    /// `heights` as the ground of `plan`, every point on `cover`.
    fn ground_of(
        plan: PatchPlan,
        lattice: Lattice,
        heights: Vec<f32>,
        cover: Option<LandUse>,
    ) -> PatchGround {
        let grid = plan.grid() as usize;
        let mut map = HeightMap::new(grid, grid, lattice.cell);
        map.data_mut().copy_from_slice(&heights);
        PatchGround {
            plan,
            min: plan.rect(lattice).min,
            heights: map,
            cover: vec![cover; grid * grid],
        }
    }

    #[test]
    fn the_far_field_reads_its_patch_where_one_has_loaded() {
        let (core, far) = scene();
        let lattice = Lattice::of(&core);
        let heights = own_heights(EAST, lattice);
        let ground = Arc::new(ground_of(EAST, lattice, heights, Some(LandUse::Park)));
        let (inside, outside) = ((160.0, -80.0), (160.0, 80.0));
        let mesh = far.mesh_height_at(&core, inside.0, inside.1);
        assert_eq!(far.drawn_height_at(&core, inside.0, inside.1), mesh);
        far.patch().set(Some(ground.clone()));
        assert_eq!(
            far.drawn_height_at(&core, inside.0, inside.1),
            ground.height_at(inside.0, inside.1)
        );
        assert_ne!(far.drawn_height_at(&core, inside.0, inside.1), mesh);
        assert_eq!(
            far.drawn_height_at(&core, outside.0, outside.1),
            far.mesh_height_at(&core, outside.0, outside.1)
        );
        assert_eq!(far.cover_at(inside.0, inside.1), Some(LandUse::Park));
        assert_eq!(far.cover_at(outside.0, outside.1), None);
        assert_eq!(far.weights_at(inside.0, inside.1), [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(
            far.drawn_normal_at(&core, inside.0, inside.1),
            ground.normal_at(inside.0, inside.1)
        );
        // On the core, the core's own, patch or none.
        let on_core = far.drawn_height_at(&core, 20.0, 10.0);
        assert_eq!(on_core, core.get_height_at(120.0, 110.0));
        // A clone of the far field reads the same slot; let go, the mesh.
        let copy = (*far).clone();
        assert_eq!(
            copy.drawn_height_at(&core, inside.0, inside.1),
            ground.height_at(inside.0, inside.1)
        );
        far.patch().set(None);
        assert_eq!(copy.drawn_height_at(&core, inside.0, inside.1), mesh);
    }

    #[test]
    fn the_far_colliders_leave_out_the_cells_a_patch_fills_whole() {
        let (core, far) = scene();
        let lattice = Lattice::of(&core);
        let rect = EAST.rect(lattice);
        let colliders = far.colliders(&core, Some(rect));
        assert!(!colliders.is_empty());
        let down = |x: f32, z: f32| {
            colliders
                .iter()
                .filter_map(|collider| {
                    collider.cast_ray(
                        Vec3::ZERO,
                        Quat::IDENTITY,
                        Vec3::new(x, 500.0, z),
                        Vec3::NEG_Y,
                        1_000.0,
                        true,
                    )
                })
                .map(|(distance, _)| 500.0 - distance)
                .reduce(f32::max)
        };
        let inner = far.inner_lines(lattice.half(), rect);
        // Inside the cells it fills whole: no far ground under it.
        assert_eq!(down(160.3, -80.3), None);
        assert_eq!(down(inner[1] - 1.3, inner[2] + 1.3), None);
        // In the cells its edge crosses, and round it: the far field's.
        for (x, z) in [
            (inner[1] + 2.1, -80.3),
            (160.3, inner[2] - 2.1),
            (230.3, -80.3),
            (160.3, 0.3),
        ] {
            let drawn = far.mesh_height_at(&core, x, z);
            let walked = down(x, z).expect("far ground");
            assert!(
                (walked - drawn).abs() < 1e-3,
                "({x}, {z}): {walked} against {drawn}"
            );
        }
    }

    #[test]
    fn the_museumsinsel_decodes_as_a_patch_against_a_core() {
        // A core of 301 points 2 m apart, flat at 33 m; the recorded
        // Museumsinsel answers as the patch of 300 points against its east
        // edge, the Spree settled to its level.
        let mut core = HeightMap::new(301, 301, 2.0);
        core.data_mut().fill(33.0);
        let far = Arc::new(FarField::from_fn(64, 40.0, |_, _| 34.0));
        let plan = PatchPlan {
            x0: 300,
            z0: 0,
            cells: 299,
        };
        let inputs = PatchInputs {
            plan,
            core: core.clone(),
            far,
            level: Some(30.5),
        };
        let decoded = futures_lite::future::block_on(decode_patch(
            &crate::terrain::geo::tests::museum_bodies(),
            &inputs,
            &Pace::default(),
        ))
        .expect("the Museumsinsel decodes");
        assert!(decoded.lost.is_empty(), "{:?}", decoded.lost);
        let ground = decoded.ground;
        let lattice = Lattice::of(&core);
        let rect = plan.rect(lattice);
        assert_eq!(ground.rect(), rect);
        assert!(ground.heights().data().iter().all(|h| h.is_finite()));
        // Against the core, the core's own heights.
        for row in 0..300 {
            assert_eq!(ground.heights().get(0, row), 33.0);
        }
        // The Spree carved below its level in the middle of the patch.
        let middle = ground
            .heights()
            .data()
            .iter()
            .filter(|&&h| h < 30.5)
            .count();
        assert!(middle > 500, "{middle} points of river");
        // Its street level carried into the world, inside the patch.
        let level = decoded.street_level.expect("its street level");
        assert!(level.buildings.len() > 20 && level.trees.len() > 100);
        let held = |x: f32, z: f32| rect.contains(Vec2::new(x, z));
        assert!(level.trees.iter().all(|t| held(t.x, t.z)));
        assert!(level.furniture.iter().all(|f| held(f.x, f.z)));
        assert!(
            level
                .buildings
                .iter()
                .all(|b| b.outline.iter().all(|&(x, z)| held(x, z)))
        );
        assert!(decoded.streets.is_some(), "its streets");
    }

    /// The default core's lattice: 512 points 2 m apart.
    fn lattice() -> Lattice {
        Lattice {
            cell: 2.0,
            cells: 511,
        }
    }

    #[test]
    fn a_patch_stands_against_the_core_on_the_side_the_body_is_nearest() {
        let lattice = lattice();
        let span = 9_000.0;
        // Just north of the core's north edge (z = -511): the patch stands
        // against that edge, the core's size, the body inside it.
        let plan = plan_for(Vec2::new(30.0, -560.0), lattice, span).unwrap();
        assert_eq!(plan.cells, 511);
        assert_eq!(plan.z0 + plan.cells as i32, 0, "against the north edge");
        let rect = plan.rect(lattice);
        assert!(rect.contains(Vec2::new(30.0, -560.0)));
        assert_eq!(rect.max.y, -lattice.half());
        // Inside the core near its east edge: pushed out east.
        let east = plan_for(Vec2::new(480.0, 40.0), lattice, span).unwrap();
        assert_eq!(east.x0, 511);
        // Beside the core but short of it by less than a step: against it,
        // no strip of far field left between.
        let near = plan_for(Vec2::new(1_200.0, 100.0), lattice, span).unwrap();
        assert_eq!(near.x0, 511, "pulled against the east edge");
        assert!(near.rect(lattice).contains(Vec2::new(1_200.0, 100.0)));
        // Far out: no push, and centred within half a step of the body.
        let far = plan_for(Vec2::new(3_000.0, 2_000.0), lattice, span).unwrap();
        let centre = far.rect(lattice).center();
        assert!((centre.x - 3_000.0).abs() <= PATCH_STEP_M / 2.0 + lattice.cell);
        assert!((centre.y - 2_000.0).abs() <= PATCH_STEP_M / 2.0 + lattice.cell);
    }

    #[test]
    fn a_patch_snaps_to_its_step_so_a_return_asks_for_the_same_one() {
        let lattice = lattice();
        let a = plan_for(Vec2::new(2_000.0, 1_500.0), lattice, 9_000.0).unwrap();
        let b = plan_for(Vec2::new(2_040.0, 1_530.0), lattice, 9_000.0).unwrap();
        assert_eq!(a, b);
        let c = plan_for(Vec2::new(2_200.0, 1_500.0), lattice, 9_000.0).unwrap();
        assert_ne!(a, c);
        assert_eq!((c.x0 - a.x0) % 125, 0, "a whole step of 125 cells");
    }

    #[test]
    fn a_patch_keeps_inside_the_far_field_and_narrows_to_its_ground() {
        let lattice = lattice();
        // A 3 km square's far field: 1,480 m either way, 969 m of ground
        // beside the core - narrower than the core.
        let span = 1_480.0;
        let plan = plan_for(Vec2::new(1_400.0, 0.0), lattice, span).unwrap();
        let rect = plan.rect(lattice);
        assert!(plan.cells < 511);
        assert!(rect.max.x <= span && rect.min.x >= lattice.half());
        assert!(rect.min.y >= -span && rect.max.y <= span);
        // Ground narrower than a patch is worth: none.
        assert_eq!(plan_for(Vec2::new(700.0, 0.0), lattice, 700.0), None);
    }

    #[test]
    fn a_patch_box_puts_its_pixels_on_the_cores_points() {
        let lattice = lattice();
        // The default core of a square at E 390000, N 5810000: 1,022 m,
        // 512 pixels.
        let core = Bbox {
            min_e: 389_489,
            min_n: 5_809_489,
            max_e: 390_511,
            max_n: 5_810_511,
        };
        let plan = PatchPlan {
            x0: 511,
            z0: -40,
            cells: 511,
        };
        let bbox = plan.bbox(core, 512);
        let pixel = (core.max_e - core.min_e) as f64 / 512.0;
        let patch_pixel = (bbox.max_e - bbox.min_e) as f64 / 512.0;
        // The patch's first pixel centre is the core's last: the point they
        // share on the core's east edge.
        let shared = core.min_e as f64 + (511.0 + 0.5) * pixel;
        assert!((bbox.min_e as f64 + 0.5 * patch_pixel - shared).abs() <= 0.5);
        // Forty points north of the core's first row.
        let row = core.max_n as f64 - (-40.0 + 0.5) * pixel;
        assert!((bbox.max_n as f64 - 0.5 * patch_pixel - row).abs() <= 0.5);
        assert_eq!(bbox.max_e - bbox.min_e, bbox.max_n - bbox.min_n);
        let _ = lattice;
    }

    #[test]
    fn a_body_keeps_its_patch_until_it_moves_on_and_lets_it_go_deep_in_the_core() {
        let lattice = lattice();
        let span = 9_000.0;
        let out = Vec2::new(1_500.0, 300.0);
        let Want::Load { plan, asked } = want(out, None, lattice, span) else {
            panic!("a body past the core wants a patch");
        };
        let current = Some((plan, asked));
        // Pacing about where it asked: the patch serves.
        for step in [-150.0, 0.0, 150.0] {
            let at = out + Vec2::new(step, -step);
            assert_eq!(want(at, current, lattice, span), Want::Stay);
        }
        // Past three quarters of a step: the next patch.
        let on = out + Vec2::new(0.8 * PATCH_STEP_M, 0.0);
        assert!(matches!(
            want(on, current, lattice, span),
            Want::Load { .. }
        ));
        // Near the core's edge inside it, a patch is wanted; deep inside,
        // none, and the one it had goes.
        assert!(matches!(
            want(Vec2::new(0.0, -400.0), None, lattice, span),
            Want::Load { .. }
        ));
        assert_eq!(want(Vec2::ZERO, None, lattice, span), Want::Stay);
        assert_eq!(want(Vec2::ZERO, current, lattice, span), Want::Drop);
    }

    #[test]
    fn a_patch_shares_the_cores_boundary_only_where_it_stands_against_it() {
        let lattice = lattice();
        let north = PatchPlan {
            x0: -100,
            z0: -511,
            cells: 511,
        };
        assert_eq!(north.shared_edge(lattice), Some((0, false, (0, 411))));
        let east = PatchPlan {
            x0: 511,
            z0: 300,
            cells: 511,
        };
        assert_eq!(east.shared_edge(lattice), Some((511, true, (300, 511))));
        let corner = PatchPlan {
            x0: 511,
            z0: 511,
            cells: 511,
        };
        assert_eq!(corner.shared_edge(lattice), Some((511, true, (511, 511))));
        let apart = PatchPlan {
            x0: 600,
            z0: 0,
            cells: 511,
        };
        assert_eq!(apart.shared_edge(lattice), None);
    }
}
