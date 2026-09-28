//! CGA Shape Grammar generator pipeline: geometry + material caches, stable
//! content hashes that invalidate them, the `build_shape_geometry` worker,
//! and the `spawn_shape_entity` dispatcher used by the room compiler.
//!
//! Shape grammars are the architecture-shaped sibling of the L-system
//! generator: instead of a stack-based turtle, the upstream
//! [`symbios_shape::Interpreter`] expands a queue of named rules into a flat
//! list of [`Terminal`] panels carrying a face-profiled cuboid scope. We bake
//! one unit-sized procedural mesh per `(profile, size)` pair and cache the
//! resulting per-terminal spawn list, so a `Placement::Scatter` with
//! `count = 100_000` re-uses the same baked terminals across every cell
//! instead of re-deriving the grammar 100 000 times on the main thread.
//!
//! How a grammar is derived ([`ShapeDef::derive`]) and how each terminal is
//! meshed and placed ([`ShapeDef::bake`]) is written once, here, and read
//! by two callers: the spawner, and the agent's z-fighting check, which
//! reaches it through [`GeneratorKind::shape_def`] so that what it checks is
//! what the world draws (#1503).
//!
//! An absolute placement may carry a seed of its own (#1505), and then every
//! Shape node of the tree it plants derives with that seed instead of its
//! own ([`ShapeDef::seeded`]), so one generator stands in a street many
//! times with each copy drawing its own variety. The geometry cache keeps
//! one entry per node AND seed, so two copies drawn with different seeds
//! do not evict each other on every compile, and forgets a node's seeds no
//! placement draws any more when it next derives the node. It remembers a
//! seed with which the grammar draws nothing - up to
//! [`MAX_REMEMBERED_FAILURES`] of them in all - so that the node's grammar
//! status shows that error while the world draws the node with it,
//! whichever copy compiled last.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;
use bevy_symbios_shape::cache::{
    MeshCacheKey, ProfileKey, ShapeMeshCache as UpstreamShapeMeshCache,
};
use bevy_symbios_shape::{mesh::build_profiled_mesh_with, transform::scope_to_transform};
use symbios_shape::grammar::{Statement, parse_statement};
use symbios_shape::{
    FaceProfile, Interpreter, Quat as SQuat, Scope, ShapeModel, Terminal, Vec3 as SVec3,
};

use crate::pds::{Fp3, Generator, GeneratorKind, Placement, RoomRecord, SovereignMaterialSettings};

use super::RoomEntity;
use super::compile::{SpawnCtx, budget_exceeded};
use super::generator_cache::{CachedBuild, GeneratorCache, GeometryHasher, settings_fingerprint};
use super::material::spawn_procedural_material;

/// Persistent cross-compile cache for shape generator `StandardMaterial` handles.
///
/// Mirrors [`super::lsystem::LSystemMaterialCache`] - a `Placement::Scatter`
/// with `count=100` over a Shape generator would otherwise allocate 100 fresh
/// `StandardMaterial`s and enqueue 100 identical foliage texture tasks for
/// each `Mat("...")` slot. The cache keys on `(generator_ref, slot_name)` and
/// reuses the handle whenever [`settings_fingerprint`] of the
/// `SovereignMaterialSettings` is identical. GC + logout semantics come with
/// [`GeneratorCache`].
pub type ShapeMaterialCache = GeneratorCache<(String, String), Handle<StandardMaterial>>;

/// One pre-baked terminal: the world-relative transform produced by
/// `scope_to_transform`, the unit-sized procedural mesh handle (shared across
/// all terminals with the same `(profile, size)` triple), and the optional
/// material name emitted by `Mat("...")` in the grammar.
#[derive(Clone, Debug)]
pub struct ShapeInstance {
    pub transform: Transform,
    pub mesh: Handle<Mesh>,
    pub material_id: Option<String>,
}

/// Persistent cross-compile cache for shape grammar geometry - the
/// per-terminal spawn list. Materials are orthogonal: the per-instance
/// `material_id` is resolved against [`ShapeMaterialCache`] at spawn time.
///
/// Without this, a scatter placement with `count = 1000` referencing a
/// shape generator would re-parse every grammar line, re-seed the
/// interpreter, re-walk the derivation queue, and re-upload one fresh
/// `Handle<Mesh>` per terminal per scatter point on the main thread.
/// Because every scattered instance of the same generator shares an
/// identical model (only the parent transform varies), we derive,
/// interpret, and bake meshes **once** per `(generator_ref, geometry_hash)`
/// pair and reuse the resulting per-terminal handles across every spawn.
/// The list is an `Arc` so a cache HIT hands out an O(1) refcount bump
/// instead of deep-cloning the `Vec` (+ its per-instance `material_id`
/// Strings) on every scatter sample / grid cell (#636).
///
/// Keyed by the node's synthetic cache key AND the seed it derives with
/// (#1505): the node's own, or the seed of the absolute placement that
/// plants it. Keyed by the node alone, two placements of one generator with
/// seeds of their own would each find the other's derivation under the key,
/// miss on the hash and replace it, re-deriving both on every compile. The
/// end-of-job GC of a full compile retains the `(node, seed)` pairs the job
/// touched, so a variant no placement draws any more is dropped with it;
/// between full compiles, a miss forgets the node's variants no placement
/// of its root draws any more ([`Self::forget_undrawn`]), so a seed changed
/// through many values leaves no trail of them behind.
///
/// A variant whose grammar draws nothing is remembered too, by the same key
/// and hash, with its error: a node drawn with several seeds shows the
/// error of any of them the world still draws, whichever copy compiled
/// last ([`Self::failing_variant`]), so the other seeds' copies must know
/// of it. Held by node, so that a node that never failed is answered
/// without a look at any other's, and at most [`MAX_REMEMBERED_FAILURES`]
/// of them in all: a copy that draws nothing spawns nothing, so the room's
/// entity budget does not bound them. What a node that did fail answers is
/// kept until its failures or the placements change, so that a scatter of
/// it reads the placements once, not once for each copy.
#[derive(Resource, Default)]
pub struct ShapeMeshCache {
    /// Each variant's terminals, as it drew them: by node, then seed, with
    /// the geometry hash it drew them from.
    pub(super) builds: HashMap<String, HashMap<u64, Built>>,
    /// Each variant that drew nothing: by node, then seed, the geometry
    /// hash it failed at and its error as the editor's grammar forge shows
    /// it.
    pub(super) failures: HashMap<String, HashMap<u64, (u64, String)>>,
    /// How many variants `failures` holds, all nodes together.
    remembered_failures: usize,
    /// The seeds the room draws each generator's Shape nodes with, by the
    /// generator's name: read from the placements the first time after a
    /// plan that a node of it is derived while the cache holds it with
    /// another seed, and dropped by the next plan
    /// ([`Self::placements_changed`]), which every edit of the placements
    /// runs first. Read once, not once for each such node and copy: a
    /// world of a thousand seeded copies derives each node a thousand times.
    root_seeds: HashMap<String, Arc<RootSeeds>>,
    /// What [`Self::failing_variant`] answered for each node since its
    /// failures or the placements last changed: dropped for the node by
    /// every change of its failures, and for all of them by the next plan
    /// ([`Self::placements_changed`]).
    failing: HashMap<String, Failing>,
    /// How many remembered failures the copies that draw have had in view
    /// as they wrote their nodes' statuses ([`Self::failing_variant`]): for
    /// a test to see that a node looks at its own and no other's.
    #[cfg(test)]
    pub(super) failures_in_view: usize,
    /// How many placements [`Self::failing_variant`] has read to answer:
    /// for a test to see that a node's copies read them once between one
    /// change and the next.
    #[cfg(test)]
    pub(super) placements_in_view: usize,
}

