//! Faces drawn twice in one place (#1436): z-fighting in what `room set`
//! writes.
//!
//! Two faces that lie in one plane and face one way are drawn at one depth,
//! and the renderer settles which of them is in front pixel by pixel and
//! frame by frame - a flicker a person sees at once as they move, and a
//! still picture barely shows. The agent builds from pictures, so it is told
//! instead: the owner found the first one live, on the front of a garage the
//! agent had written as JSON, where a header overlapped the panels beside a
//! door in one plane.
//!
//! Every primitive of a generator is meshed through the real mesher and
//! placed by its tree's composed transforms - the geometry the world draws,
//! taper, cut and hollow included. So is every terminal a shape grammar in
//! the tree derives (#1503): each Shape node's grammar is derived, and each
//! of its terminals meshed and placed, by the spawner's own code
//! (`world_builder::shape`), every terminal a piece of its own, placed by
//! its node's transform times its own as the spawner parents it. A Shape
//! node whose grammar does not parse or derive is spawned with nothing
//! below it, its children included, and so it is read: nothing of it or
//! under it is a piece. Every pair of pieces whose triangles share a plane
//! and a facing direction, with area in common, is named. Four kinds of
//! shared plane are left out, because nobody can see them: faces that face
//! each other, which are pressed between their two solids; a shared patch
//! buried inside a third piece, as where a lattice's braces cross inside
//! its leg; faces meeting wholly below the generator's anchor, which stands
//! on the ground; and, where a terminal is one of the pair, a strip of
//! shared face narrower than [`MIN_WIDTH_M`]. The shape mesher draws each
//! face a grammar splits off flat - a wall of `Comp(Faces)`, a roof's
//! slope - as a slab 1 mm thick, so where two meet at a corner, one's end
//! lies half a millimetre from the other's face along a strip that thin,
//! far too thin to see flicker; named, every corner of every such building
//! was. How wide a strip is, is judged over the whole stretch of face the
//! pair shares where it lies, not one triangle's worth at a time: a round
//! cap is drawn as a fan of slivers each narrower than that, and a cap
//! flush with a terminal's face is a disc that flickers - as is each of
//! two caps in one face, however far apart. What is left is what flickers.
//!
//! A pair is named by pointer - each piece's node, a primitive's own or the
//! Shape node a terminal was derived from - and a terminal also by which of
//! its node's terminals it is: `a_terminal` / `b_terminal`, holding its
//! `index` in derivation order, its `mesh` id (the string of its rule's
//! `I("...")`) and its `material` slot (the string of `Mat("...")`, or
//! null). A pair of primitives is answered as it was before terminals were
//! checked, by the same rules.
//!
//! A grammar may derive a hundred thousand terminals, so the check is
//! bounded. The pieces a box meets are found through a tree of their boxes
//! ([`BoxTree`]) rather than by comparing every piece with every other, and
//! the check stops once it has taken [`CHECK_TIME`]: it names what it found
//! by then, and lists each generator it had not finished by pointer
//! (`z_fighting_unchecked`), rather than hold `room set` past the time the
//! agent waits for an answer. It asks the clock between one step and the
//! next - a piece collected, a terminal placed, a pair compared, a triangle
//! of a pair's first piece, a patch joined to its stretch of face, a patch
//! probed - and no step but one runs long: a Shape node's grammar is
//! derived whole before its first terminal is placed (see [`CHECK_TIME`]).
//!
//! An absolute placement may carry a grammar seed of its own (#1505), and
//! then every Shape node of the tree it plants derives with that seed: the
//! same generator, drawn differently, which may fight where the generator
//! drawn with its own seeds does not. So each such placement whose seed is
//! new or changed - the placement did not draw its generator with that
//! seed before the set - or whose generator was changed is checked as it
//! draws its tree ([`Generator::with_shape_seed`]), once per generator and
//! seed, after the changed generators: a seed another placement drew
//! before the set is checked for the one given it now, since nothing says
//! that drawing was ever checked. Of what it draws, only the pairs the
//! generator does not draw itself are named: a pair of primitives, or of
//! terminals a grammar places alike whatever its seed, is drawn with the
//! generator's own seeds too, and named whenever the generator is set - it
//! is the generator's to answer for, not the seed's. A pair the
//! generator's own seeds bury inside a terminal is the seed's where the
//! seed moves that terminal away. Its pairs carry the placement's pointer
//! beside the generator's (`"placement": "/placements/12"`), and it is
//! listed by that pointer if the time runs out before it is finished, when
//! it names only the pairs it had found to be the seed's by then. A
//! placement whose seed draws exactly what its generator draws - a tree
//! with no Shape node, or the seed every Shape node in it has already - is
//! not checked again.
//!
//! An empty list can also mean a grammar drew nothing at all, which the
//! world says only in the World Editor (#829). So the answer also says what
//! each Shape node the check derived drew (`grammars`, #1507): its pointer
//! and how many terminals it derived, or why it drew nothing - the grammar
//! forge's message - those that drew nothing first, with the placement's
//! pointer where a placement's seed drew it. A node the check did not reach
//! before its time ran out, or one below a node that drew nothing, is not
//! derived and not answered.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::{Duration, Instant};

use bevy::math::Affine3A;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy_symbios_shape::cache::MeshCacheKey;
use serde_json::{Value, json};

use crate::catalogue::items::measure::{is_primitive, transform_of};
use crate::config::agent::WORLD_ANSWER_TIMEOUT;
use crate::pds::{Generator, GeneratorKind, Placement, RoomRecord};
use crate::world_builder::build_primitive_mesh;

/// Faces this close to one plane are drawn at one depth (m).
const PLANE_TOLERANCE_M: f32 = 1.0e-3;
/// Normals closer than this, as a cosine, face one way.
const SAME_WAY_COS: f32 = 0.9999;
/// A shared patch smaller than this is not named (m²): 10 cm², about a
/// 3 cm square.
const MIN_AREA_M2: f32 = 1.0e-3;
/// Where a terminal is one of a pair, a strip of shared face narrower than
/// this is not counted (m): twice the millimetre the shape mesher gives a
/// flat face (`scope_to_transform` draws a scope of no depth 1 mm deep),
/// whose end is the widest strip such a slab can share with its
/// neighbour's face. A patch is left out only where the stretch of face it
/// lies in is that narrow ([`Region::narrow`]): a patch is one triangle
/// clipped to another, and a round cap's fan cuts even a wide disc into
/// slivers narrower than this.
const MIN_WIDTH_M: f32 = 2.0e-3;
/// How far in front of a shared patch its visibility is probed (m).
const PROBE_M: f32 = 2.0e-3;
/// The direction a point-in-solid ray is cast: along no axis and in no
/// plane an authored face is likely to lie in, so it rarely grazes an edge.
const RAY: Vec3 = Vec3::new(0.296_8, 0.881_3, 0.367_7);

/// The longest the check may take (#1503). `room set` answers from the
/// daemon's frame, which runs nothing else meanwhile, and the agent gives
/// up on an answer after [`WORLD_ANSWER_TIMEOUT`] - by which time the set
/// has been written, so a check that outlasted it would leave the agent
/// told its set failed.
///
/// The clock is asked between steps, each short: a piece collected, a
/// terminal placed, a pair of pieces compared, a triangle of a pair's
/// first piece against the other's, a patch joined to the stretch of face
/// it lies in, a patch's burial probed. One step is not cut short: a Shape
/// node's grammar is derived whole, by the spawner's own derivation, before
/// its first terminal is placed and the clock asked again. symbios-shape
/// stops a derivation at 100 000 terminals, which bounds that step, but not
/// by this time.
const CHECK_TIME: Duration = Duration::from_secs(3);
// A third of that at most, leaving the rest to the set's other work.
const _: () = assert!(CHECK_TIME.as_millis() * 3 <= WORLD_ANSWER_TIMEOUT.as_millis());

/// Two pieces of one generator drawing faces in one place.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Overlap {
    /// Each piece's path of child indices from the generator's root: a
    /// primitive's own node, or the Shape node a terminal was derived from.
    pub(super) a: Vec<usize>,
    pub(super) b: Vec<usize>,
    /// Which of its node's terminals each piece is, when it is a shape
    /// grammar's rather than a primitive (#1503).
    pub(super) a_terminal: Option<TerminalName>,
    pub(super) b_terminal: Option<TerminalName>,
    /// The area they draw in one place where it can be seen (m²).
    pub(super) area_m2: f32,
}

/// A terminal of a shape grammar, named so that its rule can be found.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TerminalName {
    /// Its place in the derivation, from 0: the order the spawner meshes
    /// the node's terminals in.
    pub(super) index: usize,
    /// The mesh id its rule emitted, the string of `I("...")`.
    pub(super) mesh: String,
    /// The material slot stamped on it, the string of `Mat("...")`.
    pub(super) material: Option<String>,
}

impl TerminalName {
    fn json(&self) -> Value {
        json!({
            "index": self.index,
            "mesh": self.mesh,
            "material": self.material,
        })
    }
}

/// One triangle as the world draws it, facing out of its solid.
struct Tri {
    v: [Vec3; 3],
    normal: Vec3,
    min: Vec3,
    max: Vec3,
}

/// One primitive, or one terminal of a shape grammar, as the world draws
/// it.
struct Piece {
    path: Vec<usize>,
    /// Which terminal of the Shape node at `path` this is; `None` for a
    /// primitive.
    terminal: Option<TerminalName>,
    tris: Vec<Tri>,
    min: Vec3,
    max: Vec3,
    /// Whether it has an inside a point can be in. A terminal meshed as a
    /// flat panel - a hip roof's slope, a gable end - is two faces back to
    /// back with nothing between them, and has none.
    solid: bool,
}

impl Piece {
    /// Whether `point` is inside this piece's solid: a ray from it crosses
    /// the surface an odd number of times.
    fn contains(&self, point: Vec3) -> bool {
        self.solid
            && boxes_meet(point, point, self.min, self.max)
            && self
                .tris
                .iter()
                .filter(|tri| ray_crosses(point, RAY, tri))
                .count()
                % 2
                == 1
    }
}

/// A mesh as the check reads it, in its own space before it is placed.
struct Unit {
    corners: Vec<Vec3>,
    /// The corners each triangle takes, three by three.
    order: Vec<usize>,
    /// Whether it encloses a volume a point can be in.
    solid: bool,
}

impl Unit {
    /// `mesh`'s corners and triangles, `None` when it has no positions;
    /// taken for a solid when it is `closed`, or else when it encloses a
    /// volume.
    fn of(mesh: &Mesh, closed: bool) -> Option<Self> {
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            return None;
        };
        let corners: Vec<Vec3> = positions.iter().map(|p| Vec3::from_array(*p)).collect();
        let order: Vec<usize> = match mesh.indices() {
            Some(indices) => indices.iter().collect(),
            None => (0..corners.len()).collect(),
        };
        let solid = closed || encloses_volume(&corners, &order);
        Some(Self {
            corners,
            order,
            solid,
        })
    }

    /// This mesh placed by `world`: the primitive at `path`, or `terminal`
    /// of the Shape node there.
    fn place(
        &self,
        world: Affine3A,
        path: &[usize],
        terminal: Option<TerminalName>,
    ) -> Option<Piece> {
        let at: Vec<Vec3> = self
            .corners
            .iter()
            .map(|p| world.transform_point3(*p))
            .collect();
        // A mirroring transform turns every triangle's winding: turn it
        // back, so each winds counter-clockwise round the normal pointing
        // out of its solid - the normal and the clipping in `shared_patch`
        // both rest on it.
        let mirrored = world.matrix3.determinant() < 0.0;
        let tris: Vec<Tri> = self
            .order
            .as_chunks::<3>()
            .0
            .iter()
            .filter_map(|corners| {
                let mut v = [
                    at.get(corners[0])?,
                    at.get(corners[1])?,
                    at.get(corners[2])?,
                ]
                .map(|p| *p);
                if mirrored {
                    v.swap(1, 2);
                }
                let cross = (v[1] - v[0]).cross(v[2] - v[0]);
                let twice_area = cross.length();
                (twice_area > f32::EPSILON).then(|| Tri {
                    v,
                    normal: cross / twice_area,
                    min: v[0].min(v[1]).min(v[2]),
                    max: v[0].max(v[1]).max(v[2]),
                })
            })
            .collect();
        let min = tris.iter().fold(Vec3::INFINITY, |m, tri| m.min(tri.min));
        let max = tris
            .iter()
            .fold(Vec3::NEG_INFINITY, |m, tri| m.max(tri.max));
        (!tris.is_empty()).then(|| Piece {
            path: path.to_vec(),
            terminal,
            tris,
            min,
            max,
            solid: self.solid,
        })
    }
}

/// At most this many pairs are named in one answer, the most area first;
/// and at most this many grammars, those that drew nothing first.
const MAX_NAMED: usize = 16;

/// What `room set` answers about z-fighting, and about the grammars the
/// check derived on the way (#1507).
#[derive(Debug, Default, PartialEq)]
pub(super) struct Report {
    /// The pairs found, by JSON pointer into the record, the most area
    /// first: at most [`MAX_NAMED`].
    pub(super) named: Vec<Value>,
    /// How many pairs were found in all.
    pub(super) total: usize,
    /// Each generator the check ran out of time in before it had compared
    /// all of its pieces, by JSON pointer: what is named of it is some of
    /// its pairs, and perhaps not all. A generator as a placement's grammar
    /// seed draws it (#1505) is listed by the placement's pointer.
    pub(super) unchecked: Vec<String>,
    /// What each Shape node the check derived drew, by the node's pointer:
    /// how many terminals, or why nothing (`error`). Those that drew
    /// nothing first, then in the order they were derived: at most
    /// [`MAX_NAMED`].
    pub(super) grammars: Vec<Value>,
    /// How many Shape nodes were derived in all.
    pub(super) grammars_total: usize,
}

impl Report {
    /// Writes this into a set's `answer`: `z_fighting`, the pairs named;
    /// `z_fighting_total`, only when more were found than named;
    /// `z_fighting_unchecked`, only when the check ran out of time; and
    /// `grammars` and `grammars_total` by the same rule, only when the
    /// check derived a grammar.
    pub(super) fn answer(self, answer: &mut Value) {
        let named = self.named.len();
        answer["z_fighting"] = json!(self.named);
        if self.total > named {
            answer["z_fighting_total"] = json!(self.total);
        }
        if !self.unchecked.is_empty() {
            answer["z_fighting_unchecked"] = json!(self.unchecked);
        }
        let grammars = self.grammars.len();
        if grammars > 0 {
            answer["grammars"] = json!(self.grammars);
        }
        if self.grammars_total > grammars {
            answer["grammars_total"] = json!(self.grammars_total);
        }
    }
}

/// What `room set` answers about z-fighting: every generator the set left
/// different from what it was `before`, and every generator as an absolute
/// placement's grammar seed now draws it where that placement did not draw
/// it so before (#1505), checked for at most [`CHECK_TIME`].
pub(super) fn report(before: &RoomRecord, after: &RoomRecord) -> Report {
    report_within(before, after, Budget::until(Instant::now() + CHECK_TIME))
}

