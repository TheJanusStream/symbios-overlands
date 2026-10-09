//! Berlin's street level on the walkable ground (#1588): its buildings,
//! trees and street furniture, read from the WFS pages the core fetches
//! ([`geodata::berlin::BUILDINGS`], the two tree inventories, the street
//! survey's common kinds) and carried into the world frame round the core,
//! where the derived stage draws them ([`crate::terrain::derived::core`]).
//!
//! A building's parts - a high-rise section, a lower wing - only raise the
//! building's peak storeys: the building alone stands. An underground car
//! park stands nothing. Whatever reaches past [`EDGE_MARGIN_M`] inside the
//! core's edge is left out: its walls stop there.
//!
//! Each building and item of furniture is turned to its street
//! ([`Facing`]): across to the nearest carriageway the core draws, within
//! [`FACING_REACH_M`], and elsewhere down the slope of the land use's
//! distance to street space. A sign faces the traffic coming at it, which
//! keeps to the right; a bench or a rack surveyed with a long side keeps
//! its own line, its front the side nearer the street. Two items of a kind
//! within [`DUPLICATE_M`] of each other are one, the lower id kept: the
//! sign survey records some signs twice, and two signs at one spot draw as
//! one.
//!
//! Each layer stands or falls on its own page: a page that could not be had
//! or read leaves its layer out, says why, and the rest still stand.

use std::collections::HashMap;
use std::sync::Arc;

use geodata::berlin::{BuildingUse, FurnitureKind, LandUse};

use super::streets::{CoreFrame, Streets};

/// How far inside the core's edge the street level stops (m): its walls.
pub(crate) const EDGE_MARGIN_M: f32 = 3.0;

/// How far from a carriageway a building or an item still faces it (m).
pub(crate) const FACING_REACH_M: f32 = 60.0;

/// How near a carriageway's line an item stands on it (m): a sign on a
/// median, which faces by the land use instead.
const ON_AXIS_M: f32 = 0.3;

/// How near two items of a kind are one (m).
pub(crate) const DUPLICATE_M: f32 = 0.5;

/// The side of the carriageway index's buckets (m).
const BUCKET_M: f32 = 32.0;

/// How far from an inventory tree no seeded tree stands (m): about a
/// crown's width. The inventory counts the trees of the city's streets and
/// parks; a seeded stand fills the ground they leave, and Berlin's forests,
/// which it does not count, keep their stands whole.
pub(crate) const STAND_CLEAR_M: f32 = 8.0;

/// One building of the walkable ground, in the world frame round the core.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CoreBuilding {
    /// Its ALKIS uuid.
    pub id: Arc<str>,
    /// Its footprint's outer ring, closed, in world `(x, z)`.
    pub outline: Vec<(f32, f32)>,
    pub usage: BuildingUse,
    /// Its storeys above ground, where ALKIS has them.
    pub storeys: Option<u8>,
    /// The most storeys it or any of its parts rises.
    pub peak_storeys: Option<u8>,
    /// Its footprint's area (m2).
    pub area: f32,
    /// The turn about +Y that faces a catalogue front (local -Z) to its
    /// street, from its centroid.
    pub street_yaw: f32,
}

/// One tree of the inventory, in the world frame round the core.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CoreTree {
    /// Its inventory gisid.
    pub id: Arc<str>,
    pub x: f32,
    pub z: f32,
    /// Its genus, in Latin.
    pub genus: Option<String>,
    /// Its height and crown diameter (m), and its trunk's girth (cm).
    pub height: Option<f32>,
    pub crown: Option<f32>,
    pub girth: Option<f32>,
}

/// One item of street furniture, in the world frame round the core.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CoreFurniture {
    /// Its survey id.
    pub id: Arc<str>,
    pub kind: FurnitureKind,
    pub x: f32,
    pub z: f32,
    /// The turn about +Y that faces a catalogue front (local -Z) to its
    /// street - a sign's to its traffic - or across its own long side, to
    /// the street side, where it has one.
    pub yaw: f32,
    /// How long its long side is (m), where it has one.
    pub length: Option<f32>,
}

