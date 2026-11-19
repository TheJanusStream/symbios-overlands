//! The drawing of a node's light source (#1921): each authored
//! [`LightSource`] in the room a shadowless Bevy light, and of them only the
//! nearest few to the world camera drawn at once, faded in and out.
//!
//! # A light at its node
//!
//! The spawner ([`attach`]) gives each lit node of the room a child entity
//! carrying a [`PointLight`] or a [`SpotLight`] at the light's offset (a
//! spot turned to its aim), and a [`NodeLight`] with what the budget needs.
//! A child of the node, it moves, turns and goes with it: a node that spins
//! carries its light round, and one despawned takes its light along.
//!
//! How strong ([`lumens`]): a light's `brightness` is a share of the
//! reference lamp's, the investigation's prototype on the helipad (#1919),
//! which reads at night as a lamp does: what stands at four fifths of the
//! light's `range` is lit to [`FLOOR_LX`] at brightness 1 - so the power is
//! `brightness x 4 pi x 40 lx x (0.8 range)^2` lumens, and a fresh light
//! (5.6 m) is the prototype's lamp: 10,089 lm (its 10,179 at a 4.5 m
//! reach, which the record's 5.6 m range puts at 4.48). Bevy's range is the
//! record's, where its window takes the light to nothing. A spot is as
//! strong as a point light of the same power in its cone, as Bevy draws it.
//! No light casts shadows: WebGL2 holds one point light's shadow at most,
//! and the terrain already binds 15 of its 16 textures.
//!
//! A room's parts are lit, and so are an avatar's body and what it wears
//! (#1924), the local player's and every peer's ([`attach_to_body`]): a
//! vehicle's headlights and navigation lights rank in the same budget and
//! follow the same dark as a lamp post. Their seeds are read from whose
//! body it is, which of their trees (the body or a worn prop) and the
//! node's place in it ([`avatar_light_seed`]). An item's preview spawns
//! without lights,
//! as an instance set's template does: a preview's light would light the
//! world round the player as well as its picture, and a template's copies
//! are drawn merged, with no node of their own to light from.
//!
//! # The budget
//!
//! WebGL2 keeps at most 204 point and spot lights in view a frame, and past
//! that drops them by entity, not distance; near the camera the tighter
//! limit is the list of each cluster's lights, 16,384 places over the
//! screen's 4,096 clusters, which 128 overlapping lamps round a player's
//! eye overflowed in the investigation (blocks of the screen unlit). So
//! [`follow_node_lights`] draws only the nearest lights to the world camera,
//! the same number on every bundle - [`budget()`]: 64 at Effects Full, 32
//! at Reduced, none at Off - and only as many of them as the list holds,
//! counted as Bevy counts it ([`clusters`]): lights that reach far take
//! more of the list, so fewer are drawn while they are near. The lights of
//! one body, or of one thing it wears, go together (#1925): a car near the
//! end of the count shows both its headlights or neither, never one.
//!
//! Which ([`mod@budget`]): the lights in the dark enough to show,
//! nearest first, a light already wanted ranked as if a share nearer, so
//! two at nearly the same distance do not trade places every frame. A
//! light is let in under one line of the list, kept under a higher one and
//! put out at once only past a third, so the camera's sway and slow turns
//! fade lights in and out, and only a camera turned toward more light than
//! the list holds cuts one. Each light fades in and out over [`FADE_S`],
//! and a light at nothing is hidden, so it is no clusterable object at all.
//! A light respawned with its node - its item compiled again, as every
//! edit to it does - takes up the fade of the light it replaces ([`Kept`])
//! rather than going dark and fading in again: within [`KEPT_S`] of it,
//! within [`KEPT_REACH_M`] of where it stood and reaching no further, so it
//! costs the list no more than the light whose place it takes. Where an
//! avatar's light stands is read in the frame of the body or prop it hangs
//! on ([`BodyLight`]), not in the world: a craft respawned at speed has
//! moved on a metre in the frame between, and its lights with it. A light
//! respawned reaching further comes in as a newcomer, under the admitting
//! line.
//!
//! # Dusk to dawn, flicker and pulse
//!
//! A light lit from dusk to dawn is as far on as the materials' glow of
//! that name: `WorldWeather.uniforms.lights`, 0 by day and 1 in the dark,
//! from the drawn world's hour and sky. A light that is off takes no place
//! in the budget.
//!
//! A flicker or pulse ([`flicker_share`]) is a function of the shared clock
//! (the spinners' [`SpinClock`] - UTC in the game) and the light's own seed,
//! drawn from its place in the record ([`light_seed`]), so every peer sees
//! one light do the same thing. The sanitiser holds one that dips deep
//! enough to flash to three flashes a second at every setting, WCAG 2.3.1's
//! limit (`sanitize::light::max_flicker_rate_hz`); a flicker's shimmer too
//! shallow to flash may move on a rate of its own, up to the ceiling, over a
//! waver held to that (#1950). The viewer's Effects setting calms it
//! further at the drawing ([`calmed`]): at Reduced half as deep, with no
//! fine shimmer and at most once a second; steady at Off.