/// What [`ShapeMeshCache::failing_variant`] answered for one node: the
/// generator it was asked under, the geometry hash of the node's own
/// settings it was asked with, and the answer.
struct Failing {
    root: String,
    own_hash: u64,
    error: Option<String>,
}

/// One variant's terminals, and the geometry hash it drew them from.
pub(super) type Built = CachedBuild<Arc<[ShapeInstance]>>;

/// At most this many variants that draw nothing are remembered (#1505):
/// one for every placement a room can hold. Past it a failure is kept
/// nowhere and derived again each time it is drawn, as every failure was
/// before seeds, and its node's status shows it only while it is the copy
/// compiled last.
pub(crate) const MAX_REMEMBERED_FAILURES: usize = crate::pds::sanitize::limits::MAX_PLACEMENTS;

/// The seeds a room draws one generator's Shape nodes with (#1505).
#[derive(Default)]
struct RootSeeds {
    /// Every seed an absolute placement of the generator gives it.
    given: HashSet<u64>,
    /// Whether a placement of it gives none - a scatter, a grid, or an
    /// absolute placement with no seed - and so draws each node with its
    /// own.
    unseeded: bool,
}

impl RootSeeds {
    /// The seeds `record`'s placements of the generator `root` draw it with.
    fn of(record: &RoomRecord, root: &str) -> Self {
        let mut seeds = Self::default();
        for placement in &record.placements {
            if placed_generator(placement) != Some(root) {
                continue;
            }
            match placement.shape_seed() {
                Some(seed) => {
                    seeds.given.insert(seed);
                }
                None => seeds.unseeded = true,
            }
        }
        seeds
    }

    /// Whether a node of the generator whose own seed is `own` is drawn
    /// with `seed`.
    fn draws(&self, own: u64, seed: u64) -> bool {
        self.given.contains(&seed) || (self.unseeded && seed == own)
    }
}

impl ShapeMeshCache {
    /// Drop every cached build and remembered failure, and the handles the
    /// builds pin. Called on logout so one session's builds don't outlive
    /// it (#625).
    pub fn clear(&mut self) {
        self.builds.clear();
        self.failures.clear();
        self.remembered_failures = 0;
        self.root_seeds.clear();
        self.failing.clear();
    }

    /// Keep only the variants in `touched`: the end-of-job GC of a full
    /// compile, whose touch-set holds every variant the world draws.
    pub(crate) fn retain(&mut self, touched: &HashSet<(String, u64)>) {
        self.builds.retain(|node, seeds| {
            seeds.retain(|seed, _| touched.contains(&(node.clone(), *seed)));
            !seeds.is_empty()
        });
        self.failures.retain(|node, seeds| {
            seeds.retain(|seed, _| touched.contains(&(node.clone(), *seed)));
            !seeds.is_empty()
        });
        self.remembered_failures = self.failures.values().map(HashMap::len).sum();
        self.failing.clear();
    }

    /// The room's placements are planned anew: the seeds each generator is
    /// drawn with, and what a node's failing seeds are, are read from them
    /// again when next asked for.
    pub(crate) fn placements_changed(&mut self) {
        self.root_seeds.clear();
        self.failing.clear();
    }

    /// `node`'s remembered failures changed: what it answered before is
    /// read again when next asked for.
    fn failures_changed(&mut self, node: &str) {
        self.failing.remove(node);
    }

    /// The seeds `record` draws the Shape nodes of the generator `root`
    /// with, as read since the last plan.
    fn seeds_of(&mut self, record: &RoomRecord, root: &str) -> Arc<RootSeeds> {
        if let Some(seeds) = self.root_seeds.get(root) {
            return Arc::clone(seeds);
        }
        let seeds = Arc::new(RootSeeds::of(record, root));
        self.root_seeds.insert(root.to_owned(), Arc::clone(&seeds));
        seeds
    }

    /// Whether a variant of `node` is held with a seed other than `seed`.
    fn holds_another(&self, node: &str, seed: u64) -> bool {
        fn another<V>(seeds: &HashMap<u64, V>, seed: u64) -> bool {
            seeds.len() > usize::from(seeds.contains_key(&seed))
        }
        self.builds
            .get(node)
            .is_some_and(|seeds| another(seeds, seed))
            || self
                .failures
                .get(node)
                .is_some_and(|seeds| another(seeds, seed))
    }

    /// Drop the variants of `node` whose seed `drawn` says the world no
    /// longer draws it with (#1505). The cache only holds a second copy of
    /// each build's handles - every spawned copy holds its own - so a
    /// variant forgotten while drawn would cost a derivation, never a hole
    /// in the world.
    fn forget_undrawn(&mut self, node: &str, drawn: impl Fn(u64) -> bool) {
        if let Some(seeds) = self.builds.get_mut(node) {
            seeds.retain(|seed, _| drawn(*seed));
            if seeds.is_empty() {
                self.builds.remove(node);
            }
        }
        if let Some(seeds) = self.failures.get_mut(node) {
            let held = seeds.len();
            seeds.retain(|seed, _| drawn(*seed));
            let forgotten = held - seeds.len();
            if seeds.is_empty() {
                self.failures.remove(node);
            }
            if forgotten > 0 {
                self.remembered_failures -= forgotten;
                self.failures_changed(node);
            }
        }
    }

