//! Ground-cover draw distance (#1480): small scattered copies stop being
//! drawn past a distance the player sets in Settings.
//!
//! # Why
//!
//! Most visitors play in a browser, on one thread, where every drawn entity
//! costs CPU each frame whatever its size on screen: it is range- and
//! frustum-tested, extracted and batched, for the camera and again for the
//! sun's shadow cascades. Every primitive of every scattered copy is its own
//! entity, and ground cover is most of a planted world's - the Understory
//! draws 38,579 parts, about 78% of them heath, ferns, moss, reeds and
//! rosettes, all out to the fog, though a 1 m plant 150 m away is a few
//! pixels.
//!
//! # The rule
//!
//! A scattered or gridded generator's size is the largest side of the box
//! its first copy draws, in the generator's own frame - the copy's
//! placement, yaw, tilt and scale left out - measured once per generator per
//! compile job ([`CopyRecorder`]). Each copy is then sized as that times its
//! own uniform scale, the scatter's per-copy `scale_jitter`, so a copy the
//! jitter grew past its generator's class is classed by what it draws. The
//! copy's size picks a [`SizeClass`]; the class and the player's setting
//! pick the cut ([`DrawDistanceCuts::resolve`]):
//!
//! * **Small** (up to [`cfg::SMALL_MAX_M`]): cut at the setting.
//! * **Medium** (up to [`cfg::MEDIUM_MAX_M`]): cut at the setting times
//!   `MEDIUM_MAX_M / SMALL_MAX_M`, where the class's largest copy is as big
//!   on screen as the largest ground cover at its own cut.
//! * **Anything bigger** - trees, boulders, buildings - is never cut, and
//!   carries nothing: it is drawn exactly as before.
//! * **No cut lies past the fog.** A cut is capped at the room's fog
//!   visibility, where the fog has already taken 95% of the contrast. The
//!   cap never draws anything LESS than the fog shows; it keeps a foggy room
//!   from drawing its ground cover all the way to the far plane when the
//!   fog is nearer than the setting.
//! * **Unlimited** (the slider's far end) cuts nothing.
//!
//! Scattered and gridded copies only: an absolute placement is one thing the
//! owner put somewhere on purpose. Water, particles, portals and gateways are
//! not cut either - they are not ground cover, and a portal or a gateway is
//! something to walk to from afar.
//!
//! **Not behind the login screen.** While the attract backdrop
//! ([`crate::attract::AttractScene`]) is up there are no cuts: its camera
//! orbits about 150 m out, so a cut at the default would split the demo
//! world's ground cover across the middle of the first screen every visitor
//! sees, and sweep with the orbit. Login removes the marker, and the next
//! frame's cuts restamp every part.
//!
//! # Where the range goes
//!
//! Bevy's `VisibilityRange` is read per entity and does not propagate to
//! children, so it goes on every entity of a copy that has a `Mesh3d` - a
//! primitive's own mesh, each render child of a primitive split by material,
//! an L-system's material buckets, a shape grammar's terminals, a sign - and
//! on nothing else. The spawners report each of those to the
//! [`CopyRecorder`] as they create it; when the copy is done, the compile
//! stamps them with their [`SizeClass`] and the class's range.
//!
//! # Idle-free
//!
//! [`follow_draw_distance`] runs every frame and resolves the cuts from the
//! setting, the fog and the login marker - three reads and a comparison. Only when the resolved
//! cuts differ from the ones in force ([`DrawDistanceCuts`], the resource
//! the compile also stamps new copies from) does it walk the stamped parts,
//! and then it writes only a range that differs: a slider nudge that lands
//! on the same grid stop, or a fog edit that moves no cut, touches nothing.
//!
//! # WebGL2
//!
//! Every range here is `VisibilityRange::abrupt(0, cut)`: zero margins. Any
//! margin compiles Bevy's dither shader, which fails WebGL2 pipeline
//! validation and quits every web client (#1358). Bevy also keeps every
//! distinct range the app has ever used in a table that is a 64-entry
//! uniform on WebGL2 ([`cfg::WEBGL2_RANGE_SLOTS`]); the setting is snapped
//! to [`cfg::STEP_M`] and so is the fog cap, so every cut is a multiple of
//! 25 m between 25 and 800 - at most 32 distinct ranges, however the slider
//! and the fog are dragged. `ranges_stay_webgl2_safe` is the guard.

use std::collections::HashMap;

use bevy::camera::primitives::MeshAabb;
use bevy::camera::visibility::VisibilityRange;
use bevy::math::Affine3A;
use bevy::prelude::*;

use crate::config::draw_distance as cfg;
use crate::state::{LiveRoomRecord, LocalSettings};

/// How big a scattered copy draws, as far as its cut is concerned (#1480).
/// Carried by every drawn part of a small or medium copy; a bigger copy's
/// parts carry nothing.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SizeClass {
    /// Up to [`cfg::SMALL_MAX_M`]: cut at the setting.
    Small,
    /// Up to [`cfg::MEDIUM_MAX_M`]: cut further out, in proportion.
    Medium,
}

impl SizeClass {
    /// Every class that can be cut.
    #[cfg(test)]
    pub(crate) const ALL: [SizeClass; 2] = [SizeClass::Small, SizeClass::Medium];

    /// The class of a copy whose drawn box's largest side is `size_m`, or
    /// `None` for anything bigger - or a size that is not a number, which is
    /// drawn as it always was rather than cut on a guess.
    pub(crate) fn of(size_m: f32) -> Option<Self> {
        if size_m.is_nan() {
            None
        } else if size_m <= cfg::SMALL_MAX_M {
            Some(Self::Small)
        } else if size_m <= cfg::MEDIUM_MAX_M {
            Some(Self::Medium)
        } else {
            None
        }
    }

    /// The class's cut as a multiple of the setting: the class's largest
    /// size over ground cover's.
    fn factor(self) -> f32 {
        match self {
            Self::Small => 1.0,
            Self::Medium => cfg::MEDIUM_MAX_M / cfg::SMALL_MAX_M,
        }
    }
}

/// The setting on the slider's grid, clamped to its ends, or `None` for
/// Unlimited. A value that is not a number is the default.
pub(crate) fn snapped_setting(setting_m: f32) -> Option<f32> {
    if setting_m.is_nan() {
        return Some(cfg::DEFAULT_M);
    }
    if setting_m >= cfg::UNLIMITED_M {
        return None;
    }
    let clamped = setting_m.clamp(cfg::MIN_M, cfg::MAX_M);
    Some((clamped / cfg::STEP_M).round() * cfg::STEP_M)
}