pub mod budget;
pub mod clusters;

use bevy::prelude::*;

use crate::camera::WorldCamera;
use crate::pds::light::{
    FLICKER_FINE_RATE, FLICKER_FINE_SHARE, LightFlicker, LightShape, LightSource,
};
use crate::state::EffectsIntensity;
use crate::weathered::WorldWeather;

use super::spin::SpinClock;
pub use budget::{Planner, Slot, Verdict, plan};
pub use clusters::{ClusterView, Cone};

/// The illuminance (lx) a light of brightness 1 gives at four fifths of its
/// range - a lamp's working light, the faintest still seen beside the
/// darkest moon (#1919).
pub(crate) const FLOOR_LX: f32 = 40.0;

/// The share of its range a light gives [`FLOOR_LX`] at: Bevy's window
/// takes a light to nothing AT its range, so its working reach stands a
/// little inside.
const REACH_SHARE: f32 = 0.8;

/// The most lights drawn at once at Effects Full (the nearest), when the
/// cluster index list holds them: 128 lamps round a player's eye overflowed
/// it in the investigation, 64 spaced at their reach never do.
pub(crate) const FULL_BUDGET: usize = 64;

/// The most at Effects Reduced: half, which halves each lit pixel's light
/// loop on a weak machine and still lights the lamps round a player - the
/// investigation's census found at most 35 lamp-sized glowing parts within
/// 50 m of the camera in any seeded world but one.
pub(crate) const REDUCED_BUDGET: usize = 32;

/// How long a light takes to fade in or out of the budget (s).
pub(crate) const FADE_S: f32 = 0.6;

/// At Effects Reduced a flicker or pulse is this deep a share of what was
/// authored...
const REDUCED_FLICKER_DEPTH: f32 = 0.5;

/// ...and moves at most this many times a second (a flicker without its
/// quicker fine shimmer).
const REDUCED_FLICKER_RATE_HZ: f32 = 1.0;

/// One authored light, on the entity of its Bevy light: what the budget and
/// the drawing need of it, and how far into the budget it is faded.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct NodeLight {
    /// Full power (lm).
    pub lumens: f32,
    /// Bevy's range (m) - what the budget's count reads.
    pub range: f32,
    pub dusk_to_dawn: bool,
    pub flicker: Option<Flicker>,
    /// The light's own seed - see [`light_seed`].
    pub seed: u64,
    /// How far faded in, 0 to 1.
    pub level: f32,
    /// Whether the budget wants it drawn.
    pub held: bool,
}

/// A flicker or pulse in the drawing's units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Flicker {
    /// `fine`: with the quicker shimmer over the broad waver (see
    /// [`LightFlicker::Flicker`]); the Effects setting's Reduced drops it.
    /// `shimmer_rate`: the shimmer's own rate (Hz), where the record gives
    /// it one; `None` moves it [`FLICKER_FINE_RATE`] times quicker than the
    /// waver, as before #1950.
    Flicker {
        amount: f32,
        rate: f32,
        fine: bool,
        shimmer_rate: Option<f32>,
    },
    Pulse {
        amount: f32,
        rate: f32,
    },
}