    /// The error of a seed `record` draws the Shape node `node` of the
    /// generator `root` with, when one of them draws nothing (#1505): what a
    /// copy of the node that draws writes as its grammar status in place of
    /// Ok. The seeds are, for each placement of `root`, the placement's own
    /// grammar seed or, under a scatter, a grid or an absolute placement
    /// with none, the node's own (`own`); each is read as it drew from the
    /// node's settings as they are now, so that a seed no placement draws
    /// any more, or a failure an edit has fixed since, is none. The first
    /// in the order of the placements. `drawn` is the seed and geometry hash
    /// of the copy that asks, whose settings are the node's own unless its
    /// placement's seed replaced the node's.
    ///
    /// A node that never failed is answered at once, without a look at any
    /// other node's failures. One that did reads the placements, and gives
    /// the answer it read again until its failures or the placements
    /// change: a scatter of it would read them once for every copy.
    fn failing_variant(
        &mut self,
        record: &RoomRecord,
        root: &str,
        node: &str,
        own: ShapeDef<'_>,
        drawn: (u64, u64),
    ) -> Option<String> {
        let failing = self.failures.get(node)?;
        #[cfg(test)]
        {
            self.failures_in_view += failing.len();
        }
        let own_hash = match drawn {
            (seed, hash) if seed == own.seed => hash,
            _ => shape_geometry_fingerprint(&own),
        };
        if let Some(known) = self.failing.get(node)
            && known.root == root
            && known.own_hash == own_hash
        {
            return known.error.clone();
        }
        #[cfg(test)]
        let mut read = 0;
        let error = record
            .placements
            .iter()
            .enumerate()
            .find_map(|(index, placement)| {
                #[cfg(test)]
                {
                    read += 1;
                }
                if placed_generator(placement) != Some(root) {
                    return None;
                }
                let seed = placement.shape_seed();
                let def = own.seeded(seed);
                let (hash, message) = failing.get(&def.seed)?;
                let drew_from = if def.seed == own.seed {
                    own_hash
                } else {
                    shape_geometry_fingerprint(&def)
                };
                if *hash != drew_from {
                    return None;
                }
                Some(match seed {
                    Some(seed) => format!("{}: {message}", seeded_by(index, seed)),
                    None => message.clone(),
                })
            });
        #[cfg(test)]
        {
            self.placements_in_view += read;
        }
        self.failing.insert(
            node.to_owned(),
            Failing {
                root: root.to_owned(),
                own_hash,
                error: error.clone(),
            },
        );
        error
    }

    /// The grammar status of each Shape node of the generator `root` that
    /// is remembered to draw nothing with some seed, by the node's key: what
    /// a copy of it that draws writes with `record`'s placements
    /// ([`Self::failing_variant`]) (#1505). For a plan that finds a
    /// placement pointed away from `root`, which a status may name, to write
    /// them anew without building a copy of `root` - whose first copy left
    /// may be a scatter of thousands. A node with no failure remembered has
    /// no seed of a placement to name, and is left out.
    pub(crate) fn statuses_under(
        &mut self,
        record: &RoomRecord,
        root: &str,
    ) -> Vec<(String, Option<String>)> {
        let mut statuses = Vec::new();
        let Some(generator) = record.generators.get(root) else {
            return statuses;
        };
        let mut nodes: Vec<(&Generator, Vec<usize>)> = vec![(generator, Vec::new())];
        while let Some((node, path)) = nodes.pop() {
            nodes.extend(node.children.iter().enumerate().map(|(i, child)| {
                let mut below = path.clone();
                below.push(i);
                (child, below)
            }));
            let Some(own) = node.kind.shape_def() else {
                continue;
            };
            let key = super::compile::synthetic_cache_key(root, &path);
            if !self.failures.contains_key(&key) {
                continue;
            }
            let own_hash = shape_geometry_fingerprint(&own);
            let error = self.failing_variant(record, root, &key, own, (own.seed, own_hash));
            statuses.push((key, error));
        }
        statuses
    }

    /// What the variant `key` drew from the settings hashed `geometry_hash`,
    /// when it was drawn from them before: its terminals, or its error.
    fn drawn(&self, key: &(String, u64), geometry_hash: u64) -> Option<Drawn> {
        let (node, seed) = key;
        if let Some(built) = self.builds.get(node).and_then(|seeds| seeds.get(seed))
            && built.fingerprint == geometry_hash
        {
            return Some(Ok(built.value.clone()));
        }
        match self.failures.get(node).and_then(|seeds| seeds.get(seed)) {
            Some((hash, message)) if *hash == geometry_hash => Some(Err(message.clone())),
            _ => None,
        }
    }

    /// Remember what the variant `key` drew from the settings hashed
    /// `geometry_hash`, in place of anything it drew before; whether it is
    /// held now. A variant that drew nothing is not, once
    /// [`MAX_REMEMBERED_FAILURES`] others are.
    pub(super) fn remember(
        &mut self,
        key: (String, u64),
        geometry_hash: u64,
        drawn: &Drawn,
    ) -> bool {
        let (node, seed) = key;
        match drawn {
            Ok(built) => {
                if let Some(seeds) = self.failures.get_mut(&node)
                    && seeds.remove(&seed).is_some()
                {
                    self.remembered_failures -= 1;
                    if seeds.is_empty() {
                        self.failures.remove(&node);
                    }
                    self.failures_changed(&node);
                }
                self.builds.entry(node).or_default().insert(
                    seed,
                    CachedBuild {
                        fingerprint: geometry_hash,
                        value: built.clone(),
                    },
                );
                true
            }
            Err(message) => {
                if let Some(seeds) = self.builds.get_mut(&node) {
                    seeds.remove(&seed);
                    if seeds.is_empty() {
                        self.builds.remove(&node);
                    }
                }
                let held = self
                    .failures
                    .get(&node)
                    .is_some_and(|seeds| seeds.contains_key(&seed));
                if !held {
                    if self.remembered_failures >= MAX_REMEMBERED_FAILURES {
                        return false;
                    }
                    self.remembered_failures += 1;
                }
                self.failures_changed(&node);
                self.failures
                    .entry(node)
                    .or_default()
                    .insert(seed, (geometry_hash, message.clone()));
                true
            }
        }
    }
}

/// What one variant of a Shape node draws: its terminals, or the grammar's
/// error.
pub(super) type Drawn = Result<Arc<[ShapeInstance]>, String>;

/// The `GeneratorKind::Shape` payload, borrowed for the duration of one
/// build. Grouping it keeps the derivation entry points to a handful of
/// arguments now that the material map is a geometry input too (#939) -
/// these five always travel together and always come from the same node.
#[derive(Clone, Copy)]
pub(crate) struct ShapeDef<'a> {
    grammar_source: &'a str,
    root_rule: &'a str,
    footprint: Fp3,
    seed: u64,
    materials: &'a HashMap<String, SovereignMaterialSettings>,
    round_meshes: &'a [String],
}

impl GeneratorKind {
    /// This node's grammar, borrowed, when it is a [`GeneratorKind::Shape`].
    ///
    /// Written here rather than beside the enum because this module is
    /// private to the world builder: this is the one door into it, the one
    /// the spawner uses too, so a reader outside - the agent's z-fighting
    /// check (#1503) - derives and meshes a node's terminals exactly as the
    /// world does rather than through a copy that could drift.
    pub(crate) fn shape_def(&self) -> Option<ShapeDef<'_>> {
        let GeneratorKind::Shape {
            grammar_source,
            root_rule,
            footprint,
            seed,
            materials,
            round_meshes,
        } = self
        else {
            return None;
        };
        Some(ShapeDef {
            grammar_source,
            root_rule,
            footprint: *footprint,
            seed: *seed,
            materials,
            round_meshes,
        })
    }
}