/// The fog's visibility rounded UP onto the grid - never short of what the
/// fog shows - or no cap at all for a fog that is not a finite number.
fn fog_cap(fog_visibility_m: f32) -> f32 {
    if !fog_visibility_m.is_finite() {
        return f32::INFINITY;
    }
    (fog_visibility_m.max(cfg::STEP_M) / cfg::STEP_M).ceil() * cfg::STEP_M
}

/// The cut each [`SizeClass`] takes now, in metres from the camera, `None`
/// where it takes none (#1480).
///
/// As a resource it is the cuts IN FORCE: what every stamped part carries,
/// what the compile stamps new copies with, and what
/// [`follow_draw_distance`] compares the wanted cuts against. Its default -
/// no cuts - is what an app without that system (a test, an embedder that
/// never registered it) spawns under.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct DrawDistanceCuts {
    small: Option<f32>,
    medium: Option<f32>,
}

impl DrawDistanceCuts {
    /// The cuts for a player's `setting_m` in a room whose fog leaves
    /// `fog_visibility_m` visible. See the module doc for the rule.
    pub(crate) fn resolve(setting_m: f32, fog_visibility_m: f32) -> Self {
        let Some(setting) = snapped_setting(setting_m) else {
            return Self::default();
        };
        let cap = fog_cap(fog_visibility_m);
        let cut = |class: SizeClass| Some((setting * class.factor()).min(cap));
        Self {
            small: cut(SizeClass::Small),
            medium: cut(SizeClass::Medium),
        }
    }

    /// The cuts the live settings and room ask for: the setting's default
    /// without settings, the config fog without a room, and none at all
    /// behind the login screen (`attract`; see the module doc).
    fn wanted(
        settings: Option<&LocalSettings>,
        record: Option<&LiveRoomRecord>,
        attract: bool,
    ) -> Self {
        if attract {
            return Self::default();
        }
        let setting = settings.map_or(cfg::DEFAULT_M, |s| s.ground_cover_draw_distance_m);
        let fog = record.map_or(crate::config::camera::fog::VISIBILITY, |r| {
            r.0.environment.fog_visibility.0
        });
        Self::resolve(setting, fog)
    }

    /// The cut `class` takes, if any.
    pub(crate) fn cut(&self, class: SizeClass) -> Option<f32> {
        match class {
            SizeClass::Small => self.small,
            SizeClass::Medium => self.medium,
        }
    }

    /// The range a part of `class` carries, if any: drawn from the camera
    /// out to the cut, with no margin at either end - see the module doc's
    /// WebGL2 section for why there must never be one.
    pub(crate) fn range(&self, class: SizeClass) -> Option<VisibilityRange> {
        self.cut(class).map(|cut| VisibilityRange::abrupt(0.0, cut))
    }
}

/// What the compile records while it spawns one scattered or gridded copy
/// (#1480): the entities that draw it, its scale, and - on a generator's
/// first copy of the job - the box they draw, which sizes the generator for
/// the rest.
///
/// Owned by the compile job, and lent to the spawners through
/// [`SpawnCtx`](super::compile::SpawnCtx). Outside a copy it records
/// nothing, so absolute placements and avatar trees pass through untouched.
#[derive(Default)]
pub(crate) struct CopyRecorder {
    /// Each generator's size in its own frame (m, the largest side of its
    /// drawn box), by record key, once its first copy has been measured this
    /// job. `None` is a measured generator that drew nothing measurable,
    /// which is never cut.
    sizes: HashMap<String, Option<f32>>,
    /// Inside a copy.
    recording: bool,
    /// The copy's own uniform scale - the scatter's per-copy jitter - which
    /// the generator's size is multiplied by to size the copy.
    scale: f32,
    /// Inside the first copy of a generator: the box is being measured.
    measuring: bool,
    /// The current node's frame relative to the copy's own, while measuring.
    frame: Affine3A,
    /// The drawn box so far, in the copy's frame, while measuring.
    bounds: Option<(Vec3, Vec3)>,
    /// The copy's drawn parts so far.
    parts: Vec<Entity>,
}

impl CopyRecorder {
    /// Start recording a copy of `generator_ref` planted at `cell_tf` in its
    /// anchor. Measured when this job has not sized the generator yet; the
    /// frame starts at the cell's inverse, so the copy's root lands in its
    /// generator's own frame and the box leaves the scatter's per-copy
    /// scale, tilt and yaw out. The scale is kept, to size the copy by in
    /// [`Self::end_copy`]: a scatter's scale is uniform
    /// ([`instance_pose`](super::compile::scatter::instance_pose)), a grid
    /// cell's is one, and both anchors are unscaled, so its largest axis is
    /// how much bigger than its generator the copy draws.
    pub(crate) fn begin_copy(&mut self, generator_ref: &str, cell_tf: &Transform) {
        self.recording = true;
        self.parts.clear();
        self.scale = cell_tf.scale.abs().max_element();
        self.measuring = !self.sizes.contains_key(generator_ref);
        if self.measuring {
            self.frame = cell_tf.compute_affine().inverse();
            self.bounds = None;
        }
    }

    /// Descend into a node spawned at `transform` under the current one.
    /// Returns the frame to restore on the way out; nothing while not
    /// measuring, when the frame is not kept at all.
    pub(crate) fn enter_node(&mut self, transform: &Transform) -> Option<Affine3A> {
        if !self.measuring {
            return None;
        }
        let saved = self.frame;
        self.frame = saved * transform.compute_affine();
        Some(saved)
    }

    /// Climb back out of a node [`Self::enter_node`] entered.
    pub(crate) fn leave_node(&mut self, saved: Option<Affine3A>) {
        if let Some(frame) = saved {
            self.frame = frame;
        }
    }