impl Flicker {
    /// The record's flicker, or `None` for an unknown mode (drawn steady).
    pub fn of(flicker: &LightFlicker) -> Option<Self> {
        match *flicker {
            LightFlicker::Flicker {
                amount,
                rate,
                shimmer_rate,
            } => Some(Self::Flicker {
                amount: amount.0,
                rate: rate.0,
                fine: true,
                shimmer_rate: shimmer_rate.map(|s| s.0),
            }),
            LightFlicker::Pulse { amount, rate } => Some(Self::Pulse {
                amount: amount.0,
                rate: rate.0,
            }),
            LightFlicker::Unknown => None,
        }
    }
}

/// Register the drawing's system. After the spinners' registration: it
/// reads their clock.
///
/// It runs where Bevy's clustering will read what it writes: after the
/// camera and every light have their pose for the frame (propagation, and
/// the camera's lens), and before the visibility a light's `Visibility`
/// feeds - so the budget counts the very view and lights Bevy clusters.
pub(crate) fn register(app: &mut App) {
    app.add_systems(
        PostUpdate,
        follow_node_lights
            .after(bevy::transform::TransformSystems::Propagate)
            .after(bevy::camera::CameraUpdateSystems)
            .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
    );
}

/// The power (lm) of a light of `brightness` and `range` - see the module
/// docs.
pub fn lumens(brightness: f32, range: f32) -> f32 {
    let reach = range * REACH_SHARE;
    brightness * 4.0 * std::f32::consts::PI * FLOOR_LX * reach * reach
}

/// The seed of the light on the node at `path` of the generator `base_ref`,
/// in the copy whose spinners share `epoch`, on the `layer` of a grid's
/// stacked copies it stands on (0 for every other placement) - all of it
/// read from the record, so every peer derives the same one, and two
/// copies of a placed lamp flicker apart. The layer is the grid's own
/// count, not the copy's height: a copy's epoch reads only where it
/// stands across the ground (a grid snapped to the terrain puts its
/// copies at heights each client works out for itself), so the copies of
/// one column share it, and the spinners' clocks are left as they are.
pub(crate) fn light_seed(base_ref: &str, path: &[usize], epoch: f64, layer: u32) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |byte: u8| {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    base_ref.bytes().for_each(&mut eat);
    for &i in path {
        eat(b'/');
        (i as u64).to_le_bytes().into_iter().for_each(&mut eat);
    }
    epoch.to_bits().to_le_bytes().into_iter().for_each(&mut eat);
    layer.to_le_bytes().into_iter().for_each(&mut eat);
    splitmix64(h)
}

fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Whose avatar tree a light hangs on, and which of their trees (#1924):
/// what [`avatar_light_seed`] salts a seed with. Every peer knows both for a
/// body it draws, so they agree on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightOwner<'a> {
    /// The owner's DID; `None` where none is known (a guest, a tree the
    /// render tool stands in the world).
    pub did: Option<&'a str>,
    /// The worn prop the tree is, by its attachment record's key; `None`
    /// for the body.
    pub prop: Option<&'a str>,
}

impl<'a> LightOwner<'a> {
    /// The body of the avatar of `did`.
    pub fn body(did: Option<&'a str>) -> Self {
        Self { did, prop: None }
    }
}

/// The seed of the light on the node at `path` of an avatar's body or of
/// something it wears (#1924), salted with `owner`: read from the owner's
/// DID, the tree and the node's place in it, as a room light's is from the
/// record, so every peer derives the same one for a body it draws - the
/// spawn's own key names the chassis entity, which each client numbers for
/// itself - and a body respawned after an edit keeps its lights' seeds, so
/// they take up their fades ([`Kept`]). Two players' craft built alike
/// flicker apart, and so do a body's light and a worn prop's at one path.
/// With no DID and no prop it is the seed of the tree alone.
pub(crate) fn avatar_light_seed(owner: LightOwner<'_>, path: &[usize]) -> u64 {
    // The markers keep the parts apart: neither a DID nor a record key
    // holds a `/`.
    let mut base = String::from(AVATAR_LIGHT_REF);
    if let Some(did) = owner.did {
        base.push_str("/did/");
        base.push_str(did);
    }
    if let Some(prop) = owner.prop {
        base.push_str("/prop/");
        base.push_str(prop);
    }
    light_seed(&base, path, 0.0, 0)
}