/// A grammar that gave the world nothing to draw (#829).
pub(crate) struct Underived {
    /// What the editor's grammar forge shows, line-numbered where the
    /// parser knows the line.
    message: String,
    /// Whether the spawner logs it. A fault in the grammar's text or in its
    /// derivation is logged; a grammar with no rules, or with rules that
    /// emit no terminal, is not, as it never was.
    logged: bool,
}

impl Underived {
    fn fault(message: String) -> Self {
        Self {
            message,
            logged: true,
        }
    }

    fn quiet(message: &str) -> Self {
        Self {
            message: message.to_string(),
            logged: false,
        }
    }
}

impl ShapeDef<'_> {
    /// This grammar as a placement with `seed` of its own draws it (#1505):
    /// `seed` in place of the node's own when there is one - REPLACED, not
    /// mixed in, so a placement whose seed is the node's own draws exactly
    /// what an unseeded placement draws - and the node's own when there is
    /// none.
    fn seeded(self, seed: Option<u64>) -> Self {
        Self {
            seed: seed.unwrap_or(self.seed),
            ..self
        }
    }

    /// Parse the multi-line grammar source line by line, populate the
    /// interpreter and derive the model from the node's footprint with the
    /// node's seed. `Err` on a parse or derive failure or an empty model.
    ///
    /// Mirrors the line-based authoring convention used by sibling editors
    /// (`symbios-ground-lab`): one *statement* per line, blank lines and
    /// `// …` lines ignored. A line that does not parse aborts the whole
    /// derivation - partial rule tables produce confusing terminal layouts
    /// that look like silent bugs in the grammar.
    ///
    /// Since symbios-shape 0.3 a line may also be an `attr` / `const` /
    /// `style` declaration, so a room's grammar can expose its own knobs and
    /// prosperity registers rather than hard-coding every dimension.
    pub(crate) fn derive(&self) -> Result<ShapeModel, Underived> {
        let mut interpreter = Interpreter::new();
        interpreter.seed = self.seed;

        let mut rule_count: u32 = 0;
        for (i, raw) in self.grammar_source.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            match parse_statement(line) {
                Ok(statement) => {
                    // Only productions count toward "has rules" - a grammar of
                    // pure declarations derives nothing.
                    let is_rule = matches!(statement, Statement::Rule(_));
                    if let Err(e) = interpreter.add_statement(statement) {
                        return Err(Underived::fault(format!("line {}: {}", i + 1, e)));
                    }
                    if is_rule {
                        rule_count += 1;
                    }
                }
                Err(e) => return Err(Underived::fault(format!("line {}: {}", i + 1, e))),
            }
        }

        if rule_count == 0 {
            return Err(Underived::quiet("grammar has no rules"));
        }
        if !interpreter.has_rule(self.root_rule) {
            return Err(Underived::fault(format!(
                "root rule `{}` not defined in grammar",
                self.root_rule
            )));
        }

        let root_scope = Scope::new(
            SVec3::ZERO,
            SQuat::IDENTITY,
            SVec3::new(
                self.footprint.0[0] as f64,
                self.footprint.0[1] as f64,
                self.footprint.0[2] as f64,
            ),
        );

        let model = interpreter
            .derive(root_scope, self.root_rule)
            .map_err(|e| Underived::fault(format!("derivation error: {}", e)))?;

        if model.terminals.is_empty() {
            return Err(Underived::quiet(
                "grammar produced no geometry (no terminal shapes)",
            ));
        }
        Ok(model)
    }

    /// How the world draws `terminal`, one of this grammar's derived
    /// terminals: its profile, extents and UV and roundness choices, which
    /// make its unit mesh, and the transform that places that mesh in the
    /// node's frame.
    pub(crate) fn bake<'t>(&self, terminal: &'t Terminal) -> TerminalBake<'t> {
        let size = Vec3::new(
            terminal.scope.size.x as f32,
            terminal.scope.size.y as f32,
            terminal.scope.size.z as f32,
        );
        // Alpha cards must span their face exactly once; every other surface
        // tiles in world space (#939). The shape mesher's `stretch_uvs` is
        // the grammar-side equivalent of `UvMapping::Fit` on a prim `Plane`,
        // and without it a `Window` card on a 4 m wall repeats four times
        // instead of glazing it. Derived from the material rather than
        // registered by name so it cannot drift from the texture's own
        // clamp-vs-repeat sampling.
        let stretch_uvs = terminal
            .material
            .as_ref()
            .and_then(|m| self.materials.get(&m.id))
            .is_some_and(|s| s.texture.is_card());
        // Turned terminals (columns, silos, spires) bake as elliptical
        // prisms; the grammar still derived them as boxes, so splits and
        // occlusion are unaffected. Keyed on the mesh id emitted by
        // `I("...")` - a colonnade's shafts and its flat entablature
        // usually share one stone material.
        let round_segments = if self.round_meshes.contains(&terminal.mesh_id) {
            ROUND_SEGMENTS
        } else {
            0
        };
        TerminalBake {
            profile: &terminal.face_profile,
            transform: scope_to_transform(&terminal.scope),
            size,
            stretch_uvs,
            round_segments,
        }
    }
}

/// One derived terminal as the world draws it: a unit mesh, and the
/// transform that stretches and places it.
pub(crate) struct TerminalBake<'t> {
    profile: &'t FaceProfile,
    /// Where the terminal's unit mesh sits in its node's frame: the scope's
    /// centre, turn and extents. A negative extent keeps its sign and
    /// mirrors the mesh.
    pub(crate) transform: Transform,
    size: Vec3,
    stretch_uvs: bool,
    round_segments: u32,
}

impl TerminalBake<'_> {
    /// The key the spawner shares this terminal's mesh under: two terminals
    /// with the same key draw one mesh.
    pub(crate) fn cache_key(&self) -> MeshCacheKey {
        MeshCacheKey {
            profile: ProfileKey::from_profile(self.profile),
            size_x_bits: self.size.x.to_bits(),
            size_y_bits: self.size.y.to_bits(),
            size_z_bits: self.size.z.to_bits(),
            stretch_uvs: self.stretch_uvs,
            round_segments: self.round_segments,
        }
    }

    /// The terminal's unit mesh, as the spawner uploads it.
    pub(crate) fn mesh(&self) -> Mesh {
        build_profiled_mesh_with(
            self.profile,
            self.size,
            self.stretch_uvs,
            self.round_segments,
        )
    }
}

#[cfg(all(test, unix))]
impl ShapeDef<'_> {
    /// Each terminal as the spawner builds it, in its order, through
    /// [`build_shape_geometry`] with a fresh mesh cache and asset store: the
    /// transform it is spawned with under its node, and its mesh's vertex
    /// positions. For tests that hold another reader of a grammar - the
    /// agent's z-fighting check - to what the world draws.
    pub(crate) fn spawned(&self) -> Result<Vec<(Transform, Vec<Vec3>)>, String> {
        let mut meshes = Assets::<Mesh>::default();
        let mut cache = UpstreamShapeMeshCache::default();
        let built = build_shape_geometry(self, "spawned", &mut meshes, &mut cache)?;
        Ok(built
            .iter()
            .map(|instance| {
                let mesh = meshes
                    .get(&instance.mesh)
                    .expect("the spawner's mesh handle resolves");
                let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("a terminal's mesh has positions");
                };
                let positions = positions.iter().map(|p| Vec3::from_array(*p)).collect();
                (instance.transform, positions)
            })
            .collect())
    }
}