/// The walkable ground's street level, each layer nearest the core's
/// centre first.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StreetLevel {
    pub buildings: Vec<CoreBuilding>,
    pub trees: Vec<CoreTree>,
    pub furniture: Vec<CoreFurniture>,
    /// Where its trees keep a seeded stand clear, built once with them.
    clearance: TreeClearance,
}

impl StreetLevel {
    pub(crate) fn new(
        buildings: Vec<CoreBuilding>,
        trees: Vec<CoreTree>,
        furniture: Vec<CoreFurniture>,
    ) -> Self {
        let mut cells: HashMap<(i32, i32), Vec<(f32, f32)>> = HashMap::new();
        for tree in &trees {
            cells
                .entry(TreeClearance::cell(tree.x, tree.z))
                .or_default()
                .push((tree.x, tree.z));
        }
        StreetLevel {
            buildings,
            trees,
            furniture,
            clearance: TreeClearance { cells },
        }
    }

    /// Where its inventory trees keep a seeded stand clear.
    pub(crate) fn tree_clearance(&self) -> &TreeClearance {
        &self.clearance
    }

    /// Whether nothing of it could be read.
    pub(crate) fn is_empty(&self) -> bool {
        self.buildings.is_empty() && self.trees.is_empty() && self.furniture.is_empty()
    }
}

/// The inventory's trees, bucketed [`STAND_CLEAR_M`] a side, so whether one
/// stands near a point is a look at nine buckets.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TreeClearance {
    cells: HashMap<(i32, i32), Vec<(f32, f32)>>,
}

impl TreeClearance {
    fn cell(x: f32, z: f32) -> (i32, i32) {
        (
            (x / STAND_CLEAR_M).floor() as i32,
            (z / STAND_CLEAR_M).floor() as i32,
        )
    }

    /// Whether an inventory tree stands within [`STAND_CLEAR_M`] of world
    /// `(x, z)`.
    pub(crate) fn near(&self, x: f32, z: f32) -> bool {
        let (cx, cz) = Self::cell(x, z);
        (cz - 1..=cz + 1).any(|row| {
            (cx - 1..=cx + 1).any(|col| {
                self.cells.get(&(col, row)).is_some_and(|trees| {
                    trees
                        .iter()
                        .any(|&(tx, tz)| (tx - x).hypot(tz - z) < STAND_CLEAR_M)
                })
            })
        })
    }
}

/// One page as fetched, or why it could not be had, as a sentence.
pub(crate) type Page = Result<Arc<[u8]>, String>;

/// The street level's pages as fetched: the buildings', the two tree
/// inventories', and each furniture kind's.
pub(crate) struct StreetLevelBodies {
    pub buildings: Page,
    pub street_trees: Page,
    pub park_trees: Page,
    pub furniture: Vec<(FurnitureKind, Page)>,
}