    /// A part of the copy: `entity` draws `mesh` at `local` under the
    /// current node's entity. Outside a copy it is not recorded.
    pub(crate) fn note(
        &mut self,
        entity: Entity,
        mesh: &Handle<Mesh>,
        local: &Transform,
        meshes: &Assets<Mesh>,
    ) {
        if !self.recording {
            return;
        }
        self.parts.push(entity);
        if !self.measuring {
            return;
        }
        let Some(aabb) = meshes.get(mesh).and_then(MeshAabb::compute_aabb) else {
            return;
        };
        let to_copy = self.frame * local.compute_affine();
        let (centre, half) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
        let (mut min, mut max) = self
            .bounds
            .unwrap_or((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)));
        for corner in [
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(-1.0, -1.0, 1.0),
            Vec3::new(-1.0, 1.0, -1.0),
            Vec3::new(-1.0, 1.0, 1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(1.0, -1.0, 1.0),
            Vec3::new(1.0, 1.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
        ] {
            let at = to_copy.transform_point3(centre + corner * half);
            min = min.min(at);
            max = max.max(at);
        }
        self.bounds = Some((min, max));
    }

    /// Finish the copy of `generator_ref`: size the generator if this was
    /// its first copy, class the copy by the generator's size times the
    /// copy's own scale, and stamp every part with the class and the class's
    /// range under `cuts`. A copy too big to cut stamps nothing.
    pub(crate) fn end_copy(
        &mut self,
        generator_ref: &str,
        commands: &mut Commands,
        cuts: &DrawDistanceCuts,
    ) {
        self.recording = false;
        if self.measuring {
            self.measuring = false;
            let size = self
                .bounds
                .take()
                .map(|(min, max)| (max - min).max_element());
            self.sizes.insert(generator_ref.to_string(), size);
        }
        let class = self
            .sizes
            .get(generator_ref)
            .copied()
            .flatten()
            .and_then(|size| SizeClass::of(size * self.scale));
        let Some(class) = class else {
            self.parts.clear();
            return;
        };
        let range = cuts.range(class);
        // `try_`, as every insert on a spawned room entity is (#1410): the
        // parts were queued a moment ago in this same command buffer, but a
        // despawned target aborts a Bevy 0.19 client, and a stamp is never
        // worth that. The class and its range go on as one bundle: one
        // archetype move per part, not two.
        for entity in self.parts.drain(..) {
            let mut part = commands.entity(entity);
            match range.clone() {
                Some(range) => part.try_insert((class, range)),
                None => part.try_insert(class),
            };
        }
    }

    /// The size this job measured `generator_ref` at, if it has.
    #[cfg(test)]
    pub(crate) fn size_of(&self, generator_ref: &str) -> Option<Option<f32>> {
        self.sizes.get(generator_ref).copied()
    }
}

/// Keep every stamped part on the cuts the settings and the room ask for
/// (#1480). See the module doc's "Idle-free": a frame where the resolved
/// cuts have not moved reads three resources and returns.
///
/// Runs in `PostUpdate`, after the compile's `Update` commands have landed,
/// so a copy stamped from the cuts in force is always in the walk that
/// replaces them. Ordered before Bevy's visibility pass, so a new range culls
/// on the frame it is set.
pub(crate) fn follow_draw_distance(
    settings: Option<Res<LocalSettings>>,
    record: Option<Res<LiveRoomRecord>>,
    attract: Option<Res<crate::attract::AttractScene>>,
    mut in_force: ResMut<DrawDistanceCuts>,
    mut parts: Query<(Entity, &SizeClass, Option<&mut VisibilityRange>)>,
    mut commands: Commands,
) {
    let wanted =
        DrawDistanceCuts::wanted(settings.as_deref(), record.as_deref(), attract.is_some());
    // Read through `Deref`, which stamps nothing.
    if *in_force == wanted {
        return;
    }
    for (entity, class, current) in &mut parts {
        match (current, wanted.range(*class)) {
            // Compared before written: a `Mut` stamps on write, not on read.
            (Some(mut current), Some(range)) => {
                if *current != range {
                    *current = range;
                }
            }
            // `try_`: a recompile can despawn the part before these land,
            // and a plain insert on a despawned entity aborts the client
            // (#1410).
            (Some(_), None) => {
                commands.entity(entity).try_remove::<VisibilityRange>();
            }
            (None, Some(range)) => {
                commands.entity(entity).try_insert(range);
            }
            (None, None) => {}
        }
    }
    *in_force = wanted;
}

/// Register [`follow_draw_distance`] and the cuts it keeps. Shared by the
/// game's world-builder plugin and the render tool's headless compile, so a
/// `--world` render culls what a default-settings visitor's game culls.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<DrawDistanceCuts>().add_systems(
        PostUpdate,
        follow_draw_distance.before(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    );
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::config::camera::fog::VISIBILITY;

    /// Fogs across the sanitizer's whole range (10 m to 10 km), the seeded
    /// biomes' (180 to 600 m), the config default and the Understory's.
    const FOGS: [f32; 10] = [
        10.0, 60.0, 150.0, 180.0, 300.0, VISIBILITY, 600.0, 1_000.0, 5_000.0, 10_000.0,
    ];

    /// Every stop the slider can land on, Unlimited included.
    fn stops() -> Vec<f32> {
        let mut out = Vec::new();
        let mut at = cfg::MIN_M;
        while at <= cfg::UNLIMITED_M {
            out.push(at);
            at += cfg::STEP_M;
        }
        out
    }

    /// The cut a copy of `size_m` takes.
    fn cut(size_m: f32, setting_m: f32, fog_m: f32) -> Option<f32> {
        SizeClass::of(size_m)
            .and_then(|class| DrawDistanceCuts::resolve(setting_m, fog_m).cut(class))
    }

    #[test]
    fn ground_cover_is_cut_at_the_setting() {
        // A clear room, so the fog cap stays out of the way.
        for size in [0.05, 0.8, 1.4, cfg::SMALL_MAX_M] {
            assert_eq!(cut(size, 150.0, 10_000.0), Some(150.0), "{size} m");
            assert_eq!(cut(size, 75.0, 10_000.0), Some(75.0), "{size} m");
        }
        assert_eq!(cut(1.0, cfg::DEFAULT_M, 10_000.0), Some(cfg::DEFAULT_M));
    }

    #[test]
    fn a_bigger_copy_is_cut_further_out_in_proportion() {
        let factor = cfg::MEDIUM_MAX_M / cfg::SMALL_MAX_M;
        assert_eq!(factor, 2.0);
        for size in [cfg::SMALL_MAX_M + 0.01, 3.0, cfg::MEDIUM_MAX_M] {
            assert_eq!(cut(size, 150.0, 10_000.0), Some(300.0), "{size} m");
        }
        // The largest copy of each class is as big on screen at its cut.
        let small = cfg::SMALL_MAX_M / cut(cfg::SMALL_MAX_M, 150.0, 10_000.0).unwrap();
        let medium = cfg::MEDIUM_MAX_M / cut(cfg::MEDIUM_MAX_M, 150.0, 10_000.0).unwrap();
        assert_eq!(small, medium);
    }