/// Radial tessellation for round terminals, matching the upstream plugin's
/// default. Fine enough that a column reads as turned at walking distance
/// without multiplying the vertex count of a dense colonnade.
const ROUND_SEGMENTS: u32 = 24;

/// Stable content hash of the geometry-affecting fields of a
/// `GeneratorKind::Shape`. Material *settings* are deliberately excluded
/// because those are applied per-spawn on top of a shared mesh list (see
/// [`ShapeMaterialCache`]) - but which slots are alpha cards is not a
/// setting, it is a geometry input, so that much is folded in (#939). Each
/// `Fp3` axis is hashed via its fixed-point wire form so NaN/denormal
/// floats can't destabilise the key across compile passes.
fn shape_geometry_fingerprint(def: &ShapeDef<'_>) -> u64 {
    let mut h = GeometryHasher::new();
    h.field(def.grammar_source);
    h.field(def.root_rule);
    h.field(def.seed);
    h.fp(def.footprint.0[0]);
    h.fp(def.footprint.0[1]);
    h.fp(def.footprint.0[2]);
    // Which slots are alpha cards *is* a geometry input (#939): a card's
    // face meshes with UVs stretched into 0..1 instead of tiled in world
    // space. Only the card-ness enters the hash, not the settings - colour
    // and roughness edits must still reuse the baked mesh list. Sorted so
    // `HashMap` iteration order can't destabilise the key across compiles.
    let mut cards: Vec<&str> = def
        .materials
        .iter()
        .filter(|(_, s)| s.texture.is_card())
        .map(|(name, _)| name.as_str())
        .collect();
    cards.sort_unstable();
    for name in cards {
        h.field(name);
    }
    // Which terminals are turned is a geometry input too - a round shaft
    // and a square pier of identical size are different meshes. Sorted so
    // a re-ordered list reuses the same bake.
    let mut round: Vec<&str> = def.round_meshes.iter().map(String::as_str).collect();
    round.sort_unstable();
    for name in round {
        h.field(name);
    }
    h.finish()
}

// Mesh dedup keys (`MeshCacheKey`, `ProfileKey`) and the cross-spawn
// [`UpstreamShapeMeshCache`] resource live in `bevy_symbios_shape::cache` -
// importing them at the top of this module keeps mesh handle reuse
// consistent across every consumer of the shape grammar.

/// Derive the grammar ([`ShapeDef::derive`]) and return a flat list of
/// [`ShapeInstance`]s ready to spawn, each terminal meshed and placed by
/// [`ShapeDef::bake`]. `Err` (the grammar error, line-numbered where the
/// parser knows one) on parse / derive failure or empty output so the
/// caller can skip the spawn and surface the message in the editor (#829);
/// a fault in the grammar's text or its derivation is also `warn!`-logged,
/// naming the node as `label` does.
fn build_shape_geometry(
    def: &ShapeDef<'_>,
    label: &str,
    meshes: &mut Assets<Mesh>,
    upstream_cache: &mut UpstreamShapeMeshCache,
) -> Result<Vec<ShapeInstance>, String> {
    let model = def.derive().map_err(|underived| {
        if underived.logged {
            warn!("Shape `{}` {}", label, underived.message);
        }
        underived.message
    })?;

    // Dedupe meshes through the upstream `ShapeMeshCache` resource so two
    // different generators that produce terminals with the same
    // `(profile, size)` triple share the same `Handle<Mesh>` - a 1000-window
    // facade allocates one window mesh, not 1000, AND a second building
    // generator with the same window pattern reuses the existing handle
    // instead of uploading a duplicate.
    let mut instances = Vec::with_capacity(model.terminals.len());
    for terminal in &model.terminals {
        let bake = def.bake(terminal);
        let mesh = upstream_cache.get_or_insert_with(bake.cache_key(), || meshes.add(bake.mesh()));
        instances.push(ShapeInstance {
            transform: bake.transform,
            mesh,
            material_id: terminal.material.as_ref().map(|m| m.id.clone()),
        });
    }

    Ok(instances)
}

/// Resolve (and cache) a [`StandardMaterial`] handle for a given material
/// slot name. A `None` slot or a slot that has no entry in the generator's
/// `materials` map both fall through to a shared default handle keyed by
/// the sentinel `""` slot name, so 1000 unmapped terminals share one
/// fallback material instead of allocating 1000.
fn resolve_material_handle(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    generator_ref: &str,
    materials: &HashMap<String, SovereignMaterialSettings>,
    slot_name: Option<&str>,
) -> Handle<StandardMaterial> {
    const FALLBACK_SENTINEL_HASH: u64 = u64::MAX;
    let lookup = slot_name.and_then(|n| materials.get(n).map(|s| (n.to_string(), s)));
    match lookup {
        Some((name, settings)) => {
            let key = (generator_ref.to_string(), name);
            let hash = settings_fingerprint(settings);
            ctx.shape_material_touched.insert(key.clone());
            match ctx.shape_material_cache.get_if(&key, hash) {
                Some(handle) => handle,
                None => {
                    let handle = spawn_procedural_material(ctx, settings);
                    ctx.shape_material_cache.insert(key, hash, handle.clone());
                    handle
                }
            }
        }
        None => {
            // Use the empty slot name as a stable cache key for the shared
            // fallback. Without this, every unmapped terminal in a 100k
            // scatter allocates its own `StandardMaterial::default()`.
            let key = (generator_ref.to_string(), String::new());
            ctx.shape_material_touched.insert(key.clone());
            match ctx
                .shape_material_cache
                .get_if(&key, FALLBACK_SENTINEL_HASH)
            {
                Some(handle) => handle,
                None => {
                    let h = ctx.std_materials.add(StandardMaterial::default());
                    ctx.shape_material_cache
                        .insert(key, FALLBACK_SENTINEL_HASH, h.clone());
                    h
                }
            }
        }
    }
}