/// Read the street level's pages and carry them into the world frame round
/// the core `frame`, its `streets` and land use `cover` turning each
/// building and item to its street (see the module docs). Answers what was
/// read, and why each layer left out was: a page that could not be had or
/// read, in the order the pages are asked for. A page the server cut short
/// still draws what it holds.
pub(crate) fn decode_street_level(
    bodies: &StreetLevelBodies,
    frame: CoreFrame,
    cover: &[Option<LandUse>],
    streets: Option<&Streets>,
) -> (StreetLevel, Vec<String>) {
    let mut lost = Vec::new();
    let half = frame.extent() / 2.0;
    let world = |p: [f64; 2]| {
        let (x, z) = frame.local(p);
        (x - half, z - half)
    };
    let inside = |(x, z): (f32, f32)| {
        let reach = half - EDGE_MARGIN_M;
        x.abs() <= reach && z.abs() <= reach
    };
    let carriageways = streets.map(|streets| streets.segments(frame));
    let facing = Facing::new(frame, cover, carriageways.unwrap_or_default());

    // Buildings: whole ones stand, their parts raise their peaks.
    let mut whole: Vec<CoreBuilding> = Vec::new();
    let page = read_page(
        &bodies.buildings,
        "buildings",
        geodata::berlin::parse_buildings,
        &mut lost,
    );
    if let Some(page) = page {
        warn_cut_short("buildings", page.features, page.matched);
        let mut parts: Vec<((f32, f32), u8)> = Vec::new();
        for building in &page.buildings {
            let Some(ring) = building
                .outlines
                .iter()
                .max_by(|a, b| ring_area(a).total_cmp(&ring_area(b)))
            else {
                continue;
            };
            let outline: Vec<(f32, f32)> = ring.iter().map(|&p| world(p)).collect();
            if !outline.iter().all(|&p| inside(p)) {
                continue;
            }
            let centre = centroid(&outline);
            if building.part {
                if let Some(storeys) = building.storeys {
                    parts.push((centre, storeys));
                }
                continue;
            }
            if building.usage() == BuildingUse::Underground {
                continue;
            }
            whole.push(CoreBuilding {
                id: building.uuid.as_str().into(),
                area: outline_area(&outline),
                street_yaw: facing.toward(centre).map_or(0.0, yaw_towards),
                outline,
                usage: building.usage(),
                storeys: building.storeys,
                peak_storeys: building.storeys,
            });
        }
        // Each part to the building whose footprint holds its middle: the
        // boxes first, the even-odd test only inside one.
        let boxes: Vec<[f32; 4]> = whole.iter().map(|b| bounds(&b.outline)).collect();
        for (at, storeys) in parts {
            let holder = (0..whole.len()).find(|&i| {
                let [x0, z0, x1, z1] = boxes[i];
                (x0..=x1).contains(&at.0)
                    && (z0..=z1).contains(&at.1)
                    && contains(&whole[i].outline, at)
            });
            if let Some(i) = holder {
                let building = &mut whole[i];
                building.peak_storeys =
                    Some(building.peak_storeys.map_or(storeys, |s| s.max(storeys)));
            }
        }
    }

    // Trees, street and park.
    let mut trees = Vec::new();
    for (what, body) in [
        ("street trees", &bodies.street_trees),
        ("park trees", &bodies.park_trees),
    ] {
        let Some(page) = read_page(body, what, geodata::berlin::parse_trees, &mut lost) else {
            continue;
        };
        warn_cut_short(what, page.features, page.matched);
        for tree in page.trees {
            let (x, z) = world(tree.at);
            if inside((x, z)) {
                trees.push(CoreTree {
                    id: tree.id.into(),
                    x,
                    z,
                    genus: tree.genus,
                    height: tree.height,
                    crown: tree.crown,
                    girth: tree.girth,
                });
            }
        }
    }

    // Furniture, each kind's page.
    let mut furniture = Vec::new();
    for (kind, body) in &bodies.furniture {
        let parse = |body: &[u8]| geodata::berlin::parse_furniture(*kind, body);
        let Some(page) = read_page(body, kind.name(), parse, &mut lost) else {
            continue;
        };
        warn_cut_short(kind.name(), page.features, page.matched);
        let items = page
            .items
            .into_iter()
            .filter_map(|item| {
                let at = world(item.at);
                inside(at).then(|| CoreFurniture {
                    id: item.id.into(),
                    kind: *kind,
                    x: at.0,
                    z: at.1,
                    yaw: item_yaw(*kind, item.axis, facing.toward(at)),
                    length: item.length.map(|l| l as f32),
                })
            })
            .collect();
        furniture.extend(dedupe(items));
    }

    // Nearest the centre first, ties by id: an order that does not hang on
    // the server's.
    let near = |x: f32, z: f32| x * x + z * z;
    whole.sort_by(|a, b| {
        let (ca, cb) = (centroid(&a.outline), centroid(&b.outline));
        near(ca.0, ca.1)
            .total_cmp(&near(cb.0, cb.1))
            .then(a.id.cmp(&b.id))
    });
    trees.sort_by(|a, b| {
        near(a.x, a.z)
            .total_cmp(&near(b.x, b.z))
            .then(a.id.cmp(&b.id))
    });
    furniture.sort_by(|a, b| {
        near(a.x, a.z)
            .total_cmp(&near(b.x, b.z))
            .then(a.id.cmp(&b.id))
    });
    (StreetLevel::new(whole, trees, furniture), lost)
}