/// One tree the check reads: a changed generator as it draws itself, or a
/// generator as a placement's grammar seed draws it.
struct Subject<'a> {
    name: &'a str,
    tree: Cow<'a, Generator>,
    /// For a generator as a placement's grammar seed draws it: the
    /// placement, by index, and the generator as it draws itself, whose
    /// pairs are the generator's rather than the seed's. `None` for a
    /// generator as it draws itself.
    seeded: Option<(usize, &'a Generator)>,
}

/// Each absolute placement of `after` whose grammar seed (#1505) draws its
/// generator differently from how the generator draws itself, and whose
/// seed is new or changed or whose generator was changed (in `changed`):
/// every one but a placement that drew the same generator with the same
/// seed before the set, at the same index. A seed another placement drew
/// before is checked for this one all the same - nothing says that drawing
/// was ever checked: it may have been given in the World Editor, or listed
/// as unchecked when the time ran out. Once per generator and seed, the
/// first placement of each, in the order of the placements.
fn seeded_subjects<'a>(
    before: &RoomRecord,
    after: &'a RoomRecord,
    changed: &HashSet<&str>,
) -> Vec<Subject<'a>> {
    let mut taken: HashSet<(&str, u64)> = HashSet::new();
    let mut subjects = Vec::new();
    for (index, placement) in after.placements.iter().enumerate() {
        let Placement::Absolute {
            generator_ref,
            seed: Some(seed),
            ..
        } = placement
        else {
            continue;
        };
        let (name, seed) = (generator_ref.as_str(), *seed);
        let Some(generator) = after.generators.get(name) else {
            continue;
        };
        // This placement drew this very generator and seed before the set,
        // and the generator is as it was: what it draws is not new.
        let drew_it = matches!(
            before.placements.get(index),
            Some(Placement::Absolute {
                generator_ref: was,
                seed: Some(had),
                ..
            }) if was == name && *had == seed
        );
        if !changed.contains(name) && drew_it {
            continue;
        }
        if !taken.insert((name, seed)) {
            continue;
        }
        let tree = generator.with_shape_seed(seed);
        if tree == *generator {
            continue;
        }
        subjects.push(Subject {
            name,
            tree: Cow::Owned(tree),
            seeded: Some((index, generator)),
        });
    }
    subjects
}

/// [`report`], for as long as `budget` lasts: the changed generators are
/// checked in the order of their names, then the seeded placements' trees
/// in the order of the placements, and the one the budget runs out in, and
/// every one after it, are listed as unchecked.
fn report_within(before: &RoomRecord, after: &RoomRecord, mut budget: Budget) -> Report {
    let mut changed: Vec<(&String, &Generator)> = after
        .generators
        .iter()
        .filter(|(name, generator)| before.generators.get(*name) != Some(*generator))
        .collect();
    changed.sort_by(|a, b| a.0.cmp(b.0));
    let changed_names: HashSet<&str> = changed.iter().map(|(name, _)| name.as_str()).collect();
    let subjects = changed
        .iter()
        .map(|&(name, generator)| Subject {
            name,
            tree: Cow::Borrowed(generator),
            seeded: None,
        })
        .chain(seeded_subjects(before, after, &changed_names));
    let mut found: Vec<(f32, Value)> = Vec::new();
    let mut total = 0;
    let mut unchecked = Vec::new();
    // Each derived grammar's answer, and whether it drew nothing (#1507).
    let mut grammars: Vec<(bool, Value)> = Vec::new();
    for subject in subjects {
        let base = format!(
            "/generators/{}",
            subject.name.replace('~', "~0").replace('/', "~1")
        );
        let pointer = |path: &[usize]| {
            path.iter()
                .fold(base.clone(), |p, i| format!("{p}/children/{i}"))
        };
        let placement = subject
            .seeded
            .map(|(index, _)| format!("/placements/{index}"));
        let mut checked = check(&subject.tree, &mut budget);
        for grammar in &checked.grammars {
            let mut status = json!({ "node": pointer(&grammar.path) });
            match &grammar.drawn {
                Ok(terminals) => status["terminals"] = json!(terminals),
                Err(why) => status["error"] = json!(why),
            }
            if let Some(placement) = &placement {
                status["placement"] = json!(placement);
            }
            grammars.push((grammar.drawn.is_err(), status));
        }
        // Of what a seed draws, only what the generator does not draw
        // itself is the seed's to answer for (#1505).
        if let Some((_, own)) = subject.seeded {
            let decided = keep_what_only_the_seed_draws(&mut checked, own, &mut budget);
            checked.finished &= decided;
        }
        total += checked.pairs.len();
        // Only a generator's own largest can be among the largest of all,
        // so only they are named: a grammar can hold a million pairs.
        for k in 0..checked.pairs.len().min(MAX_NAMED) {
            let overlap = checked.overlap(k);
            let mut pair = json!({
                "a": pointer(&overlap.a),
                "b": pointer(&overlap.b),
                "area_m2": (f64::from(overlap.area_m2) * 10_000.0).round() / 10_000.0,
            });
            // A terminal is named beside its node's pointer (#1503); a
            // primitive has no such member, so a pair of primitives is
            // answered as it was before terminals were checked.
            if let Some(terminal) = &overlap.a_terminal {
                pair["a_terminal"] = terminal.json();
            }
            if let Some(terminal) = &overlap.b_terminal {
                pair["b_terminal"] = terminal.json();
            }
            // Drawn with a placement's grammar seed (#1505): which one.
            if let Some(placement) = &placement {
                pair["placement"] = json!(placement);
            }
            found.push((overlap.area_m2, pair));
        }
        if !checked.finished {
            unchecked.push(placement.unwrap_or(base));
        }
    }
    found.sort_by(|x, y| y.0.total_cmp(&x.0));
    // Stable: those that drew nothing first, each kind in the order it was
    // derived.
    grammars.sort_by_key(|(failed, _)| !failed);
    let grammars_total = grammars.len();
    Report {
        named: found.into_iter().take(MAX_NAMED).map(|(_, v)| v).collect(),
        total,
        unchecked,
        grammars: grammars
            .into_iter()
            .take(MAX_NAMED)
            .map(|(_, v)| v)
            .collect(),
        grammars_total,
    }
}

/// Keeps, of the pairs of `seeded` - a generator as a placement's grammar
/// seed draws it (#1505) - only those the generator as it draws itself,
/// `own`, does not draw: a pair whose two pieces `own` draws too, each in
/// the same place, with area in one plane where it can be seen there as
/// well, is the generator's, named whenever the generator is set, and not
/// the seed's - two primitives, or two terminals a grammar places alike
/// whatever its seed. A pair `own` draws buried, which a seed uncovers by
/// drawing a terminal elsewhere, is the seed's. Whether every pair was
/// decided before `budget` was spent: one left undecided is dropped, and
/// the placement is listed as unchecked.
fn keep_what_only_the_seed_draws(
    seeded: &mut Checked,
    own: &Generator,
    budget: &mut Budget,
) -> bool {
    if seeded.pairs.is_empty() {
        return true;
    }
    let mut pieces = Vec::new();
    // What its grammars draw is answered for the generator itself, where
    // the set changed it, not for the seed.
    collect(
        own,
        transform_of(&own.transform).compute_affine(),
        &mut Vec::new(),
        &mut pieces,
        &mut Vec::new(),
        budget,
    );
    // Spent while the pieces were being collected, some are missing.
    if budget.spent {
        seeded.pairs.clear();
        return false;
    }
    let tree = BoxTree::new(pieces.iter().map(|piece| (piece.min, piece.max)).collect());
    let mut by_place: HashMap<(&[usize], [u32; 6]), Vec<usize>> = HashMap::new();
    for (k, piece) in pieces.iter().enumerate() {
        by_place.entry(place_of(piece)).or_default().push(k);
    }
    // The pieces of `own` that are `piece`, drawn in the same place.
    let twins = |piece: &Piece| -> Vec<usize> {
        by_place
            .get(&place_of(piece))
            .into_iter()
            .flatten()
            .copied()
            .filter(|&k| same_piece(&pieces[k], piece))
            .collect()
    };
    let mut kept = Vec::new();
    let mut decided = true;
    for &(i, j, area) in &seeded.pairs {
        if budget.spent() {
            decided = false;
            break;
        }
        let (a, b) = (twins(&seeded.pieces[i]), twins(&seeded.pieces[j]));
        // Two pieces of `own`, one for each: a piece drawn twice over is
        // two pieces.
        let pair = a
            .iter()
            .find_map(|&x| b.iter().find(|&&y| y != x).map(|&y| (x, y)));
        let own_area = match pair {
            None => 0.0,
            Some((x, y)) => {
                match visible_shared_area(&pieces[x], &pieces[y], &pieces, [x, y], &tree, budget) {
                    Some(own_area) => own_area,
                    None => {
                        decided = false;
                        break;
                    }
                }
            }
        };
        if own_area < MIN_AREA_M2 {
            kept.push((i, j, area));
        }
    }
    seeded.pairs = kept;
    decided
}

/// Where `piece` is drawn, to find it among another tree's pieces: its
/// node's path and the bits of its box's corners.
fn place_of(piece: &Piece) -> (&[usize], [u32; 6]) {
    let [a, b, c] = piece.min.to_array().map(f32::to_bits);
    let [d, e, f] = piece.max.to_array().map(f32::to_bits);
    (&piece.path, [a, b, c, d, e, f])
}

/// Whether `a` and `b` are one piece drawn in one place: the same node's,
/// both primitives or both terminals, with the same triangles.
fn same_piece(a: &Piece, b: &Piece) -> bool {
    a.path == b.path
        && a.terminal.is_some() == b.terminal.is_some()
        && a.tris.len() == b.tris.len()
        && a.tris.iter().zip(&b.tris).all(|(x, y)| x.v == y.v)
}

/// The time the check may yet take: a clock it asks between one step and
/// the next, and once that says the time is spent, the check stops where
/// it is.
struct Budget {
    clock: Box<dyn FnMut() -> bool>,
    spent: bool,
}

impl Budget {
    /// A budget spent at `deadline`.
    fn until(deadline: Instant) -> Self {
        Self::new(move || Instant::now() >= deadline)
    }

    /// A budget spent when `clock` first says it is.
    fn new(clock: impl FnMut() -> bool + 'static) -> Self {
        Self {
            clock: Box::new(clock),
            spent: false,
        }
    }

    /// Whether the time is spent; once it is, it stays spent.
    fn spent(&mut self) -> bool {
        if !self.spent {
            self.spent = (self.clock)();
        }
        self.spent
    }
}

/// One generator, checked.
struct Checked {
    pieces: Vec<Piece>,
    /// Each pair of `pieces` that draws faces in one place where they can
    /// be seen, by index, with the area they draw there: the most area
    /// first.
    pairs: Vec<(usize, usize, f32)>,
    /// What each Shape node the check derived drew, in the order it was
    /// derived (#1507).
    grammars: Vec<Grammar>,
    /// Whether every pair was compared before the budget was spent.
    finished: bool,
}

/// What a Shape node's grammar drew when the check derived it (#1507).
struct Grammar {
    /// The node's path of child indices from the generator's root.
    path: Vec<usize>,
    /// How many terminals it derived, or why it drew nothing: the message
    /// the World Editor's grammar forge shows.
    drawn: Result<usize, String>,
}

impl Checked {
    /// The `k`th of `pairs`, named.
    fn overlap(&self, k: usize) -> Overlap {
        let (i, j, area_m2) = self.pairs[k];
        let (a, b) = (&self.pieces[i], &self.pieces[j]);
        Overlap {
            a: a.path.clone(),
            b: b.path.clone(),
            a_terminal: a.terminal.clone(),
            b_terminal: b.terminal.clone(),
            area_m2,
        }
    }
}

/// The pieces of `root`'s tree - primitives and the terminals of its shape
/// grammars - and the pairs of them that draw faces in one place where
/// they can be seen, compared for as long as `budget` lasts.
fn check(root: &Generator, budget: &mut Budget) -> Checked {
    let mut pieces = Vec::new();
    let mut grammars = Vec::new();
    collect(
        root,
        transform_of(&root.transform).compute_affine(),
        &mut Vec::new(),
        &mut pieces,
        &mut grammars,
        budget,
    );
    let tree = BoxTree::new(pieces.iter().map(|piece| (piece.min, piece.max)).collect());
    let mut pairs = Vec::new();
    let mut meeting = Vec::new();
    // Each pair once, in the order of the pieces, as every pair used to be
    // compared: of equal areas, the first found is named first.
    let finished = 'pairs: {
        // Spent while the pieces were being collected, some are missing.
        if budget.spent {
            break 'pairs false;
        }
        for i in 0..pieces.len() {
            meeting.clear();
            tree.any_meeting(pieces[i].min, pieces[i].max, |j| {
                if j > i {
                    meeting.push(j);
                }
                false
            });
            meeting.sort_unstable();
            for &j in &meeting {
                if budget.spent() {
                    break 'pairs false;
                }
                let Some(area) =
                    visible_shared_area(&pieces[i], &pieces[j], &pieces, [i, j], &tree, budget)
                else {
                    break 'pairs false;
                };
                if area >= MIN_AREA_M2 {
                    pairs.push((i, j, area));
                }
            }
        }
        true
    };
    pairs.sort_by(|x, y| y.2.total_cmp(&x.2));
    Checked {
        pieces,
        pairs,
        grammars,
        finished,
    }
}

/// Every primitive and every shape-grammar terminal under `node`, placed as
/// the world places it: each child by its parent's transform times its own,
/// as the spawner parents them, and each terminal by its node's, with what
/// each Shape node's grammar drew onto `grammars`. Nothing below a Shape
/// node whose grammar does not derive, which the world draws nothing of.
/// Stops where it is once `budget` is spent.
fn collect(
    node: &Generator,
    world: Affine3A,
    path: &mut Vec<usize>,
    out: &mut Vec<Piece>,
    grammars: &mut Vec<Grammar>,
    budget: &mut Budget,
) {
    if budget.spent() {
        return;
    }
    // A primitive is taken for the closed solid it is meshed as, as it
    // always was; a terminal is asked, since a flat panel encloses nothing.
    if is_primitive(&node.kind)
        && let Some(unit) = Unit::of(&build_primitive_mesh(&node.kind).mesh, true)
        && let Some(piece) = unit.place(world, path, None)
    {
        out.push(piece);
    }
    if !terminals(&node.kind, world, path, out, grammars, budget) {
        // The spawner hangs a node's children under the entity it spawns
        // for the node, and spawns none for a grammar that does not derive
        // (`spawn_node`): nothing below it is drawn.
        return;
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        let child_world = world * transform_of(&child.transform).compute_affine();
        collect(child, child_world, path, out, grammars, budget);
        path.pop();
    }
}