pub(super) fn spawn_shape_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    kind: &GeneratorKind,
    generator_ref: &str,
    // This node's path under its root, so its grammar status files under
    // the node rather than the tree (#1250 f84).
    path: &[usize],
    transform: Transform,
) -> Option<Entity> {
    let own = kind.shape_def()?;
    // The seed of the absolute placement being compiled, when it names one,
    // replaces the node's own (#1505).
    let placement_seed = ctx.placement_shape_seed();
    let def = own.seeded(placement_seed);

    // Reuse cached geometry when the geometry-affecting settings are
    // unchanged. A scatter placement with count=1000 would otherwise
    // re-derive the grammar and re-bake every terminal mesh on every spawn.
    // One entry per node and seed, so each placement's variant keeps its
    // own (#1505).
    let key = (generator_ref.to_string(), def.seed);
    let geometry_hash = shape_geometry_fingerprint(&def);
    let (drawn, held) = match ctx.shape_mesh_cache.drawn(&key, geometry_hash) {
        // Drawn from these very settings before: derived cleanly then, or
        // drew nothing, as it would again.
        Some(drawn) => (drawn, true),
        None => {
            // The node's variants the world no longer draws go first, so a
            // seed changed through many values leaves none of them behind
            // until a full compile (#1505).
            if ctx.shape_mesh_cache.holds_another(&key.0, key.1) {
                forget_undrawn(ctx, &key.0, own.seed);
            }
            // A variant's failure names the placement and the seed it was
            // drawn with, or a grammar that derives with its own seed would
            // show an error nothing in it explains.
            let label = match placement_seed {
                Some(seed) => format!("{generator_ref} ({})", seeded_by(ctx.placement_index, seed)),
                None => generator_ref.to_string(),
            };
            let drawn: Drawn =
                build_shape_geometry(&def, &label, ctx.meshes, ctx.upstream_shape_mesh_cache)
                    .map(Arc::from);
            // A grammar rejected, a root rule missing or an empty model is
            // remembered as well, under the hash it failed at: an edit that
            // fixes it changes the hash, so it is derived afresh rather
            // than a stale result reused (#829), and until then every copy
            // of the node knows this seed fails (`failing_variant`) - up to
            // `MAX_REMEMBERED_FAILURES` of them.
            let held = ctx
                .shape_mesh_cache
                .remember(key.clone(), geometry_hash, &drawn);
            (drawn, held)
        }
    };
    // What the end-of-job GC keeps: every variant the cache holds that this
    // job drew, and no other, so the set is bounded as the cache is.
    if held && !ctx.shape_mesh_touched.contains(&key) {
        ctx.shape_mesh_touched.insert(key);
    }

    // The node's grammar status, which the editor's grammar forge shows
    // (#829), covers every seed the world draws the node with (#1505). A
    // copy that draws records Ok - so a fixed-then-unchanged grammar leaves
    // no stale error behind - unless another seed the world draws it with
    // fails, whose error it records instead: whichever copy compiled last,
    // the status shows a failure as long as the world draws it.
    let error = match &drawn {
        Err(message) => Some(match placement_seed {
            Some(seed) => format!("{}: {message}", seeded_by(ctx.placement_index, seed)),
            None => message.clone(),
        }),
        Ok(_) => failing_variant(ctx, generator_ref, own, (def.seed, geometry_hash)),
    };
    ctx.record_grammar_status(generator_ref, path, error);
    let instances = drawn.ok()?;

    // Parent every terminal under a single transform so the placement's
    // rotation/position anchors the whole building as a unit. Avatar
    // mode skips the `RoomEntity` tag for the same reason as the
    // lsystem spawner - see `world_builder::lsystem::spawn_lsystem_entity`.
    let parent = if ctx.avatar_mode {
        ctx.commands.spawn((transform, Visibility::default())).id()
    } else {
        ctx.commands
            .spawn((
                transform,
                Visibility::default(),
                RoomEntity,
                super::PlacementUnit(ctx.placement_index),
            ))
            .id()
    };

    // Each terminal is a real ECS entity, so it contributes to the
    // room-wide spawn budget. Without this, a record can put a high-count
    // `Scatter` over a Shape grammar that derives thousands of terminals
    // - the per-generator-node accounting in `spawn_generator` would only
    // charge one per scatter point regardless of terminal count, blowing
    // past `MAX_ROOM_ENTITIES` and OOMing the ECS.
    for instance in instances.iter() {
        if budget_exceeded(*ctx.entities_spawned, ctx.budget_warned) {
            break;
        }
        let material = resolve_material_handle(
            ctx,
            generator_ref,
            def.materials,
            instance.material_id.as_deref(),
        );
        // NB: no `RoomEntity` marker on child meshes - see the lsystem
        // spawner for the rationale (recursive despawn from the parent
        // covers them; double-marking cascades into "entity despawned"
        // warnings during room rebuilds).
        let child = ctx
            .commands
            .spawn((
                Mesh3d(instance.mesh.clone()),
                MeshMaterial3d(material),
                instance.transform,
            ))
            .id();
        ctx.commands.entity(parent).add_child(child);
        // A scattered copy's terminal takes its draw distance (#1480).
        ctx.note_part(child, &instance.mesh, &instance.transform);
        *ctx.entities_spawned = ctx.entities_spawned.saturating_add(1);
    }

    Some(parent)
}

/// How a grammar error names the copy that drew it (#1505): by its
/// placement's index and the seed that placement gave it.
fn seeded_by(placement: usize, seed: u64) -> String {
    format!("with placement #{placement}'s seed {seed}")
}

/// Forget the variants of the Shape node `node`, whose own seed is `own`,
/// that the world no longer draws (#1505): an avatar draws the node with
/// its own seed alone, and a room with the seed each placement of the
/// node's root gives it, or its own where a placement gives none.
fn forget_undrawn(ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>, node: &str, own: u64) {
    if ctx.avatar_mode {
        ctx.shape_mesh_cache
            .forget_undrawn(node, |seed| seed == own);
        return;
    }
    let Some(root) = ctx
        .record
        .placements
        .get(ctx.placement_index)
        .and_then(placed_generator)
    else {
        return;
    };
    let seeds = ctx.shape_mesh_cache.seeds_of(ctx.record, root);
    ctx.shape_mesh_cache
        .forget_undrawn(node, |seed| seeds.draws(own, seed));
}

/// The generator `placement` places.
fn placed_generator(placement: &Placement) -> Option<&str> {
    match placement {
        Placement::Absolute { generator_ref, .. }
        | Placement::Scatter { generator_ref, .. }
        | Placement::Grid { generator_ref, .. } => Some(generator_ref),
        Placement::Unknown => None,
    }
}

/// [`ShapeMeshCache::failing_variant`] for a copy of the Shape node `node`
/// that draws, spawned with the seed and geometry hash `drawn`; `own` is the
/// node as its own settings draw it. Its root is the generator the unit
/// being compiled places. None for an avatar, which draws the node's own
/// seed alone.
fn failing_variant(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    node: &str,
    own: ShapeDef<'_>,
    drawn: (u64, u64),
) -> Option<String> {
    if ctx.avatar_mode {
        return None;
    }
    let record = ctx.record;
    let root = record
        .placements
        .get(ctx.placement_index)
        .and_then(placed_generator)?;
    ctx.shape_mesh_cache
        .failing_variant(record, root, node, own, drawn)
}

#[cfg(test)]
mod grammar_error_tests {
    use super::*;