/// A layer's `page`, read by `parse`, or `None`, and why pushed onto `lost`.
fn read_page<T>(
    page: &Page,
    what: &str,
    parse: impl FnOnce(&[u8]) -> Result<T, geodata::features::FeatureError>,
    lost: &mut Vec<String>,
) -> Option<T> {
    let read = page.as_ref().map_err(String::clone).and_then(|body| {
        parse(body).map_err(|e| format!("Berlin's {what} could not be read: {e}."))
    });
    read.map_err(|reason| lost.push(reason)).ok()
}

/// Say so where a page of `what` held fewer features than the server
/// matched - counted before any was left out, so a lamp page less its
/// switch cabinets is not cut short.
fn warn_cut_short(what: &str, held: usize, matched: Option<u64>) {
    if let Some(matched) = matched.filter(|&m| m > held as u64) {
        bevy::log::warn!(
            "geodata street level: {held} of {matched} {what} in one page - drawing those"
        );
    }
}

/// Items of one kind less those within [`DUPLICATE_M`] of one already kept,
/// the lower ids kept first, so which stays does not hang on the order the
/// server sent them in.
pub(crate) fn dedupe(mut items: Vec<CoreFurniture>) -> Vec<CoreFurniture> {
    items.sort_by(|a, b| a.id.cmp(&b.id));
    let cell = |x: f32, z: f32| {
        (
            (x / DUPLICATE_M).floor() as i32,
            (z / DUPLICATE_M).floor() as i32,
        )
    };
    let mut kept: HashMap<(i32, i32), Vec<(f32, f32)>> = HashMap::new();
    items
        .into_iter()
        .filter(|item| {
            let (cx, cz) = cell(item.x, item.z);
            let taken = (cz - 1..=cz + 1).any(|row| {
                (cx - 1..=cx + 1).any(|col| {
                    kept.get(&(col, row)).is_some_and(|spots| {
                        spots
                            .iter()
                            .any(|&(x, z)| (x - item.x).hypot(z - item.z) < DUPLICATE_M)
                    })
                })
            });
            if !taken {
                kept.entry((cx, cz)).or_default().push((item.x, item.z));
            }
            !taken
        })
        .collect()
}