/// What an avatar's light seeds are drawn under, in place of a room
/// generator's reference - see [`avatar_light_seed`].
const AVATAR_LIGHT_REF: &str = "avatar";

/// On the light of an avatar's node (#1924): the entity its tree hangs
/// under - the chassis of a body, the root of a worn prop - in whose frame
/// [`Kept`] reads where the light stands, so a body respawned while it
/// moves takes up its lights' fades.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BodyLight {
    pub frame: Entity,
}

/// Give the node `node` of a room the light `light` (a no-op for one that
/// does not shine): a hidden child, at nothing until the budget fades it
/// in.
pub(crate) fn attach(commands: &mut Commands, node: Entity, light: &LightSource, seed: u64) {
    spawn_light(commands, node, light, seed, None);
}

/// [`attach`] for a node of an avatar's body or of what it wears (#1924),
/// whose tree hangs under `frame` ([`BodyLight`]). Its light lights the
/// world from wherever the body is drawn: a body the first-person view
/// hides is moved to a render layer of its own (a `Propagate<RenderLayers>`
/// on the chassis), and Bevy lights a camera only with the lights of its
/// layers, so the light stays out of that propagation - a car's headlights
/// light the road ahead of the eyes inside it.
pub(crate) fn attach_to_body(
    commands: &mut Commands,
    node: Entity,
    light: &LightSource,
    seed: u64,
    frame: Entity,
) {
    spawn_light(commands, node, light, seed, Some(frame));
}