    /// #829: shape-grammar failures surface as `Err` with the message the
    /// editor forge renders - line-numbered parse errors, a named missing
    /// root rule, and the no-rules case.
    #[test]
    fn shape_grammar_errors_surface_as_results() {
        // A bare asset store suffices - the error paths never reach the
        // mesh-baking stage that would populate it.
        let mut meshes = Assets::<Mesh>::default();
        let mut cache = UpstreamShapeMeshCache::default();

        let err = build_shape_geometry(
            &ShapeDef {
                grammar_source: "",
                root_rule: "Root",
                footprint: Fp3([8.0, 8.0, 8.0]),
                seed: 1,
                materials: &HashMap::new(),
                round_meshes: &[],
            },
            "test_gen",
            &mut meshes,
            &mut cache,
        )
        .expect_err("empty grammar must be rejected");
        assert!(err.contains("no rules"), "{err}");

        let err = build_shape_geometry(
            &ShapeDef {
                grammar_source: "House --> Extrude(10) Body",
                root_rule: "Root",
                footprint: Fp3([8.0, 8.0, 8.0]),
                seed: 1,
                materials: &HashMap::new(),
                round_meshes: &[],
            },
            "test_gen",
            &mut meshes,
            &mut cache,
        )
        .expect_err("missing root rule must be rejected");
        assert!(err.contains("root rule `Root`"), "{err}");

        let err = build_shape_geometry(
            &ShapeDef {
                grammar_source: "%%% not a rule at all",
                root_rule: "Root",
                footprint: Fp3([8.0, 8.0, 8.0]),
                seed: 1,
                materials: &HashMap::new(),
                round_meshes: &[],
            },
            "test_gen",
            &mut meshes,
            &mut cache,
        )
        .expect_err("parse failure must be rejected");
        assert!(err.contains("line 1"), "{err}");
    }
}

#[cfg(test)]
mod failure_memo_tests {
    use super::*;

    /// #1505: at most [`MAX_REMEMBERED_FAILURES`] variants that draw
    /// nothing are remembered, whichever nodes they are of - one past it is
    /// refused - while one already remembered is still written afresh, and
    /// a failure forgotten, because its variant drew, makes room again.
    #[test]
    fn at_most_so_many_failures_are_remembered() {
        let mut cache = ShapeMeshCache::default();
        let failed: Drawn = Err("grammar produced no geometry (no terminal shapes)".to_owned());
        let cap = MAX_REMEMBERED_FAILURES as u64;
        for seed in 0..=cap {
            let node = format!("g/{}", seed % 3);
            assert_eq!(
                cache.remember((node, seed), 1, &failed),
                seed < cap,
                "seed {seed}"
            );
        }
        let held =
            |cache: &ShapeMeshCache| -> usize { cache.failures.values().map(HashMap::len).sum() };
        assert_eq!(held(&cache), MAX_REMEMBERED_FAILURES);
        assert!(
            cache.remember(("g/0".to_owned(), 0), 2, &failed),
            "one already remembered, written afresh"
        );
        assert!(
            !cache.remember(("g/9".to_owned(), 0), 1, &failed),
            "still full"
        );
        let drew: Drawn = Ok(Arc::from(Vec::new()));
        assert!(cache.remember(("g/0".to_owned(), 0), 3, &drew));
        assert_eq!(held(&cache), MAX_REMEMBERED_FAILURES - 1);
        assert!(
            cache.remember(("g/9".to_owned(), 0), 1, &failed),
            "room again"
        );
        assert_eq!(held(&cache), MAX_REMEMBERED_FAILURES);
    }

    /// #1505: what a node answered is its answer for the settings and the
    /// generator it was asked with, and no other. The node `n` is
    /// remembered to draw nothing with the seed 5 that two placements give
    /// it, one of the generator `a` and one of `b`. Asked under `a` with the
    /// settings that failed, it names `a`'s placement; asked with a grammar
    /// edited since, no seed of it is known to fail; asked under `b`, it
    /// names `b`'s.
    #[test]
    fn a_node_s_answer_holds_for_its_settings_and_generator_alone() {
        let shape = |grammar: &str| -> Generator {
            serde_json::from_value(serde_json::json!({
                "$type": "network.symbios.gen.shape",
                "grammar_source": grammar,
                "root_rule": "Lot",
                "footprint": [20_000, 0, 20_000],
                "seed": "1",
            }))
            .expect("a Shape node")
        };
        let placed = |generator: &str| Placement::Absolute {
            generator_ref: generator.to_owned(),
            transform: crate::pds::TransformData::default(),
            snap_to_terrain: false,
            avoid_water: false,
            avoid_water_clearance: crate::pds::Fp(0.0),
            seed: Some(5),
        };
        let record = RoomRecord {
            lex_type: "network.symbios.room".to_owned(),
            environment: Default::default(),
            generators: HashMap::new(),
            placements: vec![placed("a"), placed("b")],
            traits: HashMap::new(),
            contact_effects: Default::default(),
            default_landing: None,
            opaque_refs: Default::default(),
        };
        let failing = shape("Lot --> NIL");
        let edited = shape("Lot --> NIL\n// edited");
        let mut cache = ShapeMeshCache::default();
        let failed: Drawn = Err("grammar produced no geometry (no terminal shapes)".to_owned());
        let def = failing.kind.shape_def().expect("a Shape node");
        let hash = shape_geometry_fingerprint(&def.seeded(Some(5)));
        assert!(cache.remember(("n".to_owned(), 5), hash, &failed));
        let mut ask = |root: &str, node: &Generator| {
            let own = node.kind.shape_def().expect("a Shape node");
            let own_hash = shape_geometry_fingerprint(&own);
            cache.failing_variant(&record, root, "n", own, (own.seed, own_hash))
        };
        let named = |at: usize| {
            Some(format!(
                "with placement #{at}'s seed 5: grammar produced no geometry (no terminal shapes)"
            ))
        };
        assert_eq!(ask("a", &failing), named(0));
        assert_eq!(ask("a", &edited), None, "edited since");
        assert_eq!(ask("a", &failing), named(0));
        assert_eq!(ask("b", &failing), named(1), "another generator");
    }
}

#[cfg(test)]
mod round_mesh_tests {
    use super::*;

    /// #1036: a mesh id listed in `round_meshes` must bake as a
    /// tessellated elliptical prism, while every other terminal in the
    /// same grammar keeps the 24-vertex cuboid. Guards the whole wire:
    /// record field → `ShapeDef` → cache key → `build_profiled_mesh_with`.
    #[test]
    fn listed_terminals_bake_round_and_others_stay_square() {
        let mut meshes = Assets::<Mesh>::default();
        let mut cache = UpstreamShapeMeshCache::default();
        let grammar = [
            "Lot --> Split(X) { ~1: Shaft | ~1: Pier }",
            "Shaft --> Extrude(3) Taper(0.1) I(\"Column\")",
            "Pier --> Extrude(3) I(\"Wall\")",
        ]
        .join("\n");
        let round: Vec<String> = vec!["Column".to_string()];

        let built = build_shape_geometry(
            &ShapeDef {
                grammar_source: &grammar,
                root_rule: "Lot",
                footprint: Fp3([4.0, 0.0, 2.0]),
                seed: 1,
                materials: &HashMap::new(),
                round_meshes: &round,
            },
            "test_gen",
            &mut meshes,
            &mut cache,
        )
        .expect("grammar must derive");
        assert_eq!(built.len(), 2);

        let vert_count = |i: usize| -> usize {
            meshes
                .get(&built[i].mesh)
                .expect("mesh handle must resolve")
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .expect("positions")
                .len()
        };
        // Terminals come out in split order: Shaft, then Pier.
        assert!(
            vert_count(0) > 24,
            "the listed Column should be a tessellated prism, got {} verts",
            vert_count(0)
        );
        assert_eq!(
            vert_count(1),
            24,
            "an unlisted terminal must keep the cuboid"
        );
    }