    #[test]
    fn trees_and_big_things_are_never_cut() {
        for fog in FOGS {
            for setting in stops() {
                for size in [cfg::MEDIUM_MAX_M + 0.01, 6.0, 12.0, 40.0, f32::INFINITY] {
                    assert_eq!(cut(size, setting, fog), None, "{size} m, {setting}, {fog}");
                }
                assert_eq!(cut(f32::NAN, setting, fog), None, "an unmeasurable copy");
            }
        }
    }

    #[test]
    fn unlimited_cuts_nothing() {
        for fog in FOGS {
            for setting in [
                cfg::UNLIMITED_M,
                cfg::UNLIMITED_M + 1.0,
                1.0e9,
                f32::INFINITY,
            ] {
                assert_eq!(
                    DrawDistanceCuts::resolve(setting, fog),
                    DrawDistanceCuts::default(),
                    "{setting} under {fog} m fog"
                );
            }
        }
        // The slider's last distance still cuts.
        assert_eq!(cut(1.0, cfg::MAX_M, 10_000.0), Some(cfg::MAX_M));
    }

    /// No cut lies past the fog, and none short of what it shows.
    #[test]
    fn a_cut_is_capped_at_the_fog_and_never_inside_it() {
        // The Understory: 300 m of fog, the default setting.
        assert_eq!(cut(1.0, 150.0, 300.0), Some(150.0));
        assert_eq!(cut(3.0, 150.0, 300.0), Some(300.0));
        // A room foggier than the setting draws its ground cover to the
        // fog, not to the far plane.
        assert_eq!(cut(1.0, 150.0, 100.0), Some(100.0));
        assert_eq!(cut(3.0, 150.0, 100.0), Some(100.0));
        // A fog off the grid rounds UP: the cap never falls inside it.
        assert_eq!(cut(1.0, 150.0, 101.0), Some(125.0));
        for fog in FOGS {
            for setting in stops() {
                for class in SizeClass::ALL {
                    if let Some(at) = DrawDistanceCuts::resolve(setting, fog).cut(class) {
                        assert!(at >= fog.min(setting * class.factor()), "{at} {fog}");
                        assert!(at < fog + cfg::STEP_M || at <= setting * class.factor());
                    }
                }
            }
        }
    }

    /// A bigger copy is never cut nearer than a smaller one, and a longer
    /// setting never cuts anything nearer.
    #[test]
    fn the_cut_never_shrinks_as_a_copy_or_the_setting_grows() {
        let far = |c: Option<f32>| c.unwrap_or(f32::INFINITY);
        let sizes: Vec<f32> = (0..=60).map(|i| i as f32 * 0.1).collect();
        for fog in FOGS {
            for setting in stops() {
                for pair in sizes.windows(2) {
                    assert!(
                        far(cut(pair[1], setting, fog)) >= far(cut(pair[0], setting, fog)),
                        "{} m cut nearer than {} m at {setting} under {fog} m fog",
                        pair[1],
                        pair[0]
                    );
                }
            }
            let settings = stops();
            for pair in settings.windows(2) {
                for size in [0.5, 3.0] {
                    assert!(far(cut(size, pair[1], fog)) >= far(cut(size, pair[0], fog)));
                }
            }
        }
    }

    /// A hand-edited prefs file can neither cut at the player's feet nor put
    /// a cut off the grid.
    #[test]
    fn the_setting_is_snapped_and_clamped_on_the_way_in() {
        assert_eq!(snapped_setting(0.0), Some(cfg::MIN_M));
        assert_eq!(snapped_setting(-40.0), Some(cfg::MIN_M));
        assert_eq!(snapped_setting(f32::NEG_INFINITY), Some(cfg::MIN_M));
        assert_eq!(snapped_setting(137.3), Some(125.0));
        assert_eq!(snapped_setting(163.0), Some(175.0));
        assert_eq!(snapped_setting(cfg::UNLIMITED_M - 0.5), Some(cfg::MAX_M));
        assert_eq!(snapped_setting(f32::NAN), Some(cfg::DEFAULT_M));
    }

    /// The WebGL2 guard, in the style of the hair one
    /// (`a_hair_crossfade_would_quit_every_webgl2_client`): every range this
    /// feature can make has zero margins, and all of them together - over
    /// every slider stop, off-grid settings and fogs across the sanitizer's
    /// range - are few enough to sit well inside Bevy's WebGL2 table.
    #[test]
    fn ranges_stay_webgl2_safe() {
        let mut settings = stops();
        settings.extend((0..=900).map(|i| i as f32 * 0.5));
        let mut fogs: Vec<f32> = (10..=10_000).step_by(7).map(|f| f as f32).collect();
        fogs.extend(FOGS);
        let mut distinct = HashSet::new();
        for &setting in &settings {
            for &fog in &fogs {
                let cuts = DrawDistanceCuts::resolve(setting, fog);
                for class in SizeClass::ALL {
                    let Some(range) = cuts.range(class) else {
                        continue;
                    };
                    assert!(
                        range.is_abrupt(),
                        "a {class:?} range at {setting} m under {fog} m fog crossfades; \
                         any margin compiles Bevy's dither shader, which quits every \
                         WebGL2 client"
                    );
                    assert_eq!(range.start_margin, 0.0..0.0);
                    assert!(!range.use_aabb);
                    distinct.insert(range);
                }
            }
        }
        // Every cut is on the grid, between one step and twice the last
        // distance: at most 32 of them.
        let bound = (cfg::MAX_M * cfg::MEDIUM_MAX_M / cfg::SMALL_MAX_M / cfg::STEP_M) as usize;
        assert_eq!(bound, 32);
        assert!(
            distinct.len() <= bound,
            "{} distinct ranges, the grid allows {bound}",
            distinct.len()
        );
        // Half the table at most, and the hair's two tiers fit beside them
        // with room to spare.
        assert!(
            distinct.len() <= cfg::WEBGL2_RANGE_SLOTS / 2,
            "{} distinct ranges crowd a {}-slot table",
            distinct.len(),
            cfg::WEBGL2_RANGE_SLOTS
        );

        // The control: the same checks have to be able to FAIL. A margin of
        // a metre is a crossfade, and an unsnapped setting mints a range per
        // value.
        let band = VisibilityRange {
            start_margin: 0.0..0.0,
            end_margin: 150.0..151.0,
            use_aabb: false,
        };
        assert!(!band.is_abrupt());
        let unsnapped: HashSet<VisibilityRange> = settings
            .iter()
            .filter(|s| **s >= cfg::MIN_M && **s <= cfg::MAX_M)
            .map(|s| VisibilityRange::abrupt(0.0, *s))
            .collect();
        assert!(unsnapped.len() > bound);
    }