/// An outline's box, `[min x, min z, max x, max z]`.
fn bounds(outline: &[(f32, f32)]) -> [f32; 4] {
    outline.iter().fold(
        [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
        |[x0, z0, x1, z1], &(x, z)| [x0.min(x), z0.min(z), x1.max(x), z1.max(z)],
    )
}

/// The turn about +Y that faces a catalogue front (local -Z) along
/// `direction`: a front turned by `yaw` looks along (-sin yaw, -cos yaw).
pub(crate) fn yaw_towards(direction: (f32, f32)) -> f32 {
    libm::atan2f(-direction.0, -direction.1)
}

/// The turn of an item of `kind` (see the module docs): `axis` is its long
/// side as the survey has it (E/N), `street` the way to its street.
pub(crate) fn item_yaw(
    kind: FurnitureKind,
    axis: Option<[f64; 2]>,
    street: Option<(f32, f32)>,
) -> f32 {
    match (axis, street) {
        // A long side runs along the street: the front looks across it -
        // east is world +x, north world -z - to the street side.
        (Some([e, n]), street) => {
            let across = (n as f32, e as f32);
            let towards = street.is_none_or(|s| across.0 * s.0 + across.1 * s.1 >= 0.0);
            yaw_towards(if towards {
                across
            } else {
                (-across.0, -across.1)
            })
        }
        // A sign stands on the right of the traffic it is for, and looks
        // back along the street at it.
        (None, Some(s)) if kind == FurnitureKind::Sign => yaw_towards((s.1, -s.0)),
        (None, Some(s)) => yaw_towards(s),
        (None, None) => 0.0,
    }
}

/// Which way a point's street lies (see the module docs).
pub(crate) struct Facing {
    /// The carriageways' segments in the world frame, bucketed.
    carriageways: Vec<((f32, f32), (f32, f32))>,
    buckets: Vec<Vec<u32>>,
    side: usize,
    half: f32,
    /// Four-neighbour steps to street space, per pixel of the land use.
    to_street: Vec<u32>,
    grid: usize,
    cell: f32,
}

impl Facing {
    /// For the core `frame`, its land use `cover` and its carriageways'
    /// `segments` in the frame ([`Streets::segments`]).
    pub(crate) fn new(
        frame: CoreFrame,
        cover: &[Option<LandUse>],
        segments: Vec<((f32, f32), (f32, f32))>,
    ) -> Self {
        let half = frame.extent() / 2.0;
        let carriageways: Vec<_> = segments
            .into_iter()
            .map(|(a, b)| ((a.0 - half, a.1 - half), (b.0 - half, b.1 - half)))
            .collect();
        let side = ((2.0 * half) / BUCKET_M).ceil().max(1.0) as usize;
        let bucket = |v: f32| (((v + half) / BUCKET_M).floor().max(0.0) as usize).min(side - 1);
        let mut buckets = vec![Vec::new(); side * side];
        for (i, &(a, b)) in carriageways.iter().enumerate() {
            // Every bucket a point within reach of the segment may lie in.
            let (x0, x1) = (a.0.min(b.0) - FACING_REACH_M, a.0.max(b.0) + FACING_REACH_M);
            let (z0, z1) = (a.1.min(b.1) - FACING_REACH_M, a.1.max(b.1) + FACING_REACH_M);
            if x1 < -half || z1 < -half || x0 > half || z0 > half {
                continue;
            }
            for row in bucket(z0)..=bucket(z1) {
                for col in bucket(x0)..=bucket(x1) {
                    buckets[row * side + col].push(i as u32);
                }
            }
        }
        Facing {
            carriageways,
            buckets,
            side,
            half,
            to_street: super::ring::street_distance(cover, frame.grid as usize),
            grid: frame.grid as usize,
            cell: frame.cell,
        }
    }

    /// The unit direction from `p` to its street: across to the nearest
    /// carriageway within [`FACING_REACH_M`], unless `p` stands on its line;
    /// else down the slope of the distance to street space; `None` where
    /// that is flat too.
    pub(crate) fn toward(&self, p: (f32, f32)) -> Option<(f32, f32)> {
        if let Some(q) = self.nearest(p) {
            let (dx, dz) = (q.0 - p.0, q.1 - p.1);
            let d = dx.hypot(dz);
            if d > ON_AXIS_M {
                return Some((dx / d, dz / d));
            }
        }
        self.downslope(p)
    }

    /// The nearest point of a carriageway within [`FACING_REACH_M`] of `p`.
    fn nearest(&self, p: (f32, f32)) -> Option<(f32, f32)> {
        let bucket = |v: f32| ((v + self.half) / BUCKET_M).floor();
        let (col, row) = (bucket(p.0), bucket(p.1));
        let range = 0.0..self.side as f32;
        if !range.contains(&col) || !range.contains(&row) {
            return None;
        }
        self.buckets[row as usize * self.side + col as usize]
            .iter()
            .map(|&i| {
                let q = foot(self.carriageways[i as usize], p);
                (q, (q.0 - p.0).hypot(q.1 - p.1))
            })
            .filter(|&(_, d)| d <= FACING_REACH_M)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(q, _)| q)
    }

    /// Down the slope of the distance to street space at `p`.
    fn downslope(&self, (x, z): (f32, f32)) -> Option<(f32, f32)> {
        let n = self.grid;
        let pixel = |v: f32| {
            ((v + self.half) / self.cell)
                .round()
                .clamp(0.0, (n - 1) as f32) as usize
        };
        let (px, pz) = (pixel(x), pixel(z));
        let d = |x: usize, z: usize| self.to_street[z.min(n - 1) * n + x.min(n - 1)] as f32;
        let (gx, gz) = (
            d(px + 1, pz) - d(px.saturating_sub(1), pz),
            d(px, pz + 1) - d(px, pz.saturating_sub(1)),
        );
        let g = gx.hypot(gz);
        (g > 0.0 && g.is_finite()).then(|| (-gx / g, -gz / g))
    }
}