    /// Roundness is a geometry input: flipping the list must re-bake, or
    /// the editor would keep serving square columns after the edit.
    #[test]
    fn round_list_invalidates_the_geometry_hash() {
        let fp = Fp3([4.0, 0.0, 2.0]);
        let empty: Vec<String> = Vec::new();
        let listed: Vec<String> = vec!["Column".to_string()];
        let reordered: Vec<String> = vec!["Column".to_string(), "Silo".to_string()];
        let same_set: Vec<String> = vec!["Silo".to_string(), "Column".to_string()];

        let h = |round: &[String]| {
            shape_geometry_fingerprint(&ShapeDef {
                grammar_source: "Lot --> I(\"Column\")",
                root_rule: "Lot",
                footprint: fp,
                seed: 1,
                materials: &HashMap::new(),
                round_meshes: round,
            })
        };

        assert_ne!(h(&empty), h(&listed), "adding a turned id must re-bake");
        assert_eq!(
            h(&reordered),
            h(&same_set),
            "the same set in a different order should reuse the bake"
        );
    }
}

#[cfg(test)]
mod card_uv_tests {
    use super::*;
    use crate::pds::{Fp, SovereignAshlarConfig, SovereignTextureConfig, SovereignWindowConfig};

    fn card_mat() -> SovereignMaterialSettings {
        SovereignMaterialSettings {
            texture: SovereignTextureConfig::Window(SovereignWindowConfig::default()),
            ..Default::default()
        }
    }

    fn surface_mat() -> SovereignMaterialSettings {
        SovereignMaterialSettings {
            texture: SovereignTextureConfig::Ashlar(SovereignAshlarConfig::default()),
            ..Default::default()
        }
    }

    /// #939: the card predicate the shape mesher keys on must agree with the
    /// upstream render properties that drive clamp-vs-repeat sampling. If
    /// these ever disagree, a card's UVs and its sampler disagree too.
    #[test]
    fn card_predicate_matches_upstream_render_properties() {
        for cfg in [
            SovereignTextureConfig::Window(SovereignWindowConfig::default()),
            SovereignTextureConfig::Ashlar(SovereignAshlarConfig::default()),
            SovereignTextureConfig::None,
        ] {
            assert_eq!(
                cfg.is_card(),
                cfg.to_texture_config().render_properties().is_card,
                "{} diverged from upstream render properties",
                cfg.label()
            );
        }
    }

    /// A `Window` slot is an alpha card and must mesh with stretched UVs; an
    /// `Ashlar` slot must keep world-space tiling. Asserted on the mesh
    /// itself rather than on the flag, so a regression in how the flag is
    /// threaded to `build_profiled_mesh` is caught too: a 4 m stretched face
    /// spans `0..1`, a tiled one spans `0..4`.
    #[test]
    fn card_slots_stretch_their_uvs_and_surfaces_tile() {
        let mut meshes = Assets::<Mesh>::default();
        let mut cache = UpstreamShapeMeshCache::default();
        let mut materials = HashMap::new();
        materials.insert("Glass".to_string(), card_mat());
        materials.insert("Stone".to_string(), surface_mat());

        let grammar = [
            "Lot --> Split(X) { ~1: GlassPart | ~1: StonePart }",
            "GlassPart --> Extrude(4) Mat(\"Glass\") I(\"Pane\")",
            "StonePart --> Extrude(4) Mat(\"Stone\") I(\"Wall\")",
        ]
        .join("\n");

        let built = build_shape_geometry(
            &ShapeDef {
                grammar_source: &grammar,
                root_rule: "Lot",
                footprint: Fp3([8.0, 0.0, 4.0]),
                seed: 1,
                materials: &materials,
                round_meshes: &[],
            },
            "test_gen",
            &mut meshes,
            &mut cache,
        )
        .expect("grammar must derive");

        let max_u = |slot: &str| {
            let inst = built
                .iter()
                .find(|i| i.material_id.as_deref() == Some(slot))
                .unwrap_or_else(|| panic!("no terminal carried the `{slot}` slot"));
            let mesh = meshes.get(&inst.mesh).expect("mesh handle must resolve");
            let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) =
                mesh.attribute(Mesh::ATTRIBUTE_UV_0)
            else {
                panic!("`{slot}` mesh has no UV_0 attribute");
            };
            uvs.iter().map(|uv| uv[0]).fold(0.0_f32, f32::max)
        };

        let glass_u = max_u("Glass");
        assert!(
            (glass_u - 1.0).abs() < 1e-4,
            "card slot must span 0..1, got 0..{glass_u}"
        );

        let stone_u = max_u("Stone");
        assert!(
            stone_u > 1.5,
            "surface slot must tile in world space (metres), got 0..{stone_u}"
        );
    }

    /// The geometry cache is keyed on which slots are cards, so flipping a
    /// slot from surface to card must invalidate it - otherwise the editor
    /// would keep handing out the tiled mesh after the swap. Colour-only
    /// edits must NOT invalidate it (that is what `ShapeMaterialCache` is
    /// for), or every roughness tweak would re-derive the whole grammar.
    #[test]
    fn card_ness_invalidates_the_geometry_hash_but_colour_does_not() {
        let fp = Fp3([8.0, 0.0, 4.0]);
        let mut surface = HashMap::new();
        surface.insert("Slot".to_string(), surface_mat());

        let mut card = HashMap::new();
        card.insert("Slot".to_string(), card_mat());

        let mut recoloured = HashMap::new();
        recoloured.insert(
            "Slot".to_string(),
            SovereignMaterialSettings {
                roughness: Fp(0.123),
                ..surface_mat()
            },
        );

        let h = |m: &HashMap<String, SovereignMaterialSettings>| {
            shape_geometry_fingerprint(&ShapeDef {
                grammar_source: "Lot --> I(\"x\")",
                root_rule: "Lot",
                footprint: fp,
                seed: 1,
                materials: m,
                round_meshes: &[],
            })
        };

        assert_ne!(
            h(&surface),
            h(&card),
            "flipping a slot to an alpha card must rebuild the geometry"
        );
        assert_eq!(
            h(&surface),
            h(&recoloured),
            "a colour/roughness edit must reuse the baked mesh list"
        );
    }
}