/// The terminals of `kind`, when it is a Shape node placed by `world`:
/// derived, meshed and placed by the spawner's own code
/// (`world_builder::shape`), each by its own transform under its node's, as
/// the spawner hangs each terminal under the node's entity, with what its
/// grammar drew pushed onto `grammars` (#1507). Whether the world spawns
/// the node, as far as its grammar decides: a grammar that does not parse
/// or derive draws nothing in the world, adds nothing here, and is false;
/// every other node is true.
fn terminals(
    kind: &GeneratorKind,
    world: Affine3A,
    path: &[usize],
    out: &mut Vec<Piece>,
    grammars: &mut Vec<Grammar>,
    budget: &mut Budget,
) -> bool {
    let Some(def) = kind.shape_def() else {
        return true;
    };
    let model = match def.derive() {
        Ok(model) => model,
        Err(underived) => {
            grammars.push(Grammar {
                path: path.to_vec(),
                drawn: Err(underived.message().to_owned()),
            });
            return false;
        }
    };
    grammars.push(Grammar {
        path: path.to_vec(),
        drawn: Ok(model.terminals.len()),
    });
    // Terminals of one key draw one mesh, which the spawner builds once
    // and shares among them; so is it read here.
    let mut units: HashMap<MeshCacheKey, Option<Unit>> = HashMap::new();
    for (index, terminal) in model.terminals.iter().enumerate() {
        if budget.spent() {
            return true;
        }
        let bake = def.bake(terminal);
        let Some(unit) = units
            .entry(bake.cache_key())
            .or_insert_with(|| Unit::of(&bake.mesh(), false))
        else {
            continue;
        };
        let name = TerminalName {
            index,
            mesh: terminal.mesh_id.clone(),
            material: terminal.material.as_ref().map(|m| m.id.clone()),
        };
        let placed = world * bake.transform.compute_affine();
        if let Some(piece) = unit.place(placed, path, Some(name)) {
            out.push(piece);
        }
    }
    true
}

/// Whether the mesh of `corners`, wound by `order`, encloses a volume,
/// read in the mesh's own space before it is placed. Six times the volume
/// a closed mesh encloses is the sum over its triangles of `a . (b x c)`; a
/// flat panel, drawn as two faces back to back in the plane z = 0 as the
/// shape mesher draws a hip roof's slope or a gable end, sums to exactly
/// nothing, while a unit box sums to six.
fn encloses_volume(corners: &[Vec3], order: &[usize]) -> bool {
    let six_volume: f32 = order
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|at| {
            let [a, b, c] = [
                corners.get(at[0])?,
                corners.get(at[1])?,
                corners.get(at[2])?,
            ];
            Some(a.dot(b.cross(*c)))
        })
        .sum();
    six_volume.abs() > 1.0e-6
}

/// The area `a` and `b` draw in one plane facing one way, less what lies
/// buried inside any other of `pieces` (`skip` names `a` and `b`, and
/// `tree` holds all their boxes) and, where either is a terminal, less
/// every strip of it narrower than [`MIN_WIDTH_M`]; `None` when `budget`
/// was spent before it was measured.
fn visible_shared_area(
    a: &Piece,
    b: &Piece,
    pieces: &[Piece],
    skip: [usize; 2],
    tree: &BoxTree,
    budget: &mut Budget,
) -> Option<f32> {
    let slack = Vec3::splat(PLANE_TOLERANCE_M);
    let (lo, hi) = (a.min.max(b.min) - slack, a.max.min(b.max) + slack);
    let near = |tri: &&Tri| boxes_meet(tri.min, tri.max, lo, hi);
    let b_near: Vec<&Tri> = b.tris.iter().filter(near).collect();
    // Every patch the two share above the ground, with the triangle of `a`
    // it lies on.
    let mut patches: Vec<(&Tri, Patch)> = Vec::new();
    for ta in a.tris.iter().filter(near) {
        // Two fine meshes whose boxes meet may share no plane at all, and
        // then nothing further on asks the clock: a pipe in its sleeve is a
        // hundred thousand triangles against as many (#1503).
        if budget.spent() {
            return None;
        }
        for tb in &b_near {
            if ta.normal.dot(tb.normal) < SAME_WAY_COS
                || !boxes_meet(ta.min - slack, ta.max + slack, tb.min, tb.max)
                || tb
                    .v
                    .iter()
                    .any(|p| ta.normal.dot(*p - ta.v[0]).abs() > PLANE_TOLERANCE_M)
            {
                continue;
            }
            let Some(patch) = shared_patch(ta, tb) else {
                continue;
            };
            // A generator stands on the ground at its anchor, its y = 0, so
            // two faces meeting wholly below that - a buried footing's
            // underside - are in the ground where nobody sees them.
            if ta.v.iter().chain(&tb.v).all(|p| p.y < 0.0) {
                continue;
            }
            patches.push((ta, patch));
        }
    }
    // A pair of primitives keeps the rules it had before terminals were
    // checked; a terminal's flat faces are slabs 1 mm thick.
    let terminal = a.terminal.is_some() || b.terminal.is_some();
    let thin = |patch: &Patch| terminal && patch.width < MIN_WIDTH_M;
    let (regions, region_of) = if patches.iter().any(|(_, patch)| thin(patch)) {
        Region::of(&patches, budget)?
    } else {
        (Vec::new(), Vec::new())
    };
    let mut area = 0.0;
    for (at, (ta, patch)) in patches.iter().enumerate() {
        // Where two of the slabs the shape mesher draws a grammar's flat
        // faces as meet at a corner, one's end lies within a millimetre of
        // the other's face, along a strip too thin to see. A thin patch is
        // left out only where the stretch of face it lies in is as thin: a
        // round cap's fan cuts a disc into slivers as thin as that.
        if thin(patch) && regions[region_of[at]].narrow() {
            continue;
        }
        if budget.spent() {
            return None;
        }
        let probe = patch.centre + ta.normal * PROBE_M;
        let buried = tree.any_meeting(probe, probe, |k| {
            !skip.contains(&k) && pieces[k].contains(probe)
        });
        if !buried {
            area += patch.area;
        }
    }
    Some(area)
}

/// A stretch of face a pair of pieces shares in one plane, facing one way
/// (#1503): patches that meet one another, the area of them in all, and the
/// box round them.
struct Region {
    area: f32,
    min: Vec3,
    max: Vec3,
}

impl Region {
    /// The regions `patches` lie in, and which region each patch lies in,
    /// by index; `None` where `budget` was spent before they were found.
    /// A patch lies in the plane of the first patch whose plane holds it,
    /// as [`visible_shared_area`] holds two triangles to one, and a region
    /// is the patches of one plane joined by meeting, box to box, within
    /// [`PLANE_TOLERANCE_M`]: a strip is one region however long it runs,
    /// and two round caps of one piece in one face are two however far
    /// apart they stand - judged as one, their box made them a strip.
    fn of(patches: &[(&Tri, Patch)], budget: &mut Budget) -> Option<(Vec<Region>, Vec<usize>)> {
        let mut planes: Vec<(Vec3, f32)> = Vec::new();
        let mut in_plane: Vec<Vec<usize>> = Vec::new();
        for (at, (ta, _)) in patches.iter().enumerate() {
            let offset = ta.normal.dot(ta.v[0]);
            let plane = match planes.iter().position(|(normal, from_origin)| {
                ta.normal.dot(*normal) >= SAME_WAY_COS
                    && (offset - from_origin).abs() <= PLANE_TOLERANCE_M
            }) {
                Some(plane) => plane,
                None => {
                    planes.push((ta.normal, offset));
                    in_plane.push(Vec::new());
                    planes.len() - 1
                }
            };
            in_plane[plane].push(at);
        }
        // The patches of each plane that meet are joined: swept along the
        // axis they spread furthest along, each against the ones before it
        // whose box still reaches it.
        let slack = Vec3::splat(PLANE_TOLERANCE_M);
        let mut joined: Vec<usize> = (0..patches.len()).collect();
        for members in &mut in_plane {
            let (lo, hi) = members
                .iter()
                .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(lo, hi), &at| {
                    (lo.min(patches[at].1.min), hi.max(patches[at].1.max))
                });
            let axis = (hi - lo).max_position();
            members.sort_by(|&i, &j| patches[i].1.min[axis].total_cmp(&patches[j].1.min[axis]));
            let mut reaching: Vec<usize> = Vec::new();
            for &at in members.iter() {
                if budget.spent() {
                    return None;
                }
                let patch = &patches[at].1;
                reaching.retain(|&before| {
                    patches[before].1.max[axis] + PLANE_TOLERANCE_M >= patch.min[axis]
                });
                for &before in &reaching {
                    let other = &patches[before].1;
                    if boxes_meet(other.min - slack, other.max + slack, patch.min, patch.max) {
                        let (i, j) = (
                            first_joined(&mut joined, before),
                            first_joined(&mut joined, at),
                        );
                        joined[i.max(j)] = i.min(j);
                    }
                }
                reaching.push(at);
            }
        }
        let mut regions: Vec<Region> = Vec::new();
        let mut region_at: Vec<Option<usize>> = vec![None; patches.len()];
        let mut region_of = Vec::with_capacity(patches.len());
        for (at, (_, patch)) in patches.iter().enumerate() {
            let first = first_joined(&mut joined, at);
            let region = *region_at[first].get_or_insert_with(|| {
                regions.push(Region {
                    area: 0.0,
                    min: Vec3::INFINITY,
                    max: Vec3::NEG_INFINITY,
                });
                regions.len() - 1
            });
            let into = &mut regions[region];
            into.area += patch.area;
            into.min = into.min.min(patch.min);
            into.max = into.max.max(patch.max);
            region_of.push(region);
        }
        Some((regions, region_of))
    }

    /// Whether it is a strip narrower than [`MIN_WIDTH_M`]: its area over
    /// the diagonal of its box, which is a strip's width - the box of a
    /// strip is as long as the strip - and a little more than a disc's
    /// radius.
    fn narrow(&self) -> bool {
        self.area < MIN_WIDTH_M * (self.max - self.min).length()
    }
}

/// The first patch of those `at` is joined to, by `joined`, which holds for
/// each patch one it is joined to that comes before it, or itself.
fn first_joined(joined: &mut [usize], mut at: usize) -> usize {
    while joined[at] != at {
        joined[at] = joined[joined[at]];
        at = joined[at];
    }
    at
}

/// What two coplanar triangles facing one way have in common.
struct Patch {
    area: f32,
    centre: Vec3,
    /// How wide it is at its narrowest (m).
    width: f32,
    /// The least and greatest corners of the box round it.
    min: Vec3,
    max: Vec3,
}

/// The patch two coplanar triangles facing one way have in common: `ta`
/// clipped to `tb` in `ta`'s plane. Both wind the same way round their
/// shared normal, so each is counter-clockwise in a frame whose second
/// axis is the normal crossed with the first.
fn shared_patch(ta: &Tri, tb: &Tri) -> Option<Patch> {
    let u = (ta.v[1] - ta.v[0]).normalize();
    let w = ta.normal.cross(u);
    let flat = |p: Vec3| {
        let d = p - ta.v[0];
        Vec2::new(d.dot(u), d.dot(w))
    };
    let mut patch: Vec<Vec2> = ta.v.iter().map(|p| flat(*p)).collect();
    let edge = [flat(tb.v[0]), flat(tb.v[1]), flat(tb.v[2])];
    for k in 0..3 {
        patch = keep_left_of(&patch, edge[k], edge[(k + 1) % 3]);
        if patch.len() < 3 {
            return None;
        }
    }
    let (area, centre) = area_and_centre(&patch);
    let lift = |q: Vec2| ta.v[0] + u * q.x + w * q.y;
    let (min, max) = patch
        .iter()
        .map(|q| lift(*q))
        .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p), hi.max(p))
        });
    (area > 0.0).then(|| Patch {
        area,
        centre: lift(centre),
        width: narrowest(&patch),
        min,
        max,
    })
}

/// The part of the convex polygon `poly` on the left of the line from `a`
/// to `b` (Sutherland-Hodgman, one edge).
fn keep_left_of(poly: &[Vec2], a: Vec2, b: Vec2) -> Vec<Vec2> {
    let side = |p: Vec2| (b - a).perp_dot(p - a);
    let mut kept = Vec::with_capacity(poly.len() + 1);
    for (i, &p) in poly.iter().enumerate() {
        let q = poly[(i + 1) % poly.len()];
        let (sp, sq) = (side(p), side(q));
        if sp >= 0.0 {
            kept.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            kept.push(p + (q - p) * (sp / (sp - sq)));
        }
    }
    kept
}

/// A polygon's area and centroid (the shoelace formula).
fn area_and_centre(poly: &[Vec2]) -> (f32, Vec2) {
    let mut twice = 0.0;
    let mut weighted = Vec2::ZERO;
    for (i, &p) in poly.iter().enumerate() {
        let q = poly[(i + 1) % poly.len()];
        let cross = p.perp_dot(q);
        twice += cross;
        weighted += (p + q) * cross;
    }
    if twice.abs() <= f32::EPSILON {
        return (0.0, Vec2::ZERO);
    }
    (twice.abs() / 2.0, weighted / (3.0 * twice))
}

/// How wide the convex polygon `poly` is at its narrowest: the least
/// distance between two parallel lines that hold it between them, one of
/// which runs along one of its sides.
fn narrowest(poly: &[Vec2]) -> f32 {
    let mut least = f32::INFINITY;
    for (i, &p) in poly.iter().enumerate() {
        let side = poly[(i + 1) % poly.len()] - p;
        let length = side.length();
        if length <= f32::EPSILON {
            continue;
        }
        let across = poly
            .iter()
            .map(|q| side.perp_dot(*q - p).abs() / length)
            .fold(0.0, f32::max);
        least = least.min(across);
    }
    least
}

/// Whether the ray from `origin` along `dir` crosses `tri` (Moller-Trumbore).
fn ray_crosses(origin: Vec3, dir: Vec3, tri: &Tri) -> bool {
    let e1 = tri.v[1] - tri.v[0];
    let e2 = tri.v[2] - tri.v[0];
    let h = dir.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1.0e-12 {
        return false;
    }
    let s = origin - tri.v[0];
    let u = s.dot(h) / det;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) / det;
    v >= 0.0 && u + v <= 1.0 && e2.dot(q) / det > 0.0
}

fn boxes_meet(a_min: Vec3, a_max: Vec3, b_min: Vec3, b_max: Vec3) -> bool {
    a_min.cmple(b_max).all() && b_min.cmple(a_max).all()
}

/// A node of the tree holding at most this many boxes is a leaf.
const LEAF_BOXES: usize = 8;

/// Boxes halved and halved again, so that the few meeting one box are
/// found without comparing it with every other (#1503). Each node holds
/// the box around all of its own: a node whose box misses the one asked
/// about holds none that meets it, and is passed over whole.
struct BoxTree {
    /// Each box's least and greatest corner.
    boxes: Vec<(Vec3, Vec3)>,
    /// The root first; each node holds two halves or, as a leaf, a run of
    /// `items`.
    nodes: Vec<BoxNode>,
    /// Box indices, ordered so that each leaf's lie in one run.
    items: Vec<usize>,
    /// How many boxes the tree has compared with one asked about: for a
    /// test to see how many it passed over.
    #[cfg(test)]
    compared: std::cell::Cell<usize>,
}