    #[test]
    fn a_copy_is_sized_by_its_largest_side() {
        assert_eq!(SizeClass::of(0.0), Some(SizeClass::Small));
        assert_eq!(SizeClass::of(cfg::SMALL_MAX_M), Some(SizeClass::Small));
        assert_eq!(
            SizeClass::of(cfg::SMALL_MAX_M + 0.001),
            Some(SizeClass::Medium)
        );
        assert_eq!(SizeClass::of(cfg::MEDIUM_MAX_M), Some(SizeClass::Medium));
        assert_eq!(SizeClass::of(cfg::MEDIUM_MAX_M + 0.001), None);
    }

    /// An old prefs file - written before the setting existed - loads with
    /// the default, and a saved setting survives the round trip.
    #[test]
    fn an_old_prefs_file_loads_with_the_default_distance() {
        let older = r#"{"smooth_kinematics":false,"camera_ground_clearance_m":2.0}"#;
        let settings: LocalSettings = serde_json::from_str(older).expect("older prefs load");
        assert_eq!(settings.ground_cover_draw_distance_m, cfg::DEFAULT_M);
        assert_eq!(LocalSettings::default().ground_cover_draw_distance_m, 150.0);

        let saved = LocalSettings {
            ground_cover_draw_distance_m: cfg::UNLIMITED_M,
            ..Default::default()
        };
        let wire = serde_json::to_string(&saved).expect("settings serialise");
        let back: LocalSettings = serde_json::from_str(&wire).expect("and load");
        assert_eq!(back, saved);
    }
}

#[cfg(test)]
mod ecs_tests {
    //! The rule on real spawns (#1480): a minimal headless app driving the
    //! real compile and [`follow_draw_distance`] over a record that plants
    //! every kind of copy the rule tells apart.

    use std::collections::HashMap;

    use bevy::ecs::change_detection::Tick;

    use super::*;
    use crate::pds::generator::{FaceKey, FaceOverride};
    use crate::pds::{
        BiomeFilter, Environment, Fp, Fp2, Fp3, Generator, GeneratorKind, Placement, RoomRecord,
        ScatterBounds, ScatterNaturalness, SovereignMaterialSettings, TransformData,
    };
    use crate::world_builder::PrimMarker;
    use crate::world_builder::compile::{CompileJob, CompiledWorld, compile_room_record};

    /// A non-solid cuboid of `size` m.
    fn cuboid(size: [f32; 3]) -> Generator {
        let mut kind = GeneratorKind::default_cuboid();
        if let GeneratorKind::Cuboid { size: s, common } = &mut kind {
            *s = Fp3(size);
            common.solid = false;
        }
        Generator::from_kind(kind)
    }

    /// A 1 m cuboid whose top wears its own material: a transform-only root
    /// with one render child per material.
    fn painted() -> Generator {
        let mut g = cuboid([1.0, 1.0, 1.0]);
        g.kind.faces_mut().expect("a primitive").push(FaceOverride {
            face: FaceKey::Top,
            material: SovereignMaterialSettings {
                base_color: Fp3([0.9, 0.1, 0.1]),
                ..Default::default()
            },
            uv_mapping: None,
        });
        g
    }

    /// A 0.5 m cuboid with a 0.5 m child 3 m above it: small parts, a
    /// medium copy - sized only if the child's frame is followed down.
    fn stack() -> Generator {
        let mut root = cuboid([0.5, 0.5, 0.5]);
        let mut top = cuboid([0.5, 0.5, 0.5]);
        top.transform = TransformData {
            translation: Fp3([0.0, 3.0, 0.0]),
            ..Default::default()
        };
        root.children.push(top);
        root
    }

    /// A 1 m box grown by a shape grammar: its terminals draw.
    fn crate_box() -> Generator {
        Generator::from_kind(GeneratorKind::Shape {
            grammar_source: "Lot --> Extrude(1) I(\"Box\")".to_string(),
            root_rule: "Lot".to_string(),
            footprint: Fp3([1.0, 0.0, 1.0]),
            seed: 1,
            materials: HashMap::new(),
            round_meshes: Vec::new(),
        })
    }

    fn scatter(generator_ref: &str, count: u32, x: f32) -> Placement {
        Placement::Scatter {
            generator_ref: generator_ref.to_string(),
            bounds: ScatterBounds::Circle {
                center: Fp2([x, 0.0]),
                radius: Fp(8.0),
            },
            count,
            local_seed: 7,
            biome_filter: BiomeFilter::default(),
            snap_to_terrain: false,
            random_yaw: true,
            avoid_urban: false,
            float_on_water: false,
            naturalness: ScatterNaturalness::default(),
        }
    }

    const COPIES: u32 = 6;

    /// Every kind of copy: ground cover whole (a scatter and a grid), split
    /// by material, grown by a grammar; a medium copy, whole and nested; a
    /// tree-sized one; and the same small cuboid placed absolutely.
    fn record() -> RoomRecord {
        let fern = crate::catalogue::by_slug("lsys_fern")
            .expect("the catalogue fern")
            .build("did:plc:test");
        let generators = HashMap::from([
            ("tuft".to_string(), cuboid([0.3, 1.0, 0.3])),
            ("painted".to_string(), painted()),
            ("fern".to_string(), fern),
            ("shrub".to_string(), cuboid([3.0, 2.5, 3.0])),
            ("stack".to_string(), stack()),
            ("tree".to_string(), cuboid([1.0, 9.0, 1.0])),
            ("crate".to_string(), crate_box()),
            (
                "sign".to_string(),
                Generator::from_kind(GeneratorKind::default_sign()),
            ),
        ]);
        RoomRecord {
            lex_type: "network.symbios.room".to_string(),
            environment: Environment::default(),
            generators,
            placements: vec![
                scatter("tuft", COPIES, 0.0),
                scatter("painted", COPIES, 20.0),
                scatter("fern", COPIES, 40.0),
                scatter("shrub", COPIES, 60.0),
                scatter("stack", COPIES, 80.0),
                scatter("tree", COPIES, 100.0),
                scatter("crate", COPIES, 120.0),
                scatter("sign", COPIES, 140.0),
                Placement::Absolute {
                    generator_ref: "tuft".to_string(),
                    transform: TransformData::default(),
                    snap_to_terrain: false,
                    avoid_water: false,
                    avoid_water_clearance: Fp(0.0),
                },
                Placement::Grid {
                    generator_ref: "tuft".to_string(),
                    transform: TransformData {
                        translation: Fp3([0.0, 0.0, 40.0]),
                        ..Default::default()
                    },
                    counts: [2, 1, 3],
                    gaps: Fp3([2.0, 0.0, 2.0]),
                    snap_to_terrain: false,
                    random_yaw: false,
                },
            ],
            traits: HashMap::new(),
            contact_effects: Default::default(),
            default_landing: None,
            opaque_refs: Default::default(),
        }
    }