/// The point of the segment `(a, b)` nearest `p`.
fn foot((a, b): ((f32, f32), (f32, f32)), p: (f32, f32)) -> (f32, f32) {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let length2 = dx * dx + dz * dz;
    let t = if length2 > 1e-12 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dz) / length2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a.0 + dx * t, a.1 + dz * t)
}

/// A ring's area, unsigned, by the shoelace (E/N).
fn ring_area(ring: &[[f64; 2]]) -> f64 {
    ring.windows(2)
        .map(|w| w[0][0] * w[1][1] - w[1][0] * w[0][1])
        .sum::<f64>()
        .abs()
        / 2.0
}

/// An outline's area (m2).
pub(crate) fn outline_area(outline: &[(f32, f32)]) -> f32 {
    outline
        .windows(2)
        .map(|w| w[0].0 * w[1].1 - w[1].0 * w[0].1)
        .sum::<f32>()
        .abs()
        / 2.0
}

/// An outline's area-weighted centroid; its vertices' mean where it has no
/// area.
pub(crate) fn centroid(outline: &[(f32, f32)]) -> (f32, f32) {
    let signed: f32 = outline
        .windows(2)
        .map(|w| w[0].0 * w[1].1 - w[1].0 * w[0].1)
        .sum::<f32>()
        / 2.0;
    if signed.abs() < 1e-6 {
        let n = outline.len().max(1) as f32;
        let (x, z) = outline
            .iter()
            .fold((0.0, 0.0), |(x, z), p| (x + p.0, z + p.1));
        return (x / n, z / n);
    }
    let (mut x, mut z) = (0.0, 0.0);
    for w in outline.windows(2) {
        let cross = w[0].0 * w[1].1 - w[1].0 * w[0].1;
        x += (w[0].0 + w[1].0) * cross;
        z += (w[0].1 + w[1].1) * cross;
    }
    (x / (6.0 * signed), z / (6.0 * signed))
}