fn spawn_light(
    commands: &mut Commands,
    node: Entity,
    light: &LightSource,
    seed: u64,
    body: Option<Entity>,
) {
    if !light.shines() {
        return;
    }
    let [r, g, b] = light.color.0;
    let color = Color::srgb(r, g, b);
    let range = light.range.0;
    let radius = light.size.0;
    let offset = Vec3::from(light.offset.0);
    let state = NodeLight {
        lumens: lumens(light.brightness.0, range),
        range,
        dusk_to_dawn: light.dusk_to_dawn,
        flicker: light.flicker.as_ref().and_then(Flicker::of),
        seed,
        level: 0.0,
        held: false,
    };
    let spawned = match &light.shape {
        LightShape::Point => commands
            .spawn((
                PointLight {
                    color,
                    intensity: 0.0,
                    range,
                    radius,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_translation(offset),
                Visibility::Hidden,
                state,
                ChildOf(node),
            ))
            .id(),
        LightShape::Spot { aim, inner, outer } => commands
            .spawn((
                SpotLight {
                    color,
                    intensity: 0.0,
                    range,
                    radius,
                    shadow_maps_enabled: false,
                    inner_angle: inner.0.to_radians(),
                    outer_angle: outer.0.to_radians(),
                    ..default()
                },
                spot_transform(offset, Vec3::from(aim.0)),
                Visibility::Hidden,
                state,
                ChildOf(node),
            ))
            .id(),
        LightShape::Unknown => return,
    };
    if let Some(frame) = body {
        commands.entity(spawned).insert((
            BodyLight { frame },
            bevy::app::PropagateOver::<bevy::camera::visibility::RenderLayers>::default(),
        ));
    }
}

/// A spot's pose at `offset`, shining along `aim` (a Bevy spot shines down
/// its -Z); a zero aim shines down.
fn spot_transform(offset: Vec3, aim: Vec3) -> Transform {
    let aim = Dir3::new(aim).unwrap_or(Dir3::NEG_Y);
    let up = if aim.y.abs() > 0.99 { Dir3::Z } else { Dir3::Y };
    Transform::from_translation(offset).looking_to(aim, up)
}

/// The most lights drawn at once at `effects`.
pub fn budget(effects: EffectsIntensity) -> usize {
    match effects {
        EffectsIntensity::Full => FULL_BUDGET,
        EffectsIntensity::Reduced => REDUCED_BUDGET,
        EffectsIntensity::Off => 0,
    }
}

/// `flicker` as `effects` draws it: as authored at Full; at Reduced half as
/// deep, at most once a second and a flicker without its fine shimmer, so
/// nothing in it moves faster; `None` (steady) at Off.
pub fn calmed(flicker: Flicker, effects: EffectsIntensity) -> Option<Flicker> {
    let depth = |amount: f32| amount * REDUCED_FLICKER_DEPTH;
    let pace = |rate: f32| rate.min(REDUCED_FLICKER_RATE_HZ);
    match (effects, flicker) {
        (EffectsIntensity::Full, f) => Some(f),
        (EffectsIntensity::Reduced, Flicker::Flicker { amount, rate, .. }) => {
            Some(Flicker::Flicker {
                amount: depth(amount),
                rate: pace(rate),
                fine: false,
                shimmer_rate: None,
            })
        }
        (EffectsIntensity::Reduced, Flicker::Pulse { amount, rate }) => Some(Flicker::Pulse {
            amount: depth(amount),
            rate: pace(rate),
        }),
        (EffectsIntensity::Off, _) => None,
    }
}

/// The share of its power a light with `flicker` and `seed` gives at the
/// shared clock's `now` (s): 1 less up to the flicker's amount. A pulse
/// breathes on a cosine; a flicker wavers along a smooth random walk, with
/// a quicker one over it for its fine shimmer - on its own rate where it
/// has one, else [`FLICKER_FINE_RATE`] times the waver's. Worked in `f64`
/// where the clock is: at UTC's 1.8e9 s an `f32` cannot tell two minutes
/// apart.
pub fn flicker_share(flicker: Flicker, seed: u64, now: f64) -> f32 {
    let phase = (seed >> 11) as f64 / (1u64 << 53) as f64;
    let dip = match flicker {
        Flicker::Pulse { amount, rate } => {
            let turn = (now * f64::from(rate) + phase).rem_euclid(1.0);
            amount * (0.5 - 0.5 * (std::f64::consts::TAU * turn).cos() as f32)
        }
        Flicker::Flicker {
            amount,
            rate,
            fine,
            shimmer_rate,
        } => {
            let at = now * f64::from(rate) + phase * 1000.0;
            let broad = value_noise(seed, at);
            if fine {
                // Coupled, exactly as before #1950: a record without an own
                // shimmer rate draws the same light.
                let quick = match shimmer_rate {
                    Some(own) => {
                        now * f64::from(own) + phase * 1000.0 * f64::from(FLICKER_FINE_RATE) + 0.5
                    }
                    None => at * f64::from(FLICKER_FINE_RATE) + 0.5,
                };
                let shimmer = value_noise(seed ^ 0x5DEE_CE66, quick);
                amount * ((1.0 - FLICKER_FINE_SHARE) * broad + FLICKER_FINE_SHARE * shimmer)
            } else {
                amount * broad
            }
        }
    };
    (1.0 - dip).clamp(0.0, 1.0)
}

/// A smooth random walk through `[0, 1]`, a new value at each whole `at`.
fn value_noise(seed: u64, at: f64) -> f32 {
    let step = at.floor();
    let t = (at - step) as f32;
    let value = |i: i64| (splitmix64(seed ^ (i as u64)) >> 40) as f32 / (1u64 << 24) as f32;
    let (a, b) = (value(step as i64), value(step as i64 + 1));
    let s = t * t * (3.0 - 2.0 * t);
    a + (b - a) * s
}

/// `level` one frame of `dt` seconds on toward `target`, at a whole fade
/// each [`FADE_S`].
pub fn faded(level: f32, target: bool, dt: f32) -> f32 {
    let step = dt / FADE_S;
    if target {
        (level + step).min(1.0)
    } else {
        (level - step).max(0.0)
    }
}

/// Keep every authored light with the budget, the dark and its flicker: see
/// the module docs. Writes only what changed.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn follow_node_lights(
    time: Res<Time>,
    real: Res<Time<Real>>,
    clock: Option<ResMut<SpinClock>>,
    weather: Option<Res<WorldWeather>>,
    settings: Option<Res<crate::state::LocalSettings>>,
    camera: Query<(&Camera, &GlobalTransform, Option<&Projection>), With<WorldCamera>>,
    parents: Query<&InheritedVisibility>,
    frames: Query<&GlobalTransform>,
    mut lights: Query<(
        Entity,
        &GlobalTransform,
        &ChildOf,
        &mut NodeLight,
        Option<&mut PointLight>,
        Option<&mut SpotLight>,
        &mut Visibility,
        Option<&BodyLight>,
    )>,
    mut slots: Local<Vec<Slot>>,
    mut order: Local<Vec<Entity>>,
    mut lit_after: Local<Vec<bool>>,
    mut planner: Local<Planner>,
    mut clusters: Local<ClusterView>,
    mut kept: Local<Kept>,
) {
    let stamp = real.elapsed_secs_f64();
    // A light gone longer than a respawn takes is gone for good - also
    // while no light is left at all, so a world left and come back to
    // later fades its lights in again.
    kept.0.retain(|_, k| stamp - k.seen <= KEPT_S);
    if lights.is_empty() {
        return;
    }
    // A light respawned with its node - its item compiled again after an
    // edit, the owner's or one a peer received - takes up where the one
    // it replaces left off, rather than fading in from nothing: where it
    // stands where that one stood, and costs the cluster list no more. One
    // that reaches further comes in as any newcomer does, under the
    // budget's admitting line, so a Reach dragged out does not crowd out
    // the lights round it; and a light elsewhere with the same seed (a
    // sibling deleted, a placement moved) is not the one that went. An
    // avatar's light is placed in its body's frame, which has moved on with
    // a body respawned at speed.
    let place = |at: &GlobalTransform, body: Option<&BodyLight>| match body {
        Some(body) => frames
            .get(body.frame)
            .ok()
            .map(|frame| Place::Body(frame.affine().inverse().transform_point3(at.translation()))),
        None => Some(Place::World(at.translation())),
    };
    for (entity, at, _, mut light, _, _, _, body) in lights.iter_mut() {
        if light.is_added()
            && light.level == 0.0
            && let Some(gone) = kept.0.get(&light.seed)
            && gone.entity != entity
            && light.range <= gone.range
            && place(at, body).is_some_and(|here| here.near(gone.at))
        {
            light.level = gone.level;
            light.held = gone.held;
        }
    }
    let effects = settings.map_or(EffectsIntensity::Full, |s| s.effects_intensity);
    let night = weather.map_or(0.0, |w| w.uniforms.lights);
    let to_view = camera.single().ok().map(|(camera, eye, projection)| {
        // The lens Bevy clusters with: the camera's own, on its screen (a
        // 1280 x 720 one of the projection's before the camera has one).
        let fallback = Projection::default();
        let projection = projection.unwrap_or(&fallback);
        match camera.physical_viewport_size() {
            Some(screen) if screen.x > 0 && screen.y > 0 => {
                clusters.set(camera.clip_from_view(), projection.far(), screen);
            }
            _ => clusters.set_projection(projection, UVec2::new(1280, 720)),
        }
        clusters.set_eye(eye);
        // Bevy's view-from-world, which it reads a light's centre and a
        // spot's way through.
        Mat4::from(eye.affine().inverse())
    });
    let now = clock.map(|mut clock| clock.now(&time, &real));
    let on = |light: &NodeLight| if light.dusk_to_dawn { night } else { 1.0 };

    slots.clear();
    order.clear();
    slots.extend(
        lights
            .iter()
            .map(|(entity, at, parent, light, _, spot, _, body)| {
                order.push(entity);
                let shown = parents.get(parent.parent()).is_ok_and(|v| v.get());
                let lit = on(light) > 0.0;
                Slot {
                    view: to_view.map_or(Vec3::splat(f32::INFINITY), |to_view| {
                        (to_view * at.translation().extend(1.0)).truncate()
                    }),
                    range: light.range,
                    cone: to_view
                        .zip(spot)
                        .map(|(to_view, spot)| Cone::new(&to_view, *at.back(), spot.outer_angle)),
                    // A body's lights, and a worn prop's, go together.
                    group: body.map(|body| body.frame.to_bits()),
                    eligible: lit && shown && to_view.is_some(),
                    held: light.held,
                    drawn: lit && shown && light.level > 0.0,
                }
            }),
    );
    let verdicts = planner.plan(&slots, budget(effects), &clusters);

    let dt = time.delta_secs();
    lit_after.clear();
    lit_after.resize(slots.len(), false);
    for (k, (&entity, &verdict)) in order.iter().zip(verdicts).enumerate() {
        let Ok((_, at, _, mut light, point, spot, mut visibility, body)) = lights.get_mut(entity)
        else {
            continue;
        };
        let was = light.level;
        let want = verdict == Verdict::Draw;
        if light.held != want {
            light.held = want;
        }
        let level = match verdict {
            Verdict::Cut => 0.0,
            Verdict::Hurry => faded(light.level, false, dt * FADE_S / budget::HURRY_FADE_S),
            Verdict::Draw | Verdict::Fade => faded(light.level, want, dt),
        };
        if light.level != level {
            light.level = level;
        }
        lit_after[k] = slots[k].eligible && level > 0.0;
        // Only a light drawn this frame or the last touches the map: the
        // many waiting at nothing cost no lookup.
        if level > 0.0
            && let Some(at) = place(at, body)
        {
            kept.0.insert(
                light.seed,
                KeptLight {
                    entity,
                    level,
                    held: want,
                    range: light.range,
                    at,
                    seen: stamp,
                },
            );
        } else if was > 0.0 && kept.0.get(&light.seed).is_some_and(|k| k.entity == entity) {
            kept.0.remove(&light.seed);
        }
        let share = match (light.flicker.and_then(|f| calmed(f, effects)), now) {
            (Some(flicker), Some(now)) => flicker_share(flicker, light.seed, now),
            _ => 1.0,
        };
        let intensity = light.lumens * on(&light) * level * share;
        if let Some(mut point) = point
            && point.intensity != intensity
        {
            point.intensity = intensity;
        }
        if let Some(mut spot) = spot
            && spot.intensity != intensity
        {
            spot.intensity = intensity;
        }
        // Hidden at nothing, so it is no clusterable object - but not in a
        // flicker's trough, so Bevy clusters what the budget counted.
        let shown = if level > 0.0 && on(&light) > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != shown {
            *visibility = shown;
        }
    }
    // Bevy ends next frame's slices at the lights it clusters this frame:
    // those still lit once faded.
    planner.settle(&slots, &clusters, |k| lit_after[k]);
}