    /// The compile's resources, the settings, and the draw-distance system.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.init_asset::<Image>();
        app.init_asset::<StandardMaterial>();
        app.init_asset::<crate::water::WaterMaterial>();
        app.init_resource::<crate::world_builder::lsystem::LSystemMaterialCache>();
        app.init_resource::<crate::world_builder::lsystem::LSystemMeshCache>();
        app.init_resource::<crate::world_builder::shape::ShapeMaterialCache>();
        app.init_resource::<crate::world_builder::shape::ShapeMeshCache>();
        app.init_resource::<crate::world_builder::prim_cache::PrimMeshCache>();
        app.init_resource::<crate::world_builder::prim_cache::PrimMaterialCache>();
        app.init_resource::<bevy_symbios_shape::cache::ShapeMeshCache>();
        app.init_resource::<crate::world_builder::spatial_audio::BakedAudioCache>();
        app.insert_resource(crate::world_builder::fresh_texture_cache());
        app.init_resource::<CompiledWorld>();
        app.init_resource::<CompileJob>();
        app.init_resource::<crate::water::WaterSurfaces>();
        app.init_resource::<crate::world_builder::image_cache::BlobImageCache>();
        app.init_resource::<crate::world_builder::audio_resolver::BlobAudioCache>();
        app.init_resource::<crate::diagnostics::SessionLog>();
        app.init_resource::<LocalSettings>();
        app.insert_resource(LiveRoomRecord(record()));
        app.add_systems(Update, compile_room_record);
        register(&mut app);
        app
    }

    /// Update until the compile drains, and once more for the cuts.
    fn settle(app: &mut App) {
        for _ in 0..64 {
            app.update();
            if app.world().resource::<CompileJob>().progress().is_none() {
                app.update();
                return;
            }
        }
        panic!("compile job did not settle within 64 frames");
    }

    /// One drawn part: the generator it belongs to, and what it carries.
    struct Part {
        generator: String,
        class: Option<SizeClass>,
        range: Option<VisibilityRange>,
        stamped: Option<Tick>,
    }

    /// Every entity that draws a mesh, by the generator whose node it is or
    /// hangs under.
    fn parts(app: &mut App) -> Vec<Part> {
        let world = app.world_mut();
        let markers: HashMap<Entity, String> = world
            .query::<(Entity, &PrimMarker)>()
            .iter(world)
            .map(|(e, m)| (e, m.generator_ref.clone()))
            .collect();
        world
            .query::<(
                Entity,
                Option<&ChildOf>,
                Option<&SizeClass>,
                Option<Ref<VisibilityRange>>,
            )>()
            .iter(world)
            .filter(|(e, ..)| world.get::<Mesh3d>(*e).is_some())
            .map(|(e, parent, class, range)| Part {
                generator: markers
                    .get(&e)
                    .or_else(|| parent.and_then(|p| markers.get(&p.parent())))
                    .cloned()
                    .unwrap_or_default(),
                class: class.copied(),
                range: range.as_deref().cloned(),
                stamped: range.map(|r| r.last_changed()),
            })
            .collect()
    }

    fn of<'a>(parts: &'a [Part], generator: &str) -> Vec<&'a Part> {
        parts.iter().filter(|p| p.generator == generator).collect()
    }

    fn end(part: &Part) -> Option<f32> {
        part.range.as_ref().map(|r| {
            assert!(r.is_abrupt(), "a crossfading range quits WebGL2");
            assert_eq!(r.start_margin, 0.0..0.0);
            r.end_margin.start
        })
    }

    fn set_distance(app: &mut App, metres: f32) {
        app.world_mut()
            .resource_mut::<LocalSettings>()
            .ground_cover_draw_distance_m = metres;
    }

    #[test]
    fn scattered_small_copies_and_every_part_they_draw_carry_the_cut() {
        let mut app = app();
        settle(&mut app);
        let parts = parts(&mut app);
        // The config fog (350 m) is past both cuts.
        let fog = crate::config::camera::fog::VISIBILITY;
        assert!(fog >= 2.0 * cfg::DEFAULT_M);

        let tuft = of(&parts, "tuft");
        // Six scattered copies, six gridded ones, one absolute.
        assert_eq!(tuft.len(), 2 * COPIES as usize + 1, "tuft parts");
        let cut: Vec<_> = tuft.iter().filter(|p| p.range.is_some()).collect();
        assert_eq!(cut.len(), 2 * COPIES as usize, "scatter and grid copies");
        for p in &cut {
            assert_eq!(p.class, Some(SizeClass::Small));
            assert_eq!(end(p), Some(cfg::DEFAULT_M));
        }
        let uncut: Vec<_> = tuft.iter().filter(|p| p.range.is_none()).collect();
        assert_eq!(uncut.len(), 1);
        assert_eq!(uncut[0].class, None, "the absolute placement is left alone");

        // A split prim draws on its render children, and every one of them
        // takes the range: the root draws nothing and carries nothing.
        let painted = of(&parts, "painted");
        assert_eq!(painted.len(), 2 * COPIES as usize, "two materials a copy");
        for p in &painted {
            assert_eq!(p.class, Some(SizeClass::Small));
            assert_eq!(end(p), Some(cfg::DEFAULT_M));
        }

        // Grammar-grown: every material bucket of an L-system fern, every
        // terminal of a shape-grammar box; and a sign's panel.
        for generator in ["fern", "crate", "sign"] {
            let grown = of(&parts, generator);
            assert!(
                grown.len() >= COPIES as usize,
                "{} {generator} parts",
                grown.len()
            );
            for p in &grown {
                assert_eq!(p.class, Some(SizeClass::Small), "{generator}");
                assert_eq!(end(p), Some(cfg::DEFAULT_M), "{generator}");
            }
        }

        // Medium: twice the setting, whole or nested.
        for generator in ["shrub", "stack"] {
            let medium = of(&parts, generator);
            assert!(!medium.is_empty(), "{generator}");
            for p in &medium {
                assert_eq!(p.class, Some(SizeClass::Medium), "{generator}");
                assert_eq!(end(p), Some(2.0 * cfg::DEFAULT_M), "{generator}");
            }
        }
        assert_eq!(of(&parts, "stack").len(), 2 * COPIES as usize);

        // A tree-sized copy is drawn exactly as before.
        let tree = of(&parts, "tree");
        assert_eq!(tree.len(), COPIES as usize);
        for p in &tree {
            assert!(p.class.is_none() && p.range.is_none());
        }
    }

    #[test]
    fn a_setting_change_moves_every_cut_and_an_unchanged_one_stamps_nothing() {
        let mut app = app();
        settle(&mut app);
        let cuts_tick = |app: &App| {
            app.world()
                .resource_ref::<DrawDistanceCuts>()
                .last_changed()
        };
        let before = parts(&mut app);
        let in_force = cuts_tick(&app);

        // Idle, and a nudge that lands on the same stop: nothing is touched.
        for _ in 0..3 {
            app.update();
        }
        set_distance(&mut app, cfg::DEFAULT_M + 5.0);
        app.update();
        app.update();
        let after = parts(&mut app);
        assert_eq!(
            cuts_tick(&app),
            in_force,
            "the cuts in force were restamped"
        );
        for (b, a) in before.iter().zip(&after) {
            assert_eq!(a.stamped, b.stamped, "{} restamped", a.generator);
        }

        // A real change moves every small and medium cut.
        set_distance(&mut app, 100.0);
        app.update();
        let parts_now = parts(&mut app);
        for p in parts_now.iter().filter(|p| p.class.is_some()) {
            let want = match p.class {
                Some(SizeClass::Small) => 100.0,
                _ => 200.0,
            };
            assert_eq!(end(p), Some(want), "{}", p.generator);
        }

        // Unlimited: no part carries a range, though every one keeps its
        // class; and back, and every range returns.
        set_distance(&mut app, cfg::UNLIMITED_M);
        app.update();
        app.update();
        let unlimited = parts(&mut app);
        assert!(unlimited.iter().all(|p| p.range.is_none()));
        assert!(unlimited.iter().filter(|p| p.class.is_some()).count() > 0);
        set_distance(&mut app, cfg::DEFAULT_M);
        app.update();
        app.update();
        let back = parts(&mut app);
        for p in back.iter().filter(|p| p.class == Some(SizeClass::Small)) {
            assert_eq!(end(p), Some(cfg::DEFAULT_M), "{}", p.generator);
        }
    }

    /// A copy spawned while cuts are already in force is stamped with them
    /// by the compile itself: no walk runs for it, because nothing changed.
    #[test]
    fn a_copy_built_after_the_cuts_are_in_force_carries_them() {
        let mut app = app();
        settle(&mut app);
        let in_force = app
            .world()
            .resource_ref::<DrawDistanceCuts>()
            .last_changed();
        let tufts = of(&parts(&mut app), "tuft").len();
        // An appended placement builds on its own (#979); the rest of the
        // world is not rebuilt.
        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .placements
            .push(scatter("tuft", COPIES, -40.0));
        settle(&mut app);
        assert_eq!(
            app.world()
                .resource_ref::<DrawDistanceCuts>()
                .last_changed(),
            in_force,
            "the cuts did not move, so nothing walked"
        );
        let parts = parts(&mut app);
        let tuft = of(&parts, "tuft");
        assert_eq!(tuft.len(), tufts + COPIES as usize);
        let cut = tuft
            .iter()
            .filter(|p| end(p) == Some(cfg::DEFAULT_M))
            .count();
        assert_eq!(
            cut,
            tufts - 1 + COPIES as usize,
            "every copy but the absolute one"
        );
    }

    /// The fog moves only what it caps: fog at 250 m pulls the medium cut
    /// in from 300 m and leaves ground cover's 150 m - untouched, not
    /// rewritten with the same value.
    #[test]
    fn the_fog_moves_only_the_cuts_it_caps() {
        let mut app = app();
        settle(&mut app);
        let before = parts(&mut app);
        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .environment
            .fog_visibility = Fp(250.0);
        settle(&mut app);
        let after = parts(&mut app);
        assert_eq!(before.len(), after.len(), "a fog edit rebuilds nothing");
        for (b, a) in before.iter().zip(&after) {
            match a.class {
                Some(SizeClass::Small) => {
                    assert_eq!(end(a), Some(cfg::DEFAULT_M));
                    assert_eq!(a.stamped, b.stamped, "{} restamped", a.generator);
                }
                Some(SizeClass::Medium) => {
                    assert_eq!(end(a), Some(250.0), "{}", a.generator);
                    assert_ne!(a.stamped, b.stamped);
                }
                None => assert!(a.range.is_none()),
            }
        }
    }

    /// A scatter with `scale_jitter` on the real compile: every copy is
    /// classed by its own drawn size, so the copies the jitter grew past
    /// 2 m are cut at twice the setting and the ones past 4 m not at all,
    /// though their generator is ground cover.
    #[test]
    fn a_jittered_copy_is_classed_by_what_it_draws() {
        const HEIGHT: f32 = 1.8;
        const JITTERED: u32 = 48;
        let mut app = app();
        {
            let mut record = app.world_mut().resource_mut::<LiveRoomRecord>();
            record
                .0
                .generators
                .insert("moss".to_string(), cuboid([0.3, HEIGHT, 0.3]));
            let mut placement = scatter("moss", JITTERED, -60.0);
            if let Placement::Scatter { naturalness, .. } = &mut placement {
                // Copies from 0.41 to 2.46 times the generator: 0.7 to 4.4 m.
                naturalness.scale_jitter = Fp(0.9);
            }
            record.0.placements.push(placement);
        }
        settle(&mut app);
        let world = app.world_mut();
        let copies: Vec<(f32, Option<SizeClass>, Option<VisibilityRange>)> = world
            .query::<(
                &PrimMarker,
                &Transform,
                Option<&SizeClass>,
                Option<&VisibilityRange>,
            )>()
            .iter(world)
            .filter(|(m, ..)| m.generator_ref == "moss")
            .map(|(_, tf, class, range)| (HEIGHT * tf.scale.x, class.copied(), range.cloned()))
            .collect();
        assert_eq!(copies.len(), JITTERED as usize);
        let mut seen = HashMap::new();
        for (size, class, range) in &copies {
            // A copy a hair from a class edge could fall either side of it
            // in f32: it proves nothing either way.
            if [cfg::SMALL_MAX_M, cfg::MEDIUM_MAX_M]
                .iter()
                .any(|edge| (size - edge).abs() < 1.0e-3)
            {
                continue;
            }
            assert_eq!(*class, SizeClass::of(*size), "a {size} m copy");
            let want = class.map(|c| match c {
                SizeClass::Small => cfg::DEFAULT_M,
                SizeClass::Medium => 2.0 * cfg::DEFAULT_M,
            });
            assert_eq!(
                range.as_ref().map(|r| r.end_margin.start),
                want,
                "a {size} m copy"
            );
            *seen.entry(*class).or_insert(0) += 1;
        }
        // Non-vacuous: the jitter drew copies into every class.
        for class in [Some(SizeClass::Small), Some(SizeClass::Medium), None] {
            assert!(
                seen.get(&class).copied().unwrap_or(0) > 0,
                "no {class:?} copy in {seen:?}"
            );
        }
    }

    /// Behind the login screen nothing is cut, and logging in restamps
    /// every part the demo world built.
    #[test]
    fn the_login_backdrop_is_not_cut_and_login_restamps_it() {
        let mut app = app();
        app.insert_resource(crate::attract::AttractScene);
        settle(&mut app);
        let demo = parts(&mut app);
        assert!(
            demo.iter().any(|p| p.class == Some(SizeClass::Small)),
            "the demo world has ground cover"
        );
        for p in &demo {
            assert!(p.range.is_none(), "{} is cut behind the login", p.generator);
        }

        app.world_mut()
            .remove_resource::<crate::attract::AttractScene>();
        app.update();
        let game = parts(&mut app);
        for p in &game {
            let want = p.class.map(|c| match c {
                SizeClass::Small => cfg::DEFAULT_M,
                SizeClass::Medium => 2.0 * cfg::DEFAULT_M,
            });
            assert_eq!(end(p), want, "{}", p.generator);
        }
    }

    /// The generator is sized once a job, on its first copy, in its own
    /// frame: the scatter's per-copy yaw and scale do not grow the box.
    #[test]
    fn a_generator_is_measured_once_in_its_own_frame() {
        let mut meshes = Assets::<Mesh>::default();
        let mesh = meshes.add(Cuboid::new(1.9, 1.9, 1.9));
        let mut recorder = CopyRecorder::default();
        let world = World::new();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        // A copy turned 45 degrees and grown by half: its world box is 4 m
        // wide, but the generator is 1.9 m.
        let yawed = Transform::from_xyz(5.0, 0.0, 5.0)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4))
            .with_scale(Vec3::splat(1.5));
        recorder.begin_copy("cube", &yawed);
        let saved = recorder.enter_node(&yawed);
        recorder.note(
            Entity::from_raw_u32(1).unwrap(),
            &mesh,
            &Transform::IDENTITY,
            &meshes,
        );
        recorder.leave_node(saved);
        recorder.end_copy("cube", &mut commands, &DrawDistanceCuts::default());
        let measured = |r: &CopyRecorder| r.size_of("cube").flatten().expect("measured");
        assert!(
            (measured(&recorder) - 1.9).abs() < 1.0e-4,
            "{}",
            measured(&recorder)
        );

        // The second copy is not measured: a bigger mesh changes nothing.
        let big = meshes.add(Cuboid::new(9.0, 9.0, 9.0));
        recorder.begin_copy("cube", &Transform::IDENTITY);
        recorder.note(
            Entity::from_raw_u32(2).unwrap(),
            &big,
            &Transform::IDENTITY,
            &meshes,
        );
        recorder.end_copy("cube", &mut commands, &DrawDistanceCuts::default());
        assert!(
            (measured(&recorder) - 1.9).abs() < 1.0e-4,
            "{}",
            measured(&recorder)
        );

        // Outside a copy nothing is recorded at all.
        recorder.note(
            Entity::from_raw_u32(3).unwrap(),
            &big,
            &Transform::IDENTITY,
            &meshes,
        );
        assert!(recorder.parts.is_empty());
    }

    /// Each copy is classed by what IT draws: its generator's size times its
    /// own scatter scale. The moss of the Understory is 1.81 m, and its
    /// scatter's `scale_jitter` draws copies up to 2.44 m; a jitter at the
    /// sanitizer's 1.5 grows a 1.9 m plant to 8.5 m - no ground cover.
    #[test]
    fn a_copy_is_classed_by_its_own_scale() {
        let mut meshes = Assets::<Mesh>::default();
        let mesh = meshes.add(Cuboid::new(1.9, 1.9, 1.9));
        let mut recorder = CopyRecorder::default();
        let mut world = World::new();
        let cuts = DrawDistanceCuts::resolve(cfg::DEFAULT_M, 10_000.0);
        let small = Some(cfg::DEFAULT_M);
        let medium = Some(2.0 * cfg::DEFAULT_M);
        // The first copy is grown by half, and measured at the generator's
        // own 1.9 m all the same.
        let want = [
            (1.5, Some(SizeClass::Medium), medium),
            (1.0, Some(SizeClass::Small), small),
            (0.5, Some(SizeClass::Small), small),
            (2.1, Some(SizeClass::Medium), medium),
            (2.2, None, None),
            (1.5f32.exp(), None, None),
        ];
        for (scale, class, cut) in want {
            let cell = Transform::from_xyz(3.0, 0.0, -2.0)
                .with_rotation(Quat::from_rotation_y(0.7))
                .with_scale(Vec3::splat(scale));
            let part = world.spawn_empty().id();
            let mut queue = bevy::ecs::world::CommandQueue::default();
            {
                let mut commands = Commands::new(&mut queue, &world);
                recorder.begin_copy("moss", &cell);
                let saved = recorder.enter_node(&cell);
                recorder.note(part, &mesh, &Transform::IDENTITY, &meshes);
                recorder.leave_node(saved);
                recorder.end_copy("moss", &mut commands, &cuts);
            }
            queue.apply(&mut world);
            assert_eq!(world.get::<SizeClass>(part).copied(), class, "x{scale}");
            let end = world
                .get::<VisibilityRange>(part)
                .map(|r| r.end_margin.start);
            assert_eq!(end, cut, "x{scale}");
        }
    }
}