/// Whether the closed `outline` holds `p` (even-odd).
pub(crate) fn contains(outline: &[(f32, f32)], p: (f32, f32)) -> bool {
    let mut inside = false;
    for w in outline.windows(2) {
        let (a, b) = (w[0], w[1]);
        if (a.1 > p.1) != (b.1 > p.1) {
            let x = a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0);
            if p.0 < x {
                inside = !inside;
            }
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::{Quat, Vec3};
    use geodata::request::Bbox;

    /// Where a front turned by `yaw` looks, as Bevy turns it.
    fn front(yaw: f32) -> (f32, f32) {
        let f = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
        (f.x, f.z)
    }

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-5 && (a.1 - b.1).abs() < 1e-5
    }

    #[test]
    fn a_turn_faces_a_catalogue_front_where_it_says() {
        for k in 0..16 {
            let a = k as f32 * std::f32::consts::TAU / 16.0;
            let d = (a.cos(), a.sin());
            assert!(close(front(yaw_towards(d)), d), "{d:?}");
        }
    }

    /// A 100 m core with one carriageway running north-south through its
    /// middle, and no street space in its land use.
    fn crossed() -> Facing {
        let frame = CoreFrame {
            bbox: Bbox {
                min_e: 0,
                min_n: 0,
                max_e: 102,
                max_n: 102,
            },
            grid: 51,
            cell: 2.0,
        };
        let cover = vec![Some(LandUse::Housing); 51 * 51];
        Facing::new(frame, &cover, vec![((50.0, 0.0), (50.0, 100.0))])
    }

    #[test]
    fn an_item_faces_across_to_its_carriageway_and_a_sign_its_traffic() {
        let facing = crossed();
        let east = (5.0, 10.0);
        let toward = |p| facing.toward(p).expect("a street");
        assert!(close(toward(east), (-1.0, 0.0)));
        assert!(close(toward((-7.0, -3.0)), (1.0, 0.0)));
        // On the line, or past reach, the land use says nothing here.
        assert_eq!(facing.toward((0.1, 0.0)), None);
        assert_eq!(facing.toward((FACING_REACH_M + 1.0, 0.0)), None);

        let street = facing.toward(east);
        assert!(close(
            front(item_yaw(FurnitureKind::Lamp, None, street)),
            (-1.0, 0.0)
        ));
        // East of a north-south street the traffic drives north, coming
        // from the south (world +z): the sign looks south at it.
        assert!(close(
            front(item_yaw(FurnitureKind::Sign, None, street)),
            (0.0, 1.0)
        ));
        let west = facing.toward((-5.0, 10.0));
        assert!(close(
            front(item_yaw(FurnitureKind::Sign, None, west)),
            (0.0, -1.0)
        ));
        // A bench along the street (north-south, E/N [0, 1]) looks across
        // it, to the street side; one across it keeps its own line.
        let along = Some([0.0, 1.0]);
        assert!(close(
            front(item_yaw(FurnitureKind::Bench, along, street)),
            (-1.0, 0.0)
        ));
        assert!(close(
            front(item_yaw(FurnitureKind::Bench, along, west)),
            (1.0, 0.0)
        ));
        let across = Some([1.0, 0.0]);
        let f = front(item_yaw(FurnitureKind::Bench, across, street));
        assert!(close(f, (0.0, 1.0)) || close(f, (0.0, -1.0)), "{f:?}");
    }

    #[test]
    fn with_no_carriageway_an_item_faces_down_to_street_space() {
        // Street space along the core's west edge.
        let frame = CoreFrame {
            bbox: Bbox {
                min_e: 0,
                min_n: 0,
                max_e: 40,
                max_n: 40,
            },
            grid: 20,
            cell: 2.0,
        };
        let cover: Vec<Option<LandUse>> = (0..400)
            .map(|i| (i % 20 >= 2).then_some(LandUse::Housing))
            .collect();
        let facing = Facing::new(frame, &cover, Vec::new());
        assert!(close(
            facing.toward((4.0, 0.0)).expect("downhill"),
            (-1.0, 0.0)
        ));
    }

    #[test]
    fn a_seeded_stand_keeps_a_crowns_width_off_each_inventory_tree() {
        let tree = |x: f32, z: f32| CoreTree {
            id: "t".into(),
            x,
            z,
            genus: None,
            height: None,
            crown: None,
            girth: None,
        };
        let level = StreetLevel::new(
            Vec::new(),
            vec![tree(10.0, -5.0), tree(-0.5, 0.5)],
            Vec::new(),
        );
        let clear = level.tree_clearance();
        assert!(clear.near(10.0 + STAND_CLEAR_M - 0.1, -5.0));
        assert!(!clear.near(10.0 + STAND_CLEAR_M + 0.1, -5.0));
        assert!(
            clear.near(5.0, -10.0),
            "7.1 m off, across a bucket's corner"
        );
        // Across the origin, where the buckets' signs change.
        assert!(clear.near(-0.5 - 7.9, 0.5) && clear.near(-0.5, 0.5 + 7.9));
        assert!(!clear.near(-30.0, 40.0));
        assert!(!StreetLevel::default().tree_clearance().near(0.0, 0.0));
    }

    /// Two items of a kind within half a metre are one, the lower id kept,
    /// whatever order they came in and whichever side of a bucket's edge
    /// they stand.
    #[test]
    fn two_items_on_one_spot_are_one_the_lower_id_kept() {
        let item = |id: &str, x: f32| CoreFurniture {
            id: id.into(),
            kind: FurnitureKind::Sign,
            x,
            z: 3.0,
            yaw: 0.0,
            length: None,
        };
        let items = vec![
            item("b", 0.0),
            item("a", 0.3),
            item("c", 5.0),
            item("d", 5.6),
            item("f", 10.51),
            item("e", 10.49),
        ];
        let ids = |items: Vec<CoreFurniture>| -> Vec<String> {
            dedupe(items).iter().map(|i| i.id.to_string()).collect()
        };
        assert_eq!(ids(items.clone()), ["a", "c", "d", "e"]);
        let mut reversed = items;
        reversed.reverse();
        assert_eq!(ids(reversed), ["a", "c", "d", "e"]);
    }

    /// The Museumsinsel's street level, read with its ground (see
    /// `geo::tests::museum_level`).
    #[test]
    fn the_museumsinsel_street_level_lands_round_the_core() {
        let level = super::super::tests::museum_level();
        let reach = 299.0 - EDGE_MARGIN_M;
        let inside = |(x, z): (f32, f32)| x.abs() <= reach && z.abs() <= reach;
        // Its buildings: whole ones only, none underground, every one on
        // the walkable ground; the cathedral among them; a part raises its
        // building's peak.
        let buildings = &level.buildings;
        // Of the square's 66, those wholly inside its walls.
        assert_eq!(buildings.len(), 34);
        assert!(
            buildings
                .iter()
                .all(|b| b.outline.iter().all(|&p| inside(p)))
        );
        assert!(
            buildings
                .iter()
                .all(|b| b.usage != BuildingUse::Underground)
        );
        let worship: Vec<_> = buildings
            .iter()
            .filter(|b| b.usage == BuildingUse::Religious)
            .collect();
        assert_eq!(worship.len(), 1, "the Berliner Dom");
        assert!(worship[0].area > 5_000.0, "{}", worship[0].area);
        assert!(
            buildings
                .iter()
                .any(|b| b.peak_storeys > b.storeys && b.storeys.is_some())
        );
        // Its trees, street and park.
        assert!(
            (440..=467).contains(&level.trees.len()),
            "{}",
            level.trees.len()
        );
        assert!(level.trees.iter().all(|t| inside((t.x, t.z))));
        // Its furniture, counted apart from the code (a Python pass over the
        // fixtures): of the 140 signs inside the walls, the four recorded
        // twice are one each; no lamp is doubled.
        let count = |kind| level.furniture.iter().filter(|f| f.kind == kind).count();
        assert_eq!(count(FurnitureKind::Sign), 136);
        assert_eq!(count(FurnitureKind::Lamp), 179);
        for kind in FurnitureKind::ALL {
            let of_kind: Vec<_> = level.furniture.iter().filter(|f| f.kind == kind).collect();
            for (i, a) in of_kind.iter().enumerate() {
                for b in &of_kind[i + 1..] {
                    assert!(
                        (a.x - b.x).hypot(a.z - b.z) >= DUPLICATE_M,
                        "{} and {} on one spot",
                        a.id,
                        b.id
                    );
                }
            }
        }
        // Nearest the centre first, each kind of item.
        let near = |x: f32, z: f32| x * x + z * z;
        assert!(
            level
                .trees
                .windows(2)
                .all(|w| near(w[0].x, w[0].z) <= near(w[1].x, w[1].z))
        );
        assert!(
            level
                .furniture
                .windows(2)
                .all(|w| near(w[0].x, w[0].z) <= near(w[1].x, w[1].z))
        );
        // The island is crossed by streets: nearly every item faces one.
        let facing_streets = level
            .furniture
            .iter()
            .filter(|f| f.kind == FurnitureKind::Lamp)
            .filter(|f| f.yaw != 0.0)
            .count();
        assert!(facing_streets * 10 >= count(FurnitureKind::Lamp) * 9);
    }
}