/// How long a light that has gone keeps its place for one respawned in
/// its stead (s): a compile spreads an item over a few frames, and its
/// old entities go before or after the new ones come.
pub(crate) const KEPT_S: f64 = 2.0;

/// How far from where the light that went last stood one respawned in its
/// stead may stand and still take up its fade (m): an edit that leaves
/// the light where it was, or moves it a hair. A light further off is
/// another light, or one moved far enough to cost the list differently.
/// Measured in the world for a room's light, and in its body's frame for
/// an avatar's ([`BodyLight`]).
pub(crate) const KEPT_REACH_M: f32 = 0.5;

/// Where a light stands, for [`Kept`]: in the world, or in the frame of
/// the body or prop it hangs on ([`BodyLight`]). Lights of the two kinds
/// never take up each other's fades.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    World(Vec3),
    Body(Vec3),
}

impl Place {
    /// Whether a light standing here stands within [`KEPT_REACH_M`] of one
    /// that stood at `other`.
    fn near(self, other: Place) -> bool {
        match (self, other) {
            (Place::World(a), Place::World(b)) | (Place::Body(a), Place::Body(b)) => {
                a.distance_squared(b) <= KEPT_REACH_M * KEPT_REACH_M
            }
            _ => false,
        }
    }
}

/// The lights drawn or fading at the last frame they were seen, by seed
/// (see [`light_seed`]): what a light respawned at the same node of the
/// same copy takes up, so a recompiled lamp does not go dark and fade in
/// again - the World Editor's every edit recompiles its item, for the
/// owner and for every visitor.
#[derive(Default)]
pub(crate) struct Kept(bevy::platform::collections::HashMap<u64, KeptLight>);

/// One light in [`Kept`].
struct KeptLight {
    entity: Entity,
    level: f32,
    held: bool,
    /// Its range (m): one respawned reaching further does not take over.
    range: f32,
    /// Where it stood, in the world or in its body's frame.
    at: Place,
    /// When last seen, on the real clock (s).
    seen: f64,
}

#[cfg(test)]
mod tests;