struct BoxNode {
    min: Vec3,
    max: Vec3,
    holds: Holds,
}

enum Holds {
    /// Two nodes, by index.
    Halves(usize, usize),
    /// A leaf's run of `items`.
    Run(Range<usize>),
}

impl BoxTree {
    fn new(boxes: Vec<(Vec3, Vec3)>) -> Self {
        let mut tree = Self {
            items: (0..boxes.len()).collect(),
            boxes,
            nodes: Vec::new(),
            #[cfg(test)]
            compared: std::cell::Cell::new(0),
        };
        if !tree.items.is_empty() {
            tree.halve(0..tree.items.len());
        }
        tree
    }

    /// Adds the node holding the boxes of `items[run]` and returns its
    /// index: while it holds more than [`LEAF_BOXES`], halved about the
    /// middle one of their centres along the axis they spread widest on.
    fn halve(&mut self, run: Range<usize>) -> usize {
        let (mut min, mut max) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
        let (mut low, mut high) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
        for &i in &self.items[run.clone()] {
            let (lo, hi) = self.boxes[i];
            min = min.min(lo);
            max = max.max(hi);
            // Twice each centre, which orders them as the centres do.
            low = low.min(lo + hi);
            high = high.max(lo + hi);
        }
        let at = self.nodes.len();
        self.nodes.push(BoxNode {
            min,
            max,
            holds: Holds::Run(run.clone()),
        });
        if run.len() > LEAF_BOXES {
            let axis = (high - low).max_position();
            let half = run.len() / 2;
            let boxes = &self.boxes;
            self.items[run.clone()].select_nth_unstable_by(half, |&x, &y| {
                let centre = |i: usize| boxes[i].0[axis] + boxes[i].1[axis];
                centre(x).total_cmp(&centre(y))
            });
            let lower = self.halve(run.start..run.start + half);
            let upper = self.halve(run.start + half..run.end);
            self.nodes[at].holds = Holds::Halves(lower, upper);
        }
        at
    }

    /// Calls `each` with the index of every box that meets `min..max`, as
    /// [`boxes_meet`] decides, until it returns true; whether one did.
    fn any_meeting(&self, min: Vec3, max: Vec3, mut each: impl FnMut(usize) -> bool) -> bool {
        let mut todo = Vec::new();
        if !self.nodes.is_empty() {
            todo.push(0);
        }
        while let Some(at) = todo.pop() {
            let node = &self.nodes[at];
            if !boxes_meet(node.min, node.max, min, max) {
                continue;
            }
            match &node.holds {
                Holds::Halves(lower, upper) => todo.extend([*lower, *upper]),
                Holds::Run(run) => {
                    for &i in &self.items[run.clone()] {
                        #[cfg(test)]
                        self.compared.set(self.compared.get() + 1);
                        let (lo, hi) = self.boxes[i];
                        if boxes_meet(lo, hi, min, max) && each(i) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

/// A house in wire form (#1505) whose grammar tosses a coin by its seed:
/// its 2 m panel stands beside its 2 m post, or is pushed half a metre back
/// into it, where their fronts, backs, tops and bottoms fight - 1 m² each,
/// 4 m² in all. `seed` is the Shape node's own.
#[cfg(test)]
pub(super) fn coin_house(seed: u64) -> Value {
    let grammar = [
        "Lot --> Extrude(2) Split(X) { ~1: Left | ~1: Right }",
        "Left --> I(\"Post\")",
        "Right --> 50% Beside | 50% Pushed",
        "Beside --> I(\"Panel\")",
        "Pushed --> Translate(-0.5, 0, 0) I(\"Panel\")",
    ]
    .join("\n");
    json!({
        "$type": "network.symbios.gen.shape",
        "grammar_source": grammar,
        "root_rule": "Lot",
        "footprint": [40_000, 0, 20_000],
        "seed": seed.to_string(),
        "transform": { "translation": [0, 10_000, 0] },
    })
}

/// Two seeds for [`coin_house`]: one that leaves the panel beside the post,
/// and one that pushes it in - found by checking it, so that a change to
/// how the grammar engine tosses its coins moves the seeds, not the tests.
#[cfg(test)]
pub(super) fn coin_seeds() -> (u64, u64) {
    let fights = |seed: u64| {
        let house: Generator = serde_json::from_value(coin_house(seed)).expect("a generator");
        !check(&house, &mut Budget::new(|| false)).pairs.is_empty()
    };
    let beside = (1..64)
        .find(|&seed| !fights(seed))
        .expect("a seed that leaves it beside");
    let pushed = (1..64)
        .find(|&seed| fights(seed))
        .expect("a seed that pushes it in");
    (beside, pushed)
}

/// Every pair of pieces of `root` that draws faces in one place where they
/// can be seen, the most area first, each as one line naming both pieces
/// and the area they share - the check `room set` runs, with no clock. For
/// the catalogue's overhaul guard and census (#1575,
/// `catalogue::items::overhaul`), where nobody waits on an answer and a
/// grammar building may take longer than [`CHECK_TIME`] to compare whole.
#[cfg(test)]
pub(crate) fn coplanar_overlap_lines(root: &Generator) -> Vec<String> {
    let mut budget = Budget::new(|| false);
    let checked = check(root, &mut budget);
    (0..checked.pairs.len())
        .map(|k| {
            let pair = checked.overlap(k);
            format!(
                "{:.2} m2: {} and {}",
                pair.area_m2,
                piece_line(root, &pair.a, pair.a_terminal.as_ref()),
                piece_line(root, &pair.b, pair.b_terminal.as_ref())
            )
        })
        .collect()
}

/// A piece named for a person reading a test failure: its node's path of
/// child indices and kind, and which terminal of it, if a grammar's.
#[cfg(test)]
fn piece_line(root: &Generator, path: &[usize], terminal: Option<&TerminalName>) -> String {
    let node = path
        .iter()
        .try_fold(root, |node, &i| node.children.get(i))
        .map_or("?", |node| node.kind_tag());
    let mut line = format!("{node} at children{path:?}");
    if let Some(terminal) = terminal {
        line.push_str(&format!(
            " terminal {} (I(\"{}\"){})",
            terminal.index,
            terminal.mesh,
            terminal
                .material
                .as_deref()
                .map(|m| format!(", Mat(\"{m}\")"))
                .unwrap_or_default()
        ));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::Fp;

    /// A cuboid prim in wire form: `size` and `at` in metres.
    fn cuboid(size: [f32; 3], at: [f32; 3], children: Vec<Value>) -> Value {
        let wire = |v: [f32; 3]| v.map(|x| (x * 10_000.0).round() as i64);
        json!({
            "$type": "network.symbios.gen.cuboid",
            "size": wire(size),
            "solid": true,
            "material": {},
            "transform": { "translation": wire(at) },
            "children": children,
        })
    }

    fn generator(wire: Value) -> Generator {
        serde_json::from_value(wire).expect("a generator")
    }

    /// A world holding `generators` and `placements`, and nothing else the
    /// check reads.
    fn world(generators: HashMap<String, Generator>, placements: Vec<Placement>) -> RoomRecord {
        RoomRecord {
            lex_type: "network.symbios.room".to_owned(),
            environment: Default::default(),
            generators,
            placements,
            traits: HashMap::new(),
            contact_effects: Default::default(),
            default_landing: None,
            opaque_refs: Default::default(),
        }
    }

    /// A world holding nothing.
    fn empty() -> RoomRecord {
        world(HashMap::new(), Vec::new())
    }

    /// Every pair the check finds in the generator `wire`, given all the
    /// time it wants.
    fn overlaps(wire: Value) -> Vec<Overlap> {
        let checked = check(&generator(wire), &mut Budget::new(|| false));
        assert!(
            checked.finished,
            "the check ran out of time with none to run out of"
        );
        (0..checked.pairs.len())
            .map(|k| checked.overlap(k))
            .collect()
    }

    /// `node`'s pieces, as the check collects them given all the time it
    /// wants.
    fn pieces_of(node: &Generator) -> Vec<Piece> {
        pieces_within(node, &mut Budget::new(|| false))
    }

    fn pieces_within(node: &Generator, budget: &mut Budget) -> Vec<Piece> {
        let mut pieces = Vec::new();
        collect(
            node,
            transform_of(&node.transform).compute_affine(),
            &mut Vec::new(),
            &mut pieces,
            &mut Vec::new(),
            budget,
        );
        pieces
    }

    /// The live case (#1436): a panel whose top overlaps a header in one
    /// plane - both the fronts and the backs are drawn twice, each over the
    /// strip they share.
    #[test]
    fn faces_sharing_a_plane_and_a_direction_are_named_with_their_area() {
        // A 2 m x 1 m header, and under it a 0.6 m panel whose top 0.5 m
        // runs up into the header's own plane (its sides clear of the
        // header's, which would share a plane of their own).
        let found = overlaps(cuboid(
            [2.0, 1.0, 0.1],
            [0.0, 3.0, 0.0],
            vec![cuboid([0.6, 2.0, 0.1], [0.5, -1.0, 0.0], vec![])],
        ));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            (found[0].a.as_slice(), found[0].b.as_slice()),
            (&[][..], &[0][..])
        );
        // Front and back, each 0.6 m x 0.5 m.
        assert!((found[0].area_m2 - 0.6).abs() < 1e-3, "{found:?}");
    }

    /// Two millimetres apart is two depths, not one - square to the axes,
    /// and turned 30 degrees, where each face's box spans the other's and
    /// only its distance from the plane tells them apart.
    #[test]
    fn faces_a_hair_apart_are_not_named() {
        for turned in [false, true] {
            let mut root = cuboid(
                [2.0, 1.0, 0.1],
                [0.0, 3.0, 0.0],
                vec![cuboid([0.6, 2.0, 0.1], [0.5, -1.0, 0.002], vec![])],
            );
            if turned {
                root["transform"]["rotation"] = json!([0, 2_588, 0, 9_659]);
            }
            let found = overlaps(root);
            assert!(found.is_empty(), "turned {turned}: {found:?}");
        }
    }

    /// Side by side, the tops share a plane and a direction but no area.
    #[test]
    fn faces_meeting_edge_to_edge_are_not_named() {
        let found = overlaps(cuboid(
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
            vec![cuboid([1.0, 1.0, 1.0], [1.0, 0.0, 0.0], vec![])],
        ));
        assert!(found.is_empty(), "{found:?}");
    }

    /// Stacked, one's top and the other's bottom share a plane but face
    /// each other: pressed between the two solids, never drawn to an eye.
    #[test]
    fn faces_facing_each_other_are_not_named() {
        let found = overlaps(cuboid(
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
            vec![cuboid([1.0, 1.0, 1.0], [0.0, 1.0, 0.0], vec![])],
        ));
        assert!(found.is_empty(), "{found:?}");
    }

    /// Two bars crossing at one height share their tops where they cross;
    /// a post enclosing the crossing buries it, as a lattice's leg does.
    #[test]
    fn a_patch_buried_inside_a_third_solid_is_not_named() {
        let bars = || {
            vec![
                cuboid([2.0, 0.1, 0.1], [0.0, 1.0, 0.0], vec![]),
                cuboid([0.1, 0.1, 2.0], [0.0, 1.0, 0.0], vec![]),
            ]
        };
        let open = overlaps(cuboid([0.2, 0.2, 0.2], [5.0, 0.0, 0.0], bars()));
        assert!(!open.is_empty(), "the crossing shows without the post");

        let buried = overlaps(cuboid([0.4, 3.0, 0.4], [0.0, 1.5, 0.0], {
            let mut children = bars();
            for child in &mut children {
                // Relative to the post's centre at 1.5 m.
                child["transform"]["translation"][1] = json!(-5_000);
            }
            children
        }));
        assert!(buried.is_empty(), "{buried:?}");
    }

    /// Two footings side by side, sharing their undersides a metre down,
    /// meet in the ground; raised to stand on it, they are seen.
    #[test]
    fn faces_meeting_below_the_ground_are_not_named() {
        let pair = |y: f32| {
            cuboid(
                [1.0, 1.0, 1.0],
                [0.0, y, 0.0],
                vec![cuboid([1.0, 1.0, 1.0], [0.5, 0.0, 0.2], vec![])],
            )
        };
        let buried = overlaps(pair(-1.0));
        assert!(buried.is_empty(), "{buried:?}");
        let standing = overlaps(pair(1.0));
        assert_eq!(standing.len(), 1, "{standing:?}");
    }

    /// A patch under 10 cm² is not named: a 2 cm cube's top in a box's top
    /// face is 4 cm², a 5 cm cube's is 25 cm².
    #[test]
    fn a_patch_smaller_than_ten_square_centimetres_is_not_named() {
        let cube_in_the_top = |edge: f32| {
            overlaps(cuboid(
                [1.0, 1.0, 1.0],
                [0.0, 0.5, 0.0],
                vec![cuboid([edge; 3], [0.0, 0.5 - edge / 2.0, 0.0], vec![])],
            ))
        };
        let small = cube_in_the_top(0.02);
        assert!(small.is_empty(), "{small:?}");
        let named = cube_in_the_top(0.05);
        assert_eq!(named.len(), 1, "{named:?}");
        assert!((named[0].area_m2 - 0.0025).abs() < 1e-5, "{named:?}");
    }

    /// A mirrored child's faces face out of it as it is drawn: mirrored in
    /// X under a box, its far end lands in the box's +X face, facing +X -
    /// the winding alone would point that face back into the child.
    #[test]
    fn a_mirrored_child_faces_the_way_it_is_drawn() {
        let mut child = cuboid([1.0, 1.0, 1.0], [0.5, 0.0, 0.0], vec![]);
        child["transform"]["scale"] = json!([-10_000, 10_000, 10_000]);
        let found = overlaps(cuboid([2.0, 2.0, 2.0], [0.0, 1.0, 0.0], vec![child]));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!((found[0].area_m2 - 1.0).abs() < 1e-3, "{found:?}");
    }

    /// The answer names the sixteen largest pairs by pointer - a generator's
    /// name escaped as a JSON pointer escapes it - and counts them all; a
    /// generator the set left as it was is not checked at all.
    #[test]
    fn the_report_names_the_largest_sixteen_and_counts_them_all() {
        // Seventeen small panels, each sharing its front and back with a
        // long slab and touching nothing else.
        let panels = (0..17)
            .map(|i| cuboid([0.4, 0.4, 0.1], [i as f32 - 8.0, 0.0, 0.0], vec![]))
            .collect();
        let slab = generator(cuboid([20.0, 1.0, 0.1], [0.0, 1.0, 0.0], panels));
        let after = world(HashMap::from([("a/b~c".to_owned(), slab)]), Vec::new());

        let Report { named, total, .. } = report(&empty(), &after);

        assert_eq!((named.len(), total), (16, 17));
        assert_eq!(named[0]["a"], "/generators/a~1b~0c");
        assert!(
            named[0]["b"]
                .as_str()
                .is_some_and(|b| b.starts_with("/generators/a~1b~0c/children/")),
            "{}",
            named[0]
        );
        assert_eq!(report(&after, &after), Report::default(), "nothing changed");
    }

    /// A child is placed by its parent's rotation: under a root turned a
    /// quarter about Y, a child authored against the root's +X end lands on
    /// its -Z end in the world, and the two share that face.
    #[test]
    fn a_child_is_placed_by_its_parents_rotation() {
        let mut root = cuboid(
            [4.0, 2.0, 2.0],
            [0.0, 0.0, 0.0],
            vec![cuboid([1.0, 1.0, 1.0], [1.5, 0.0, 0.0], vec![])],
        );
        root["transform"]["rotation"] = json!([0, 7071, 0, 7071]);
        let found = overlaps(root);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!((found[0].area_m2 - 1.0).abs() < 1e-3, "{found:?}");
    }

    /// What is drawn is the mesher's: a cylinder's cap in a box's top face
    /// fights over the cap's whole disc.
    #[test]
    fn a_rounded_primitive_is_read_from_its_mesh() {
        // The box spans 0..1 m up; the cylinder 0.4..1 m, its top cap in
        // the box's top face and the rest inside the box.
        let mut root = cuboid([2.0, 1.0, 2.0], [0.0, 0.5, 0.0], vec![]);
        root["children"] = json!([{
            "$type": "network.symbios.gen.cylinder",
            "radius": 4_000,
            "height": 6_000,
            "resolution": 32,
            "solid": true,
            "material": {},
            "transform": { "translation": [0, 2_000, 0] },
        }]);
        let found = overlaps(root);
        assert_eq!(found.len(), 1, "{found:?}");
        let disc = std::f32::consts::PI * 0.4 * 0.4;
        assert!(
            (found[0].area_m2 - disc).abs() < 0.02,
            "{} against a disc of {disc}",
            found[0].area_m2
        );
    }

    /// A shape-grammar node in wire form (#1503): `grammar` one statement a
    /// line, derived from `Lot`, its footprint and place in metres.
    fn shape(grammar: &[&str], footprint: [f32; 3], at: [f32; 3]) -> Value {
        let wire = |v: [f32; 3]| v.map(|x| (x * 10_000.0).round() as i64);
        json!({
            "$type": "network.symbios.gen.shape",
            "grammar_source": grammar.join("\n"),
            "root_rule": "Lot",
            "footprint": wire(footprint),
            "seed": "1",
            "transform": { "translation": wire(at) },
        })
    }

    /// The answer for one generator named `name`, set where there was none.
    fn answer(name: &str, wire: Value) -> (Vec<Value>, usize) {
        let Report {
            named,
            total,
            unchecked,
            ..
        } = report_within(
            &empty(),
            &world(
                HashMap::from([(name.to_owned(), generator(wire))]),
                Vec::new(),
            ),
            Budget::new(|| false),
        );
        assert!(unchecked.is_empty(), "{unchecked:?}");
        (named, total)
    }

    /// Two terminals of one grammar drawing faces in one place are named
    /// as primitives are, each by its Shape node's pointer and by which
    /// terminal it is: its place in the derivation, its mesh id and its
    /// material slot. A post, and a panel that runs half a metre back into
    /// it, share the front plane where they overlap (1 m²) and the top and
    /// bottom planes (0.5 m² each).
    #[test]
    fn terminals_of_one_grammar_in_one_plane_are_named_with_their_rules() {
        let (named, total) = answer(
            "house",
            shape(
                &[
                    "Lot --> Extrude(2) Split(X) { ~1: Left | ~1: Right }",
                    "Left --> Mat(\"Oak\") I(\"Post\")",
                    "Right --> Translate(-0.5, 0, 0) Size(scope.x + 0.5, scope.y, scope.z * 0.5) \
                     Mat(\"Stone\") I(\"Panel\")",
                ],
                [4.0, 0.0, 2.0],
                [0.0, 1.0, 0.0],
            ),
        );
        assert_eq!(total, 1, "{named:?}");
        assert_eq!(
            named[0],
            json!({
                "a": "/generators/house",
                "a_terminal": { "index": 0, "mesh": "Post", "material": "Oak" },
                "b": "/generators/house",
                "b_terminal": { "index": 1, "mesh": "Panel", "material": "Stone" },
                "area_m2": 2.0,
            })
        );
    }

    /// Split side by side, two terminals share their fronts' plane but no
    /// area, and where they meet, one's end and the other's face each
    /// other, pressed between the two: nothing to name. Pushed half a
    /// metre into each other, the same two are named.
    #[test]
    fn terminals_meeting_edge_to_edge_or_face_to_face_are_not_named() {
        let pair = |right: &str| {
            overlaps(shape(
                &[
                    "Lot --> Extrude(2) Split(X) { ~1: Left | ~1: Right }",
                    "Left --> I(\"Post\")",
                    right,
                ],
                [4.0, 0.0, 2.0],
                [0.0, 1.0, 0.0],
            ))
        };
        let meeting = pair("Right --> I(\"Panel\")");
        assert!(meeting.is_empty(), "{meeting:?}");
        let pushed_in = pair("Right --> Translate(-0.5, 0, 0) I(\"Panel\")");
        assert_eq!(pushed_in.len(), 1, "{pushed_in:?}");
    }

    /// A terminal whose top lies in a primitive's top face is named against
    /// it: the primitive by its pointer alone, the terminal by its node's
    /// pointer and which terminal it is - here the grammar's only one,
    /// with no material slot.
    #[test]
    fn a_terminal_in_one_plane_with_a_primitive_is_named() {
        let mut root = cuboid([2.0, 2.0, 2.0], [0.0, 1.0, 0.0], vec![]);
        root["children"] = json!([shape(
            &["Lot --> Extrude(1) I(\"Post\")"],
            [1.0, 0.0, 1.0],
            [-0.5, 0.0, -0.5],
        )]);
        let (named, total) = answer("g", root);
        assert_eq!(total, 1, "{named:?}");
        assert_eq!(
            named[0],
            json!({
                "a": "/generators/g",
                "b": "/generators/g/children/0",
                "b_terminal": { "index": 0, "mesh": "Post", "material": null },
                "area_m2": 1.0,
            })
        );
    }

    /// A pair of primitives is answered in the same bytes as before
    /// terminals were checked: its pointers and its area, and nothing else.
    #[test]
    fn a_pair_of_primitives_is_answered_as_before() {
        let (named, _) = answer(
            "g",
            cuboid(
                [2.0, 1.0, 0.1],
                [0.0, 3.0, 0.0],
                vec![cuboid([0.6, 2.0, 0.1], [0.5, -1.0, 0.0], vec![])],
            ),
        );
        assert_eq!(
            serde_json::to_string(&named).expect("json"),
            r#"[{"a":"/generators/g","area_m2":0.6,"b":"/generators/g/children/0"}]"#
        );
    }

    /// A Shape node is placed by every transform above it, and each of its
    /// terminals by the node's transform times its own. Under a parent
    /// turned 30 degrees and moved, a block set against the parent's +X end
    /// lands in that end's turned plane. Placed by its node's own transform
    /// alone, square to the axes, it would land in a decoy box set there
    /// instead, and turned the other way round it would land nowhere near.
    #[test]
    fn a_terminal_is_placed_by_its_nodes_composed_transform() {
        let mut parent = cuboid(
            [4.0, 2.0, 2.0],
            [3.0, 1.0, -2.0],
            vec![shape(
                &["Lot --> Extrude(1) I(\"Block\")"],
                [1.0, 0.0, 1.0],
                [1.0, -0.5, -0.5],
            )],
        );
        parent["transform"]["rotation"] = json!([0, 2_588, 0, 9_659]);
        let decoy = cuboid([1.0, 1.0, 1.0], [1.5, 0.0, 0.0], vec![]);
        let found = overlaps(cuboid(
            [0.1, 0.1, 0.1],
            [0.0, 0.0, 0.0],
            vec![parent, decoy],
        ));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            (found[0].a.as_slice(), found[0].b.as_slice()),
            (&[0][..], &[0, 0][..]),
            "{found:?}"
        );
        assert_eq!(
            found[0].b_terminal.as_ref().map(|t| t.mesh.as_str()),
            Some("Block")
        );
        assert!((found[0].area_m2 - 1.0).abs() < 1e-3, "{found:?}");
    }

    /// A mirrored Shape node's terminals face out of them as they are
    /// drawn: mirrored in X inside a box, a block's near end lands in the
    /// box's +X face, facing +X - the winding alone would point that face
    /// back into the block, pressed against the box's face instead of
    /// fighting it. A terminal whose own scope is mirrored, a negative size
    /// the spawner would draw mirrored, is put right the same way: every
    /// face of the block faces out of it.
    #[test]
    fn a_mirrored_terminal_faces_the_way_it_is_drawn() {
        let block = || {
            shape(
                &["Lot --> Extrude(1) I(\"Block\")"],
                [1.0, 0.0, 1.0],
                [1.0, -0.5, -0.5],
            )
        };
        let mut mirrored = block();
        mirrored["transform"]["scale"] = json!([-10_000, 10_000, 10_000]);
        let found = overlaps(cuboid([2.0, 2.0, 2.0], [0.0, 1.0, 0.0], vec![mirrored]));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!((found[0].area_m2 - 1.0).abs() < 1e-3, "{found:?}");

        let node = generator(block());
        let def = node.kind.shape_def().expect("a Shape node");
        let terminal = symbios_shape::Terminal::new(
            symbios_shape::Scope::new(
                symbios_shape::Vec3::new(2.0, 0.0, 0.0),
                symbios_shape::Quat::IDENTITY,
                symbios_shape::Vec3::new(-1.0, 1.0, 1.0),
            ),
            "Block",
        );
        let bake = def.bake(&terminal);
        assert!(bake.transform.scale.x < 0.0, "{:?}", bake.transform);
        let piece = Unit::of(&bake.mesh(), false)
            .and_then(|unit| unit.place(bake.transform.compute_affine(), &[], None))
            .expect("a piece");
        let centre = (piece.min + piece.max) / 2.0;
        assert!(
            (centre - Vec3::new(1.5, 0.5, 0.5)).length() < 1e-4,
            "{centre}"
        );
        for tri in &piece.tris {
            let middle = (tri.v[0] + tri.v[1] + tri.v[2]) / 3.0;
            assert!(
                tri.normal.dot(middle - centre) > 0.0,
                "a face at {middle} faces into the block: {}",
                tri.normal
            );
        }
    }

    /// A grammar that does not parse or derive draws nothing, in the world
    /// and here: it adds no piece, and nothing panics. The same node with
    /// its grammar mended fights the box it is set in.
    #[test]
    fn a_grammar_that_does_not_derive_adds_nothing() {
        let set_in_a_box = |grammar: &[&str]| {
            overlaps(cuboid(
                [2.0, 2.0, 2.0],
                [0.0, 1.0, 0.0],
                vec![shape(grammar, [1.0, 0.0, 1.0], [-0.5, 0.0, -0.5])],
            ))
        };
        let mended = set_in_a_box(&["Lot --> Extrude(1) I(\"Post\")"]);
        assert_eq!(mended.len(), 1, "{mended:?}");
        for broken in [
            // A line that does not parse sinks the lines that do.
            &["Lot --> Extrude(1) I(\"Post\")", "%%% not a statement"][..],
            // No root rule.
            &["Base --> Extrude(1) I(\"Post\")"],
            // Three that parse and fail to derive: a height below zero, a
            // rule called with an argument it does not take, and a rule
            // that never ends.
            &["Lot --> Extrude(scope.x - 2) I(\"Post\")"],
            &["Lot --> Post(1)", "Post --> Extrude(1) I(\"Post\")"],
            &["Lot --> Extrude(1) Lot"],
            // No rules, and rules that emit no terminal.
            &["// a comment", "const H = 1"],
            &["Lot --> NIL"],
            &[""],
        ] {
            let found = set_in_a_box(broken);
            assert!(found.is_empty(), "{broken:?}: {found:?}");
        }
    }

    /// #1507: a set's answer says what each grammar it derived drew, since
    /// an empty `z_fighting` can also mean a grammar drew nothing at all. A
    /// Shape node that derives is answered with how many terminals it
    /// derived; one that does not, with the message the World Editor's
    /// grammar forge shows, ahead of every one that does; one below a node
    /// that does not derive is not derived, as the world spawns nothing
    /// there; and a generator with no grammar answers none.
    #[test]
    fn the_answer_says_what_each_grammar_drew() {
        let good = shape(
            &[
                "Lot --> Extrude(1) Split(X) { ~1: Post | ~1: Post }",
                "Post --> I(\"Post\")",
            ],
            [2.0, 0.0, 1.0],
            [3.0, 0.0, 0.0],
        );
        let mut broken = shape(
            &["Lot --> Extrude(1) I(\"Post\")", "%%% not a statement"],
            [1.0, 0.0, 1.0],
            [-3.0, 0.0, 0.0],
        );
        broken["children"] = json!([shape(
            &["Lot --> Extrude(1) I(\"Post\")"],
            [1.0, 0.0, 1.0],
            [0.0, 2.0, 0.0],
        )]);
        let tree = cuboid([1.0, 1.0, 1.0], [0.0, 0.5, 0.0], vec![good, broken]);
        let report = within(
            &empty(),
            &world(
                HashMap::from([("house".to_owned(), generator(tree))]),
                Vec::new(),
            ),
        );
        assert_eq!(report.grammars_total, 2, "{:?}", report.grammars);
        assert_eq!(report.grammars.len(), 2, "{:?}", report.grammars);
        assert_eq!(
            report.grammars[0]["node"], "/generators/house/children/1",
            "{:?}",
            report.grammars
        );
        assert!(
            report.grammars[0]["error"]
                .as_str()
                .is_some_and(|why| why.starts_with("line 2: ")),
            "{:?}",
            report.grammars
        );
        assert_eq!(
            report.grammars[1],
            json!({ "node": "/generators/house/children/0", "terminals": 2 })
        );

        let plain = within(
            &empty(),
            &world(
                HashMap::from([(
                    "box".to_owned(),
                    generator(cuboid([1.0, 1.0, 1.0], [0.0, 0.5, 0.0], vec![])),
                )]),
                Vec::new(),
            ),
        );
        assert_eq!((plain.grammars.len(), plain.grammars_total), (0, 0));
    }

    /// #1507: at most [`MAX_NAMED`] grammars are answered, those that drew
    /// nothing first - each kind in the order it was derived - and how many
    /// were derived in all. Twenty Shape nodes, the last three of which do
    /// not parse.
    #[test]
    fn the_grammars_that_drew_nothing_are_answered_first() {
        let nodes: Vec<Value> = (0..20)
            .map(|i| {
                let grammar: &[&str] = if i >= 17 {
                    &["Lot --> Extrude(1) I(\"Post\")", "%%% not a statement"]
                } else {
                    &["Lot --> Extrude(1) I(\"Post\")"]
                };
                shape(grammar, [0.5, 0.0, 0.5], [i as f32 * 2.0, 0.0, 0.0])
            })
            .collect();
        let report = within(
            &empty(),
            &world(
                HashMap::from([(
                    "row".to_owned(),
                    generator(cuboid([0.2, 0.2, 0.2], [0.0, -5.0, 0.0], nodes)),
                )]),
                Vec::new(),
            ),
        );
        assert_eq!(report.grammars_total, 20);
        assert_eq!(report.grammars.len(), MAX_NAMED);
        let nodes: Vec<&str> = report
            .grammars
            .iter()
            .map(|status| status["node"].as_str().expect("a pointer"))
            .collect();
        let want: Vec<String> = (17..20)
            .chain(0..MAX_NAMED - 3)
            .map(|i| format!("/generators/row/children/{i}"))
            .collect();
        assert_eq!(nodes, want);
        assert!(
            report.grammars[..3]
                .iter()
                .all(|status| status.get("error").is_some()),
            "{:?}",
            report.grammars
        );
        assert!(
            report.grammars[3..]
                .iter()
                .all(|status| status["terminals"] == 1),
            "{:?}",
            report.grammars
        );
    }

    /// Nor is anything below a grammar that does not derive: the world
    /// spawns a node's children under the entity it spawns for the node,
    /// and none for such a grammar (#1503). A header and a panel sharing
    /// their front plane hang from a Shape node whose post stands clear of
    /// both: named while the grammar derives, and not once a line of it
    /// does not parse.
    #[test]
    fn nothing_below_a_grammar_that_does_not_derive_is_checked() {
        let under = |grammar: &[&str]| {
            let mut node = shape(grammar, [0.2, 0.0, 0.2], [0.0, 0.0, 0.0]);
            node["children"] = json!([
                cuboid([2.0, 1.0, 0.1], [0.0, 3.0, 0.0], vec![]),
                cuboid([0.6, 2.0, 0.1], [0.5, 2.0, 0.0], vec![]),
            ]);
            overlaps(node)
        };
        let mended = under(&["Lot --> Extrude(0.2) I(\"Post\")"]);
        assert_eq!(mended.len(), 1, "{mended:?}");
        assert_eq!(
            (mended[0].a.as_slice(), mended[0].b.as_slice()),
            (&[0][..], &[1][..])
        );
        let broken = under(&["Lot --> Extrude(0.2) I(\"Post\")", "%%% not a statement"]);
        assert!(broken.is_empty(), "{broken:?}");
    }

    /// What is checked is what the world draws (#1503): for a grammar of
    /// boxes, a tapered shaft, a turned column and a gabled roof's flat
    /// panels, under a node turned and moved, the pieces are the spawner's
    /// terminals one for one and in its order, each at the corners of the
    /// mesh the spawner builds, placed as Bevy places a child entity under
    /// its parent.
    #[test]
    fn the_checked_terminals_are_the_spawned_ones() {
        let mut wire = shape(
            &[
                "Lot --> Extrude(3) Split(Y) { 2: Body | ~1: Top }",
                "Body --> Split(X) { 0.5: Column | ~1: Wall | 0.5: Shaft }",
                "Column --> Mat(\"Stone\") I(\"Column\")",
                "Wall --> Mat(\"Plaster\") I(\"Wall\")",
                "Shaft --> Taper(0.3) I(\"Shaft\")",
                "Top --> Roof(Gable, 35, 0.2) { Slope: Tile | GableEnd: Gable }",
                "Tile --> Mat(\"Tile\") I(\"Tile\")",
                "Gable --> I(\"Gable\")",
            ],
            [4.0, 0.0, 2.0],
            [2.0, 0.5, -1.0],
        );
        // A quarter-turn-free rotation that is exactly a unit quaternion
        // on the wire (0.6² + 0.8² = 1), so both readers turn alike.
        wire["transform"]["rotation"] = json!([0, 6_000, 0, 8_000]);
        wire["round_meshes"] = json!(["Column"]);
        let node = generator(wire);

        let pieces = pieces_of(&node);
        let spawned = node
            .kind
            .shape_def()
            .expect("a Shape node")
            .spawned()
            .expect("the grammar derives");

        assert_eq!(pieces.len(), spawned.len());
        let flat = pieces.iter().filter(|piece| !piece.solid).count();
        assert!(flat >= 2, "the roof has flat panels: {flat}");
        let parent = GlobalTransform::from(transform_of(&node.transform));
        let near = |p: Vec3, set: &[Vec3]| set.iter().any(|q| q.distance(p) < 1e-4);
        for (index, (piece, (transform, positions))) in pieces.iter().zip(&spawned).enumerate() {
            assert_eq!(piece.terminal.as_ref().map(|t| t.index), Some(index));
            let placed = parent.mul_transform(*transform);
            let drawn: Vec<Vec3> = positions
                .iter()
                .map(|p| placed.transform_point(*p))
                .collect();
            let corners: Vec<Vec3> = piece.tris.iter().flat_map(|tri| tri.v).collect();
            assert!(
                drawn.iter().all(|p| near(*p, &corners))
                    && corners.iter().all(|p| near(*p, &drawn)),
                "terminal {index} ({:?}) is not where the spawner draws it",
                piece.terminal
            );
        }
    }

    /// A patch buried inside a terminal's solid is not named: two bars
    /// crossing at one height, their crossing inside a grammar's post.
    #[test]
    fn a_patch_buried_inside_a_terminal_is_not_named() {
        let bars = |post: Option<Value>| {
            let mut root = cuboid([2.0, 0.1, 0.1], [0.0, 1.0, 0.0], vec![]);
            let mut children = vec![cuboid([0.1, 0.1, 2.0], [0.0, 0.0, 0.0], vec![])];
            children.extend(post);
            root["children"] = json!(children);
            overlaps(root)
        };
        let open = bars(None);
        assert_eq!(
            open.len(),
            1,
            "the crossing shows without the post: {open:?}"
        );
        // A 0.4 m post from the ground to 3 m, around the crossing at 1 m.
        let post = shape(
            &["Lot --> Extrude(3) I(\"Post\")"],
            [0.4, 0.0, 0.4],
            [-0.2, -1.0, -0.2],
        );
        let buried = bars(Some(post));
        assert!(buried.is_empty(), "{buried:?}");
    }

    /// A flat panel - a hip roof's slope, a hip end - is two faces back to
    /// back with nothing between them, so no point is inside it: not beside it,
    /// and not in its own plane, where a ray cast from the point meets both
    /// faces at the point itself and rounding counts one and not the other
    /// (a few hundred of this roof's points, read by crossings alone). The
    /// wall under it is a solid, and a point in it is inside.
    #[test]
    fn a_flat_panel_has_no_inside() {
        let mut wire = shape(
            &[
                "Lot --> Extrude(3) Split(Y) { 2: Body | ~1: Top }",
                "Body --> I(\"Wall\")",
                "Top --> Roof(Hip, 35, 0.2) { Slope: Tile }",
                "Tile --> I(\"Tile\")",
            ],
            [4.0, 0.0, 2.0],
            [2.0, 0.5, -1.0],
        );
        wire["transform"]["rotation"] = json!([0, 2_588, 0, 9_659]);
        let node = generator(wire);
        let pieces = pieces_of(&node);
        let named = |mesh: &str| {
            pieces
                .iter()
                .filter(|piece| piece.terminal.as_ref().is_some_and(|t| t.mesh == mesh))
                .collect::<Vec<&Piece>>()
        };
        let panels = named("Tile");
        assert_eq!(panels.len(), 4, "two trapezoid slopes and two hip ends");
        let wall = named("Wall");
        assert_eq!(wall.len(), 1);
        let middle = (wall[0].min + wall[0].max) / 2.0;
        assert!(wall[0].contains(middle), "the wall is a solid");
        let steps = 24;
        for panel in panels {
            for tri in &panel.tris {
                for i in 0..=steps {
                    for j in 0..=steps - i {
                        let (u, v) = (i as f32 / steps as f32, j as f32 / steps as f32);
                        let point =
                            tri.v[0] + (tri.v[1] - tri.v[0]) * u + (tri.v[2] - tri.v[0]) * v;
                        for off in [-0.01, 0.0, 0.01] {
                            let at = point + tri.normal * off;
                            assert!(!panel.contains(at), "{at} is inside {:?}", panel.terminal);
                        }
                    }
                }
            }
        }
    }

    /// The faces a grammar splits off flat meet at their edges without
    /// fighting, though the mesher draws each as a slab 1 mm thick and one
    /// slab's end lies half a millimetre from the next one's face: a box
    /// drawn as its four walls and its roof, a gabled roof with no
    /// overhang, whose sloped slabs end in the plane of its gable ends, and
    /// a tower of three floors whose faces are piers and glazing. A face
    /// drawn twice in one plane still fights: the box's front, its second
    /// half pushed a metre along into its first.
    #[test]
    fn faces_a_grammar_splits_off_meet_at_their_edges_unnamed() {
        let found = |grammar: &[&str]| overlaps(shape(grammar, [6.0, 0.0, 4.0], [0.0, 0.0, 0.0]));
        let walls = found(&[
            "Lot --> Extrude(3) Comp(Faces) { Side: Wall | Top: Wall | Bottom: NIL }",
            "Wall --> Mat(\"Plaster\") I(\"Wall\")",
        ]);
        assert!(walls.is_empty(), "{walls:?}");
        let gable = found(&[
            "Lot --> Extrude(5) Split(Y) { 3: Body | ~1: Top }",
            "Body --> I(\"Wall\")",
            "Top --> Roof(Gable, 35) { Slope: Tile | GableEnd: Gable }",
            "Tile --> I(\"Tile\")",
            "Gable --> I(\"Gable\")",
        ]);
        assert!(gable.is_empty(), "{gable:?}");
        let tower = found(&[
            "Lot --> Extrude(9) Comp(Faces) { Side: Facade | Top: NIL | Bottom: NIL }",
            "Facade --> Repeat(Y, 3) { Floor }",
            "Floor --> Split(X) { 1: Pier | ~1: Glazing | 1: Pier }",
            "Pier --> Mat(\"Stone\") I(\"Pier\")",
            "Glazing --> Mat(\"Glass\") I(\"Glazing\")",
        ]);
        assert!(tower.is_empty(), "{tower:?}");

        let twice = found(&[
            "Lot --> Extrude(3) Comp(Faces) { Front: Facade | Side: Wall | Top: Wall | Bottom: NIL }",
            "Facade --> Split(X) { ~1: Wall | ~1: Pushed }",
            "Pushed --> Translate(-1, 0, 0) I(\"Wall\")",
            "Wall --> Mat(\"Plaster\") I(\"Wall\")",
        ]);
        assert_eq!(twice.len(), 1, "{twice:?}");
        // Front and back of the two slabs, 1 m x 3 m each.
        assert!((twice[0].area_m2 - 6.0).abs() < 1e-2, "{twice:?}");
    }

    /// Where a terminal is one of the pair, a strip of shared face
    /// narrower than 2 mm is not named: a panel run 1 mm back into a post
    /// shares their fronts, tops and bottoms along strips that thin, and
    /// run 5 mm back, the same panel is named over all three - whether the
    /// post is the grammar's own terminal or a primitive the grammar's node
    /// hangs from.
    #[test]
    fn a_strip_narrower_than_two_millimetres_is_not_named_where_a_terminal_is() {
        let in_the_grammar = |metres: f32| {
            overlaps(shape(
                &[
                    "Lot --> Extrude(2) Split(X) { ~1: Left | ~1: Right }",
                    "Left --> I(\"Post\")",
                    &format!(
                        "Right --> Translate(-{metres}, 0, 0) \
                         Size(scope.x + {metres}, scope.y, scope.z * 0.5) I(\"Panel\")"
                    ),
                ],
                [4.0, 0.0, 2.0],
                [0.0, 1.0, 0.0],
            ))
        };
        // A 2 m cube standing on the ground, and a grammar's 2 x 2 x 1 m
        // panel from its +X face on, run back into it.
        let on_a_primitive = |metres: f32| {
            let mut post = cuboid([2.0, 2.0, 2.0], [0.0, 1.0, 0.0], vec![]);
            post["children"] = json!([shape(
                &["Lot --> Extrude(2) I(\"Panel\")"],
                [2.0, 0.0, 1.0],
                [1.0 - metres, -1.0, -1.0],
            )]);
            overlaps(post)
        };
        for (label, run_back) in [
            (
                "two terminals",
                &in_the_grammar as &dyn Fn(f32) -> Vec<Overlap>,
            ),
            ("a terminal and a primitive", &on_a_primitive),
        ] {
            let thin = run_back(0.001);
            assert!(thin.is_empty(), "{label}: {thin:?}");
            let wide = run_back(0.005);
            assert_eq!(wide.len(), 1, "{label}: {wide:?}");
            // The front 2 m high, the top and the bottom 1 m deep: 4 m of
            // strip, 5 mm wide.
            assert!((wide[0].area_m2 - 0.02).abs() < 1e-4, "{label}: {wide:?}");
        }
    }

    /// A pair of primitives keeps the rules it was checked by before
    /// terminals were: a board whose front shares a strip 1 mm wide with a
    /// block's front, and whose top and bottom share strips as thin with
    /// the block's, is named over all three.
    #[test]
    fn a_thin_strip_between_primitives_is_named_as_before() {
        let found = overlaps(cuboid(
            [2.0, 2.0, 2.0],
            [0.0, 1.0, 0.0],
            vec![cuboid([2.0, 2.0, 1.0], [1.999, 0.0, -0.5], vec![])],
        ));
        assert_eq!(found.len(), 1, "{found:?}");
        // 1 mm by 2 m in front, by 1 m on top and by 1 m underneath.
        assert!((found[0].area_m2 - 0.004).abs() < 1e-4, "{found:?}");
    }

    /// The 2 mm rule holds whichever piece of the pair is collected first:
    /// a grammar's node carrying a post as its child is collected before
    /// the post, so its panel is the pair's `a` and the post its `b` - the
    /// other way round from a post that carries the grammar's node.
    #[test]
    fn a_thin_strip_is_not_named_where_the_terminal_is_collected_first() {
        let over_a_primitive = |metres: f32| {
            let mut panel = shape(
                &["Lot --> Extrude(2) I(\"Panel\")"],
                [2.0, 0.0, 1.0],
                [1.0 - metres, 0.0, -1.0],
            );
            panel["children"] = json!([cuboid([2.0, 2.0, 2.0], [metres - 1.0, 1.0, 1.0], vec![])]);
            overlaps(panel)
        };
        let wide = over_a_primitive(0.005);
        assert_eq!(wide.len(), 1, "{wide:?}");
        assert!(
            wide[0].a_terminal.is_some() && wide[0].b_terminal.is_none(),
            "the terminal is the pair's first piece: {wide:?}"
        );
        // The front 2 m high, the top and the bottom 1 m deep: 4 m of
        // strip, 5 mm wide.
        assert!((wide[0].area_m2 - 0.02).abs() < 1e-4, "{wide:?}");
        let thin = over_a_primitive(0.001);
        assert!(thin.is_empty(), "{thin:?}");
    }

    /// A round cap flush with a terminal's face flickers over its whole
    /// disc, and is named so (#1503): Bevy draws a cylinder's cap as a fan
    /// from one rim vertex, every sliver of it narrower than 2 mm on a
    /// 4 cm cap of 128 sides - and the block's face cuts a 19 mm cap of the
    /// default 32 into pieces as thin - yet the disc is no strip. The whole
    /// polygon is named, as it is against a primitive.
    #[test]
    fn a_fine_round_cap_flush_with_a_terminal_is_named_over_its_disc() {
        for (radius, resolution) in [(0.04_f32, 128_u32), (0.019, 32)] {
            let found = overlaps(json!({
                "$type": "network.symbios.gen.cylinder",
                "radius": (radius * 10_000.0).round() as i64,
                "height": 5_000,
                "resolution": resolution,
                "solid": true,
                "material": {},
                "transform": { "translation": [0, 7_500, 0] },
                "children": [shape(
                    &["Lot --> Extrude(1) I(\"Block\")"],
                    [1.0, 0.0, 1.0],
                    [-0.5, -0.75, -0.5],
                )],
            }));
            assert_eq!(found.len(), 1, "r {radius}, n {resolution}: {found:?}");
            let n = resolution as f32;
            let polygon = 0.5 * n * radius * radius * (std::f32::consts::TAU / n).sin();
            assert!(
                (found[0].area_m2 - polygon).abs() < 1e-5,
                "r {radius}, n {resolution}: {found:?} against a polygon of {polygon}"
            );
        }
    }

    /// Two round caps of one piece flush with one terminal's face are two
    /// discs, each named over its whole polygon (#1503): a bent pipe whose
    /// two ends, 10 m apart, stand in a block's top face. Each cap is a fan
    /// of slivers narrower than 2 mm - on a 2 cm pipe of 64 sides, and a
    /// 3 cm one of 128. How wide a patch is, is judged over the stretch of
    /// face it lies in, not over all the pair shares in its plane - whose
    /// box, 10 m across, made the two discs read as one strip, so that
    /// neither was named. A pipe with one end in the face is the control.
    #[test]
    fn two_round_caps_in_one_terminal_face_are_named_over_both_discs() {
        let wire = |metres: f32| (metres * 10_000.0).round() as i64;
        let pipe = |radius: f32, resolution: u32, points: &[[f32; 2]]| {
            let points: Vec<Value> = points
                .iter()
                .map(
                    |[x, y]| json!({ "position": [wire(*x), wire(*y), 0], "radius": wire(radius) }),
                )
                .collect();
            overlaps(json!({
                "$type": "network.symbios.gen.spine",
                "points": points,
                "resolution": resolution,
                "samples_per_segment": 8,
                "solid": true,
                "material": {},
                // A block 12 m long and 1 m deep, its top face at y = 0,
                // where each end of the pipe stands.
                "children": [shape(
                    &["Lot --> Extrude(1) I(\"Block\")"],
                    [12.0, 0.0, 1.0],
                    [-6.0, -1.0, -0.5],
                )],
            }))
        };
        for (radius, resolution) in [(0.02_f32, 64_u32), (0.03, 128)] {
            let n = resolution as f32;
            let disc = 0.5 * n * radius * radius * (std::f32::consts::TAU / n).sin();
            let case = format!("r {radius}, n {resolution}");
            let one = pipe(
                radius,
                resolution,
                &[[-5.0, 0.0], [-5.0, -0.5], [-4.9, -0.8], [-4.0, -0.8]],
            );
            assert_eq!(
                one.len(),
                1,
                "fixture: one end in the face, {case}: {one:?}"
            );
            assert!(
                (one[0].area_m2 - disc).abs() < 1e-5,
                "fixture, {case}: {one:?} against a disc of {disc}"
            );
            let both = pipe(
                radius,
                resolution,
                &[
                    [-5.0, 0.0],
                    [-5.0, -0.5],
                    [-4.9, -0.8],
                    [4.9, -0.8],
                    [5.0, -0.5],
                    [5.0, 0.0],
                ],
            );
            assert_eq!(both.len(), 1, "{case}: {both:?}");
            assert!(
                (both[0].area_m2 - 2.0 * disc).abs() < 2e-5,
                "{case}: {both:?} against two discs of {disc}"
            );
        }
    }

    /// A budget whose clock says the time is spent at its `n`th ask,
    /// counting from 1.
    fn spent_at(n: usize) -> Budget {
        let mut asked = 0;
        Budget::new(move || {
            asked += 1;
            asked >= n
        })
    }

    /// A budget that never runs out, and how many times its clock has been
    /// asked.
    fn counting() -> (Budget, std::rc::Rc<std::cell::Cell<usize>>) {
        let asked = std::rc::Rc::new(std::cell::Cell::new(0));
        let counter = asked.clone();
        let budget = Budget::new(move || {
            counter.set(counter.get() + 1);
            false
        });
        (budget, asked)
    }

    /// How many times `run` asks the clock, given all the time it wants.
    fn asks(run: impl FnOnce(&mut Budget)) -> usize {
        let (mut budget, asked) = counting();
        run(&mut budget);
        asked.get()
    }

    /// Out of time, the check stops where it is: it names the pairs it has
    /// measured, and lists by pointer each generator it had not finished,
    /// the one it ran out in and every one after it. It asks the clock as
    /// it collects the pieces, before each pair it compares, before each
    /// triangle of the pair's first piece, and before each patch whose
    /// burial it probes, so that no one step can run on for long. "a" is a
    /// block round a smaller one, whose boxes meet though no faces do; "b"
    /// is a header over two panels.
    #[test]
    fn a_check_out_of_time_names_what_it_found_and_lists_what_it_did_not_finish() {
        let nested = generator(cuboid(
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
            vec![cuboid([0.5, 0.5, 0.5], [0.0, 0.0, 0.0], vec![])],
        ));
        let header = generator(cuboid(
            [4.0, 1.0, 0.1],
            [0.0, 3.0, 0.0],
            vec![
                cuboid([0.6, 2.0, 0.1], [-1.0, -1.0, 0.0], vec![]),
                cuboid([0.6, 2.0, 0.1], [1.0, -1.0, 0.0], vec![]),
            ],
        ));
        let after = HashMap::from([
            ("a".to_owned(), nested.clone()),
            ("b".to_owned(), header.clone()),
        ]);
        let (empty, after) = (empty(), world(after, Vec::new()));
        let within = |budget| report_within(&empty, &after, budget);
        let unchecked = |names: &[&str]| Report {
            unchecked: names.iter().map(|n| format!("/generators/{n}")).collect(),
            ..Report::default()
        };

        let full = within(Budget::new(|| false));
        assert_eq!((full.total, full.unchecked.len()), (2, 0), "{full:?}");
        // Out of time before it begins.
        assert_eq!(within(spent_at(1)), unchecked(&["a", "b"]));
        // Out of time as it comes to compare "a"'s one pair, which has no
        // patch to probe.
        let collecting_a = asks(|budget| {
            pieces_within(&nested, budget);
        });
        assert_eq!(within(spent_at(collecting_a + 1)), unchecked(&["a", "b"]));
        // Out of time half way through measuring "b"'s first pair: that
        // pair is not named.
        let checking_a = asks(|budget| {
            check(&nested, budget);
        });
        let collecting_b = asks(|budget| {
            pieces_within(&header, budget);
        });
        assert_eq!(
            within(spent_at(checking_a + collecting_b + 2)),
            unchecked(&["b"])
        );
        // Out of time at any ask of the clock before its last, it has not
        // finished "b"; what it names is some of what it names given time,
        // and somewhere it names one of "b"'s pairs and not the other.
        let (budget, counted) = counting();
        within(budget);
        let asked = counted.get();
        let mut partial = false;
        for n in 1..=asked {
            let cut = within(spent_at(n));
            assert!(!cut.unchecked.is_empty(), "spent at ask {n}: {cut:?}");
            assert!(
                cut.named.iter().all(|pair| full.named.contains(pair)),
                "spent at ask {n}: {cut:?}"
            );
            partial |= cut.named.len() == 1;
        }
        assert!(partial, "a pair measured before the time ran out is named");
        assert_eq!(within(spent_at(asked + 1)), full);
    }

    /// Out of time while a grammar's terminals are collected, the check
    /// stops collecting them: its node found at the first ask of the clock,
    /// one terminal placed at the second, and nothing more at the third.
    #[test]
    fn collecting_stops_once_the_time_is_spent() {
        let tower = generator(shape(
            &[
                "Lot --> Extrude(9) Comp(Faces) { Side: Facade | Top: NIL | Bottom: NIL }",
                "Facade --> Repeat(Y, 3) { Floor }",
                "Floor --> Split(X) { 1: Pier | ~1: Glazing | 1: Pier }",
                "Pier --> I(\"Pier\")",
                "Glazing --> I(\"Glazing\")",
            ],
            [6.0, 0.0, 4.0],
            [0.0, 0.0, 0.0],
        ));
        assert_eq!(pieces_of(&tower).len(), 36);
        assert_eq!(pieces_within(&tower, &mut spent_at(3)).len(), 1);
    }

    /// Two pieces whose boxes meet but whose faces share no plane are
    /// compared triangle against triangle with no patch to probe, and the
    /// clock is asked all the same, before each triangle of the pair's
    /// first piece (#1503): a ball in a slightly larger ball, each of the
    /// sanitiser's finest, stops at whichever ask the time runs out at.
    /// Bevy's ico sphere splits each edge of the icosahedron's 20 faces
    /// into n + 1, so the finest, 6, is 20 x 7 x 7 = 980 triangles.
    #[test]
    fn a_pair_sharing_no_plane_asks_the_clock_as_it_compares() {
        let ball = |radius: i64, at: [i64; 3], children: Vec<Value>| {
            json!({
                "$type": "network.symbios.gen.sphere",
                "radius": radius,
                "resolution": 6,
                "solid": true,
                "material": {},
                "transform": { "translation": at },
                "children": children,
            })
        };
        let node = generator(ball(
            10_500,
            [0, 20_000, 0],
            vec![ball(10_000, [0, 0, 0], vec![])],
        ));
        let pieces = pieces_of(&node);
        assert_eq!(
            pieces.iter().map(|p| p.tris.len()).collect::<Vec<_>>(),
            [980, 980]
        );
        let tree = BoxTree::new(pieces.iter().map(|piece| (piece.min, piece.max)).collect());
        let measure = |budget: &mut Budget| {
            visible_shared_area(&pieces[0], &pieces[1], &pieces, [0, 1], &tree, budget)
        };
        assert_eq!(
            measure(&mut Budget::new(|| false)),
            Some(0.0),
            "fixture: no plane is shared"
        );
        let asked = asks(|budget| {
            measure(budget);
        });
        assert!(asked > 100, "{asked} asks");
        for n in [1, asked / 2, asked] {
            assert_eq!(measure(&mut spent_at(n)), None, "spent at ask {n}");
        }
        assert_eq!(measure(&mut spent_at(asked + 1)), Some(0.0));
    }

    /// `room set`'s check stops at [`CHECK_TIME`] however much is left,
    /// well inside the time the agent waits for its answer: a grammar of
    /// 20 000 blocks 10 m across, each half a millimetre along from the
    /// last so that every one fights every other - two hundred million
    /// pairs - is answered in time, listed as not finished, with the
    /// largest pairs it found by then named.
    #[test]
    fn a_grammar_too_big_to_check_in_time_is_answered_in_time() {
        let stack = generator(shape(
            &[
                "Lot --> Repeat(X, 0.0005) { Block }",
                "Block --> Size(10, 3, 10) I(\"Block\")",
            ],
            [10.0, 0.0, 10.0],
            [0.0, 0.0, 0.0],
        ));
        let started = Instant::now();
        let answer = report(
            &empty(),
            &world(HashMap::from([("stack".to_owned(), stack)]), Vec::new()),
        );
        let took = started.elapsed();
        assert!(took < WORLD_ANSWER_TIMEOUT, "{took:?}");
        assert_eq!(answer.unchecked, ["/generators/stack"]);
        assert_eq!(answer.named.len(), MAX_NAMED);
    }

    /// The answer carries `z_fighting` always, `z_fighting_total` only when
    /// more pairs were found than named, `z_fighting_unchecked` only when
    /// the check ran out of time, and `grammars` only when it derived one,
    /// with `grammars_total` only when more were derived than named (#1507).
    #[test]
    fn the_answer_lists_what_was_not_checked_only_when_something_was_not() {
        let mut answer = json!({ "changed": true });
        Report::default().answer(&mut answer);
        assert_eq!(answer, json!({ "changed": true, "z_fighting": [] }));

        let mut answer = json!({ "changed": true });
        Report {
            named: vec![json!({ "a": "/generators/g" })],
            total: 3,
            unchecked: vec!["/generators/g".to_owned()],
            grammars: vec![json!({ "node": "/generators/g", "terminals": 2 })],
            grammars_total: 5,
        }
        .answer(&mut answer);
        assert_eq!(
            answer,
            json!({
                "changed": true,
                "z_fighting": [{ "a": "/generators/g" }],
                "z_fighting_total": 3,
                "z_fighting_unchecked": ["/generators/g"],
                "grammars": [{ "node": "/generators/g", "terminals": 2 }],
                "grammars_total": 5,
            })
        );

        let mut answer = json!({ "changed": true });
        Report {
            grammars: vec![json!({ "node": "/generators/g", "error": "line 1: no" })],
            grammars_total: 1,
            ..Report::default()
        }
        .answer(&mut answer);
        assert_eq!(
            answer,
            json!({
                "changed": true,
                "z_fighting": [],
                "grammars": [{ "node": "/generators/g", "error": "line 1: no" }],
            })
        );
    }

    /// The box tree finds every box a scan of them all finds, and no
    /// other, and passes over most of the rest: 1000 boxes of a grid, each
    /// jittered from its cell so that neighbours overlap, touch or miss,
    /// asked about each box and about each box's lowest corner.
    #[test]
    fn the_box_tree_finds_what_a_scan_finds_and_passes_over_the_rest() {
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut unit = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 40) as f32 / (1_u64 << 24) as f32
        };
        let boxes: Vec<(Vec3, Vec3)> = (0..1000)
            .map(|i| {
                let cell = Vec3::new((i % 10) as f32, (i / 10 % 10) as f32, (i / 100) as f32);
                let lo = cell + Vec3::new(unit(), unit(), unit()) * 0.5;
                (
                    lo,
                    lo + Vec3::new(unit(), unit(), unit()) + Vec3::splat(0.25),
                )
            })
            .collect();
        let tree = BoxTree::new(boxes.clone());
        let mut most = 0;
        for (lo, hi) in boxes.iter().flat_map(|&(lo, hi)| [(lo, hi), (lo, lo)]) {
            let before = tree.compared.get();
            let mut found = Vec::new();
            let any = tree.any_meeting(lo, hi, |i| {
                found.push(i);
                false
            });
            most = most.max(tree.compared.get() - before);
            found.sort_unstable();
            let scan: Vec<usize> = (0..boxes.len())
                .filter(|&i| boxes_meet(boxes[i].0, boxes[i].1, lo, hi))
                .collect();
            assert!(!any);
            assert_eq!(found, scan, "{lo}..{hi}");
            // Stopped at the first `each` that says so.
            let last = *scan.last().expect("a box meets itself");
            assert!(tree.any_meeting(lo, hi, |i| i == last));
        }
        assert!(most <= 150, "{most} of 1000 boxes compared at most");
    }

    /// An absolute placement at the world's origin, with `seed` or none.
    fn absolute(name: &str, seed: Option<u64>) -> Placement {
        Placement::Absolute {
            generator_ref: name.to_owned(),
            transform: Default::default(),
            snap_to_terrain: true,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
            seed,
        }
    }

    /// A world holding one generator, `house`, placed by each of `seeds`.
    fn housed(house: Value, seeds: &[Option<u64>]) -> RoomRecord {
        world(
            HashMap::from([("house".to_owned(), generator(house))]),
            seeds.iter().map(|seed| absolute("house", *seed)).collect(),
        )
    }

    fn within(before: &RoomRecord, after: &RoomRecord) -> Report {
        report_within(before, after, Budget::new(|| false))
    }

    /// `report` with what it says of the grammars it derived left out
    /// (#1507), for a test of what it says of z-fighting.
    fn fights(report: Report) -> Report {
        Report {
            grammars: Vec::new(),
            grammars_total: 0,
            ..report
        }
    }

    /// #1505: a placement's grammar seed draws its generator as the
    /// generator's own seed does not, and what it draws is checked. A house
    /// whose own seed leaves its panel beside its post fights nowhere, set
    /// on its own or placed with no seed or its own seed; placed with a
    /// seed that pushes the panel into the post, the pair is named by the
    /// generator's pointers - the node's and each terminal's - and the
    /// placement's.
    #[test]
    fn a_placement_s_grammar_seed_is_checked_as_it_draws_the_generator() {
        let (beside, pushed) = coin_seeds();
        let alone = housed(coin_house(beside), &[]);
        assert_eq!(
            fights(within(&empty(), &alone)),
            Report::default(),
            "its own seed draws nothing twice"
        );
        for seed in [None, Some(beside)] {
            // Not checked again, so the clock is never asked: a pair this
            // tree names is dropped as the generator's own, and would hide
            // a check that ran for nothing (#1505).
            let (budget, asked) = counting();
            assert_eq!(
                report_within(&alone, &housed(coin_house(beside), &[seed]), budget),
                Report::default(),
                "placed with {seed:?}, it draws what its generator draws"
            );
            assert_eq!(asked.get(), 0, "placed with {seed:?}, it is not checked");
        }

        let seeded = within(&alone, &housed(coin_house(beside), &[None, Some(pushed)]));
        assert_eq!(seeded.total, 1, "{seeded:?}");
        assert!(seeded.unchecked.is_empty(), "{seeded:?}");
        assert_eq!(
            seeded.named[0],
            json!({
                "a": "/generators/house",
                "a_terminal": { "index": 0, "mesh": "Post", "material": null },
                "b": "/generators/house",
                "b_terminal": { "index": 1, "mesh": "Panel", "material": null },
                "area_m2": 4.0,
                "placement": "/placements/1",
            })
        );
    }

    /// #1507: a grammar a placement's seed draws (#1505) is answered as
    /// that seed draws it, by its node's pointer and the placement's; the
    /// generator as it draws itself is answered, without one, where the set
    /// changed the generator.
    #[test]
    fn a_grammar_a_placement_s_seed_draws_is_answered_with_the_placement() {
        let (beside, pushed) = coin_seeds();
        let alone = housed(coin_house(beside), &[]);
        let own = within(&empty(), &alone);
        assert_eq!(
            own.grammars,
            [json!({ "node": "/generators/house", "terminals": 2 })]
        );
        let seeded = within(&alone, &housed(coin_house(beside), &[None, Some(pushed)]));
        assert_eq!(
            seeded.grammars,
            [json!({
                "node": "/generators/house",
                "terminals": 2,
                "placement": "/placements/1",
            })]
        );
        assert_eq!(seeded.grammars_total, 1);
    }

    /// #1505: what a seed draws is checked when its placement did not draw
    /// it so before the set, and only then - a set that leaves the seeded
    /// placement's generator and seed as they were, moving it or writing
    /// nothing at all, names nothing; one that changes its generator checks
    /// it again, as its seed draws it. Two placements given one seed in one
    /// set are checked once, named by the first.
    #[test]
    fn a_grammar_seed_is_checked_when_what_it_draws_is_new() {
        let (beside, pushed) = coin_seeds();
        let placed = housed(coin_house(beside), &[Some(pushed), None, Some(pushed)]);
        let once = within(&housed(coin_house(beside), &[]), &placed);
        assert_eq!(
            once.total, 1,
            "one generator and seed, checked once: {once:?}"
        );
        assert_eq!(once.named[0]["placement"], "/placements/0");

        assert_eq!(
            within(&placed, &placed),
            Report::default(),
            "nothing changed"
        );
        let mut moved = placed.clone();
        let Placement::Absolute { transform, .. } = &mut moved.placements[0] else {
            panic!("absolute");
        };
        transform.translation.0[0] += 5.0;
        assert_eq!(
            within(&placed, &moved),
            Report::default(),
            "moved, it draws what it drew"
        );

        // The generator raised half a metre: changed, so what its seed draws
        // is new, though the seed is not.
        let mut raised = coin_house(beside);
        raised["transform"]["translation"][1] = json!(15_000);
        let edited = housed(raised, &[Some(pushed), None, Some(pushed)]);
        let again = within(&placed, &edited);
        assert_eq!(again.total, 1, "{again:?}");
        assert_eq!(again.named[0]["placement"], "/placements/0");
    }

    /// #1505: a seed another placement drew before the set is checked for
    /// the placement given it now - nothing says the first drawing was ever
    /// checked: it may have been given in the World Editor, or listed as
    /// unchecked when the time ran out. Only the placement that drew that
    /// generator and seed itself before the set is not checked again.
    #[test]
    fn a_seed_another_placement_already_draws_is_checked_for_the_one_given_it() {
        let (beside, pushed) = coin_seeds();
        let before = housed(coin_house(beside), &[Some(pushed), None]);
        let after = housed(coin_house(beside), &[Some(pushed), Some(pushed)]);
        let seeded = within(&before, &after);
        assert_eq!(seeded.total, 1, "{seeded:?}");
        assert_eq!(seeded.named[0]["placement"], "/placements/1");
    }

    /// #1505: a seed draws nothing new in a tree with no shape grammar, so a
    /// seeded placement of one is not checked: the fighting header and panel
    /// of a garage front, placed with a seed, name nothing the set did not
    /// change. Set itself, the same generator still names its pair.
    #[test]
    fn a_seed_on_a_tree_with_no_grammar_is_not_checked() {
        let garage = cuboid(
            [2.0, 1.0, 0.1],
            [0.0, 3.0, 0.0],
            vec![cuboid([0.6, 2.0, 0.1], [0.5, -1.0, 0.0], vec![])],
        );
        let before = housed(garage.clone(), &[]);
        assert_eq!(within(&empty(), &before).total, 1, "the garage fights");
        // Not checked at all, so the clock is never asked: the garage's
        // pair, found again, would be dropped as the generator's own and
        // hide a check that ran for nothing.
        let (budget, asked) = counting();
        assert_eq!(
            report_within(&before, &housed(garage, &[Some(7)]), budget),
            Report::default()
        );
        assert_eq!(
            asked.get(),
            0,
            "a seed that draws what its generator draws is not checked again"
        );
    }

    /// #1505: out of time before a placement's seed is checked, the check
    /// lists the placement by its pointer - the generator as it draws
    /// itself was not what it had left to do.
    #[test]
    fn a_seeded_placement_out_of_time_is_listed_by_its_pointer() {
        let (beside, pushed) = coin_seeds();
        let before = housed(coin_house(beside), &[None]);
        let after = housed(coin_house(beside), &[None, Some(pushed)]);
        let cut = report_within(&before, &after, spent_at(1));
        assert_eq!(cut.unchecked, ["/placements/1"], "{cut:?}");
        assert!(cut.named.is_empty(), "{cut:?}");
    }

    /// A seed for [`coin_house`] other than `beside` that also leaves its
    /// panel beside its post: a seed that draws what the house's own does.
    fn another_quiet_seed(beside: u64) -> u64 {
        (1..64)
            .find(|&seed| {
                let house = generator(coin_house(seed));
                seed != beside && check(&house, &mut Budget::new(|| false)).pairs.is_empty()
            })
            .expect("a second seed that leaves the panel beside")
    }

    /// A garage in wire form (#1505) whose header and panel fight, 0.6 m²
    /// of face, holding two grammars 20 m either side: one that always
    /// pushes its panel into its post, 4 m², and [`coin_house`] with its
    /// own seed `beside`, second of the garage's children, which tosses its
    /// coin.
    fn garage_of_grammars(beside: u64) -> Value {
        let mut coin = coin_house(beside);
        coin["transform"]["translation"] = json!([200_000, 10_000, 0]);
        let mut always = coin_house(beside);
        always["grammar_source"] = json!(
            [
                "Lot --> Extrude(2) Split(X) { ~1: Left | ~1: Right }",
                "Left --> I(\"Post\")",
                "Right --> Translate(-0.5, 0, 0) I(\"Panel\")",
            ]
            .join("\n")
        );
        always["transform"]["translation"] = json!([-200_000, 10_000, 0]);
        cuboid(
            [2.0, 1.0, 0.1],
            [0.0, 3.0, 0.0],
            vec![
                cuboid([0.6, 2.0, 0.1], [0.5, -1.0, 0.0], vec![]),
                coin,
                always,
            ],
        )
    }

    /// #1505: what a seeded placement's tree draws is named only where the
    /// generator does not draw it with its own seeds. Two primitives, and
    /// two terminals a grammar places alike whatever its seed, fight the
    /// same with every seed: that is the generator's, named when it is set,
    /// and not again for each placement that gives it a seed, as though the
    /// seed had drawn it. A garage's header and panel fight, and so do a
    /// post and a panel a grammar always pushes into it; a second grammar
    /// tosses its coin. With a seed that leaves the coin's panel beside its
    /// post, nothing is named; with one that pushes it in, that pair alone,
    /// with the placement's pointer.
    #[test]
    fn a_seeded_placement_is_named_for_what_its_seed_draws_alone() {
        let (beside, pushed) = coin_seeds();
        let quiet = another_quiet_seed(beside);
        let garage = garage_of_grammars(beside);
        let set = housed(garage.clone(), &[None]);
        let own = within(&empty(), &set);
        assert_eq!(own.total, 2, "fixture: the garage and the pushed panel");

        assert_eq!(
            fights(within(&set, &housed(garage.clone(), &[None, Some(quiet)]))),
            Report::default(),
            "seed {quiet} draws no pair its generator does not"
        );
        let seeded = within(&set, &housed(garage, &[None, Some(pushed)]));
        assert_eq!(seeded.total, 1, "{seeded:?}");
        assert_eq!(
            seeded.named[0],
            json!({
                "a": "/generators/house/children/1",
                "a_terminal": { "index": 0, "mesh": "Post", "material": null },
                "b": "/generators/house/children/1",
                "b_terminal": { "index": 1, "mesh": "Panel", "material": null },
                "area_m2": 4.0,
                "placement": "/placements/1",
            })
        );
    }

    /// #1505: a pair the generator's own seeds bury, a seed that uncovers
    /// it names - buried or not is the one part of a pair of primitives a
    /// seed can change. A garage's header and panel fight where they
    /// overlap, unless a grammar's block stands over that patch, as its own
    /// seed stands it; a seed that moves the block away uncovers them, and
    /// the pair is named for the placement.
    #[test]
    fn a_seed_that_uncovers_a_pair_its_generator_buries_names_it() {
        let grammar = [
            "Lot --> 50% Cover | 50% Away",
            "Cover --> Extrude(0.9) I(\"Block\")",
            "Away --> Translate(10, 0, 0) Extrude(0.9) I(\"Block\")",
        ]
        .join("\n");
        let block = |seed: u64| {
            json!({
                "$type": "network.symbios.gen.shape",
                "grammar_source": grammar,
                "root_rule": "Lot",
                "footprint": [8_000, 0, 8_000],
                "seed": seed.to_string(),
                "transform": { "translation": [1_000, -7_000, -4_000] },
            })
        };
        let garage = |seed: u64| {
            cuboid(
                [2.0, 1.0, 0.1],
                [0.0, 3.0, 0.0],
                vec![
                    cuboid([0.6, 2.0, 0.1], [0.5, -1.0, 0.0], vec![]),
                    block(seed),
                ],
            )
        };
        let buried = |seed: u64| {
            check(&generator(garage(seed)), &mut Budget::new(|| false))
                .pairs
                .is_empty()
        };
        let cover = (1..64).find(|&s| buried(s)).expect("a seed that covers");
        let away = (1..64).find(|&s| !buried(s)).expect("one that does not");

        let set = housed(garage(cover), &[None]);
        assert_eq!(fights(within(&empty(), &set)), Report::default(), "fixture");
        let seeded = within(&set, &housed(garage(cover), &[None, Some(away)]));
        assert_eq!(seeded.total, 1, "{seeded:?}");
        assert_eq!(
            seeded.named[0],
            json!({
                "a": "/generators/house",
                "b": "/generators/house/children/0",
                "area_m2": 0.6,
                "placement": "/placements/1",
            })
        );
    }

    /// #1505: out of time while it sets what a seed draws against what the
    /// generator draws itself, the check names no pair it had not decided -
    /// one not yet found to be the seed's alone may be the generator's -
    /// and lists the placement. Cut at every ask of the clock, what it names
    /// is some of what it names given all the time it wants, and it lists
    /// the placement whenever it answers less; the asks after the seed's
    /// own tree is checked are where that is decided.
    #[test]
    fn a_seeded_placement_cut_short_names_only_what_it_decided() {
        let (beside, pushed) = coin_seeds();
        let garage = garage_of_grammars(beside);
        let before = housed(garage.clone(), &[None]);
        let after = housed(garage, &[None, Some(pushed)]);
        let full = within(&before, &after);
        assert_eq!(full.total, 1, "fixture: {full:?}");

        let (budget, counted) = counting();
        report_within(&before, &after, budget);
        let asked = counted.get();
        let seeded = after.generators["house"].with_shape_seed(pushed);
        let checking_the_seed = asks(|budget| {
            check(&seeded, budget);
        });
        assert!(
            asked > checking_the_seed + 10,
            "fixture: {asked} asks, {checking_the_seed} to check the seed's tree"
        );
        for n in 1..=asked {
            let cut = report_within(&before, &after, spent_at(n));
            assert!(
                cut.named.iter().all(|pair| full.named.contains(pair)),
                "spent at ask {n}: {cut:?}"
            );
            if cut != full {
                assert_eq!(cut.unchecked, ["/placements/1"], "spent at ask {n}");
            }
        }
        assert_eq!(report_within(&before, &after, spent_at(asked + 1)), full);
    }
}
