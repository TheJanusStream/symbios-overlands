//! A storm's lightning and thunder (#1684).
//!
//! Strikes fall at instants every visitor reads off the clock (UTC): the
//! clock is cut into slots of [`cfg::SLOT_SECS`], and a slot strikes by a
//! chance of [`cfg::STRIKE_SHARE`] of the storm's amount at the slot's own
//! start, at an instant drawn from its count and the weather's own salt
//! (`weather::salt`) - so two rooms set alike storm alike, and every visitor
//! sees each flash at once, as nearly as their clocks agree ([`strike_of`]).
//! A strike falls somewhere round the room's middle, the nearer the rarer
//! and the brighter, and flashes: a pulse, and for most a second, dimmer
//! one.
//!
//! Never more than two flashes in any second, under the three a second at
//! which flashing light can set off a seizure: two pulses a strike, and no
//! strike drawn within [`cfg::SLOT_TAIL_SECS`] of the last one drawn. The
//! schedule keeps that spacing by itself; the drawing holds it whatever
//! the schedule does - an owner dragging the weather's sliders through a
//! storm redraws the schedule every frame, and the strike drawn is then
//! the last one that was, or one far enough after it. A visitor for whom
//! flashing is a problem turns it down with the Effects setting (#1687):
//! Reduced draws a third of each flash's light and of the stops it opens
//! the exposure by, Off none, and the thunder sounds either way
//! ([`crate::state::EffectsIntensity::flash_scale`]).
//!
//! Lightning adds light: the flash adds a cold white to the sky's colour -
//! and so to the clouds', the water's and the rain's light from the sky,
//! which read it too, the clouds glowing with it - and to the fog's, which
//! a storm's haze is mostly made of; and it opens the camera's exposure by
//! as much as its light multiplies the key light's (under the Effects
//! setting's Full; its share of those stops under Reduced), so all the
//! world flashes, the more so by night. The colours are material writes, made
//! only while a pulse is lit and only where a colour differs: a few frames
//! a strike. Thunder follows each strike drawn as far behind it as sound
//! takes to cross its distance, quieter the farther off, with a crack
//! before the rumble when it is near - two clips, each synthesised the
//! first time it is needed, at most [`MAX_THUNDER`] playing at once,
//! spawned muted under the master mute (`crate::audio_mute`).

use std::sync::Arc;

use bevy::audio::{AudioPlayer, AudioSource, PlaybackMode, PlaybackSettings, Volume};
use bevy::camera::Exposure;
use bevy::pbr::DistanceFog;
use bevy::prelude::*;

use super::{splitmix64, storm_at};
use crate::clouds::{CloudLayer, CloudMaterial};
use crate::config::lighting::weather::storm as cfg;
use crate::drawn_environment::{DrawEnvironmentSet, DrawnEnvironment, unix_now};
use crate::pds::Weather;
use crate::sky::{SkyBox, SkyMaterial};
use crate::state::LiveRoomRecord;

/// One strike of lightning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    /// When it strikes (s after 1970 began, UTC).
    pub at: f64,
    /// Its bearing from the room's middle (rad, from world +X toward +Z).
    pub bearing: f32,
    /// Its distance from the room's middle (m).
    pub distance: f32,
    /// Its flash at its brightest, 0 to 1: the nearer, the brighter.
    pub light: f32,
    /// A second, dimmer pulse: how long after the first (s), and its share
    /// of the first's light.
    pub second: Option<(f64, f32)>,
}

/// A value from 0 to 1 out of a hash.
fn unit(x: u64) -> f32 {
    (x >> 40) as f32 / (1u64 << 24) as f32
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// The strike in slot `slot` of the clock (slots of [`cfg::SLOT_SECS`]
/// since 1970) under a storm of `storm` (0 to 1), for a weather salted
/// `salt`; none if the slot does not strike.
pub fn strike_in(slot: i64, storm: f32, salt: u64) -> Option<Strike> {
    let draw = |k: u64| {
        unit(splitmix64(
            salt ^ (slot as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ k.wrapping_mul(0xD1B5_4A32_D192_ED03),
        ))
    };
    if storm.is_nan() || draw(1) >= storm * cfg::STRIKE_SHARE {
        return None;
    }
    let at =
        slot as f64 * cfg::SLOT_SECS + f64::from(draw(2)) * (cfg::SLOT_SECS - cfg::SLOT_TAIL_SECS);
    // The nearer, the rarer: a share drawn toward the far end.
    let far = draw(3).sqrt();
    let distance = lerp(cfg::DISTANCE.0, cfg::DISTANCE.1, far);
    let second = (draw(5) < cfg::SECOND_SHARE).then(|| {
        (
            cfg::SECOND_AFTER.0 + f64::from(draw(6)) * (cfg::SECOND_AFTER.1 - cfg::SECOND_AFTER.0),
            lerp(cfg::SECOND_LIGHT.0, cfg::SECOND_LIGHT.1, draw(7)),
        )
    });
    Some(Strike {
        at,
        bearing: draw(4) * std::f32::consts::TAU,
        distance,
        light: lerp(1.0, cfg::FAR_FLASH, far),
        second,
    })
}

/// The strike in slot `slot` of `weather`: by the storm the weather brings
/// at the slot's start, so every visitor decides it alike, and a storm
/// drawn a step apart by two of them cannot tell them apart on a strike.
pub fn strike_of(weather: &Weather, slot: i64) -> Option<Strike> {
    let storm = storm_at(weather, slot as f64 * cfg::SLOT_SECS);
    if storm <= 0.0 {
        return None;
    }
    strike_in(slot, storm, super::salt(weather))
}

/// Whether `strike` may be drawn after `last`, the last strike drawn: it is
/// that one, or it falls at least [`cfg::SLOT_TAIL_SECS`] after it. What
/// holds the flashes to two in a second whatever the schedule does.
pub fn may_draw(last: Option<&Strike>, strike: &Strike) -> bool {
    last.is_none_or(|last| last == strike || strike.at >= last.at + cfg::SLOT_TAIL_SECS)
}

/// How long after it strikes a pulse is out: its light is under 1/256 of
/// its peak by then (`ln 256` of [`cfg::PULSE_FADE`]).
const PULSE_OUT: f64 = cfg::PULSE_FADE * 5.545;

/// A pulse of light `since` seconds after it struck: all of it at once,
/// fading over [`cfg::PULSE_FADE`], and out after [`PULSE_OUT`].
fn pulse(since: f64) -> f32 {
    if !(0.0..PULSE_OUT).contains(&since) {
        0.0
    } else {
        (-since / cfg::PULSE_FADE).exp() as f32
    }
}

/// The light of `strike` at `unix_secs`: its pulses.
fn strike_light(strike: &Strike, unix_secs: f64) -> f32 {
    let first = strike.light * pulse(unix_secs - strike.at);
    let second = strike.second.map_or(0.0, |(after, share)| {
        strike.light * share * pulse(unix_secs - strike.at - after)
    });
    first.max(second)
}

/// The flash at `unix_secs` under `weather`, 0 to 1, by its schedule alone:
/// the pulses of the strikes in this slot and the one before (a pulse is
/// out well within a slot).
pub fn flash_at(unix_secs: f64, weather: &Weather) -> f32 {
    let slot = (unix_secs / cfg::SLOT_SECS).floor() as i64;
    [slot - 1, slot]
        .into_iter()
        .filter_map(|s| strike_of(weather, s))
        .map(|strike| strike_light(&strike, unix_secs))
        .fold(0.0, f32::max)
}

/// The stops a flash of `flash` (0 to 1) opens the camera's exposure by,
/// over a key light of `key_lux`: as much as its light multiplies the key
/// light's, at most [`cfg::FLASH_MAX_STOPS`].
pub fn flash_stops(flash: f32, key_lux: f32) -> f32 {
    let key = key_lux.max(cfg::FLASH_FLOOR_LUX);
    (1.0 + flash.max(0.0) * cfg::FLASH_LUX / key)
        .log2()
        .min(cfg::FLASH_MAX_STOPS)
}

/// The thunder's volume for a strike `distance` metres off.
fn thunder_volume(distance: f32) -> f32 {
    let far = ((distance - cfg::DISTANCE.0) / (cfg::DISTANCE.1 - cfg::DISTANCE.0)).clamp(0.0, 1.0);
    lerp(cfg::THUNDER_VOLUME.0, cfg::THUNDER_VOLUME.1, far)
}

/// The rate thunder is synthesised at (Hz): its rumble lies far under
/// half of it, and a slower rate halves the work of the frame that makes
/// it.
pub(crate) const THUNDER_RATE: u32 = 11_025;

/// At most this many thunderclaps play at once: more, each nearly at full
/// volume, would clip.
pub const MAX_THUNDER: usize = 2;

/// Thunder, synthesised: a rumble of brown noise swelling a few times and
/// dying away, deeper and slower to come when it is `near == false`; a
/// near one cracks first. The same samples every time.
pub(crate) fn thunder_samples(near: bool) -> Vec<f32> {
    let secs = if near { 3.5_f32 } else { 5.0 };
    let n = (secs * THUNDER_RATE as f32) as usize;
    let mut state = 0x74DE2_u64 + u64::from(near);
    let mut next = || {
        state = splitmix64(state);
        unit(state)
    };
    // The rumble's swells: when (s), how loud, how long (s).
    let swells: Vec<(f32, f32, f32)> = (0..5)
        .map(|k| {
            (
                0.1 + k as f32 * secs * 0.13 + next() * 0.3,
                if k == 0 { 1.0 } else { 0.4 + 0.5 * next() },
                0.25 + 0.4 * next(),
            )
        })
        .collect();
    // Smoothing per sample, at this rate (a one-pole low-pass).
    let (attack, smooth) = if near { (0.03, 0.16) } else { (0.35, 0.07) };
    let (mut brown, mut low) = (0.0_f32, 0.0_f32);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / THUNDER_RATE as f32;
        let white = next() * 2.0 - 1.0;
        // Brown noise (white, integrated and leaking), then smoothed: the
        // low roll of thunder.
        brown = (brown + white * 0.03) * 0.996;
        low += (brown - low) * smooth;
        let swell: f32 = swells
            .iter()
            .map(|&(at, gain, width)| gain * (-((t - at) / width).powi(2)).exp())
            .sum();
        let rise = (t / attack).min(1.0);
        let fade = (-t / (secs * 0.4)).exp();
        let mut s = low * swell * rise * fade;
        if near {
            s += white * 0.35 * (-t / 0.05).exp();
        }
        out.push(s);
    }
    let peak = out.iter().fold(0.0_f32, |m, s| m.max(s.abs())).max(1.0e-6);
    out.iter_mut().for_each(|s| *s *= 0.9 / peak);
    out
}

/// A thunder clip as an `AudioSource`: 16-bit WAV, half the bytes of
/// float on the wasm heap, which never shrinks.
fn thunder_source(near: bool) -> AudioSource {
    let bytes =
        bevy_symbios_audio::samples_to_wav_bytes_pcm16(&thunder_samples(near), THUNDER_RATE);
    AudioSource {
        bytes: Arc::from(bytes.into_boxed_slice()),
    }
}

/// The storm's lightning as it is drawn, and its thunder on the way.
#[derive(Resource, Default)]
pub struct Lightning {
    /// The flash drawn now, 0 to 1.
    flash: f32,
    /// The last strike drawn: none within [`cfg::SLOT_TAIL_SECS`] after it
    /// is drawn, whatever the schedule says.
    last: Option<Strike>,
    /// Thunder on its way: when it arrives (s, UTC), its volume, and
    /// whether it cracks.
    coming: Vec<(f64, f32, bool)>,
    /// The near and the far thunder, each synthesised the first time it is
    /// needed.
    near: Option<Handle<AudioSource>>,
    far: Option<Handle<AudioSource>>,
}

/// A flash held for a still (the render tool's `--flash`, 0 with none) in
/// place of the clock's, 0 to 1.
#[derive(Resource, Clone, Copy, Debug)]
pub struct FlashOverride(pub f32);

/// Marks a thunderclap's voice.
#[derive(Component)]
pub struct Thunder;

/// A flash this faint is out.
const FLASH_FLOOR: f32 = 1.0e-3;

/// Register the lightning - in the game and in the render tool's world.
pub(crate) fn register(app: &mut App) {
    app.init_resource::<Lightning>().add_systems(
        Update,
        flash_lightning
            .after(DrawEnvironmentSet)
            .after(crate::world_builder::compile::apply_environment_state)
            .after(super::precipitation::weather_the_volume),
    );
}

/// Strike, flash and thunder: see the module docs. Writes the materials
/// and the cameras only while a pulse is lit, and the resource only when
/// something in it changes.
#[allow(clippy::too_many_arguments)]
fn flash_lightning(
    drawn: Res<DrawnEnvironment>,
    record: Option<Res<LiveRoomRecord>>,
    (held, settings): (
        Option<Res<FlashOverride>>,
        Option<Res<crate::state::LocalSettings>>,
    ),
    mut lightning: ResMut<Lightning>,
    mut skies: ResMut<Assets<SkyMaterial>>,
    sky_box: Query<&MeshMaterial3d<SkyMaterial>, With<SkyBox>>,
    mut clouds: ResMut<Assets<CloudMaterial>>,
    cloud_layer: Query<&MeshMaterial3d<CloudMaterial>, With<CloudLayer>>,
    waters: Option<ResMut<Assets<crate::water::WaterMaterial>>>,
    mut rain: ResMut<Assets<super::precipitation::PrecipitationMaterial>>,
    volume: Query<
        &MeshMaterial3d<super::precipitation::PrecipitationMaterial>,
        With<super::precipitation::Precipitation>,
    >,
    mut cameras: Query<
        (Entity, Option<&Exposure>, Option<&mut DistanceFog>),
        crate::camera::IsWorldCamera,
    >,
    voices: Query<(), With<Thunder>>,
    muted: Option<Res<crate::audio_mute::AudioMuted>>,
    audio: Option<ResMut<Assets<AudioSource>>>,
    mut commands: Commands,
) {
    // No room, no storm: a weather left in the drawn environment after a
    // logout does not strike on the login screen.
    let weather = record.as_ref().and_then(|r| r.0.environment.weather);
    let held = held.map(|h| h.0);
    let now = unix_now();
    let stormy = weather.is_some_and(|w| storm_at(&w, now) > 0.0);
    let mut flash = 0.0_f32;
    match (held, weather) {
        (Some(held), _) => flash = held.clamp(0.0, 1.0),
        (None, Some(weather)) if stormy => {
            // This slot's strike and the last's, drawn only as the last
            // strike drawn allows; thunder for each newly drawn.
            let slot = (now / cfg::SLOT_SECS).floor() as i64;
            for s in [slot - 1, slot] {
                let Some(strike) = strike_of(&weather, s).filter(|strike| strike.at <= now) else {
                    continue;
                };
                if !may_draw(lightning.last.as_ref(), &strike) {
                    continue;
                }
                if lightning.last != Some(strike) {
                    lightning.last = Some(strike);
                    if audio.is_some() {
                        lightning.coming.push((
                            strike.at + f64::from(strike.distance / cfg::SOUND_SPEED),
                            thunder_volume(strike.distance),
                            strike.distance < cfg::CRACK_WITHIN,
                        ));
                    }
                }
                flash = flash.max(strike_light(&strike, now));
            }
        }
        _ => {}
    }
    // As much of it as this visitor's Effects setting draws (#1687): that
    // share of the light it adds, and of the stops it opens the exposure by
    // - at night a flash of any size opens the full four, so a share of the
    // light alone would soften nothing there. The strikes, and so the
    // thunder, fall all the same.
    let share = settings
        .as_ref()
        .map_or(1.0, |s| s.effects_intensity.flash_scale());
    let whole = flash;
    flash *= share;
    if !stormy && held.is_none() && !lightning.coming.is_empty() {
        lightning.coming.clear();
    }

    // Thunder that has arrived: at most `MAX_THUNDER` at once, any more
    // dropped.
    if let Some(mut audio) = audio
        && lightning.coming.iter().any(|&(due, ..)| due <= now)
    {
        let silent = muted.is_some_and(|m| m.0);
        let room = MAX_THUNDER.saturating_sub(voices.iter().count());
        let mut arrived = Vec::new();
        lightning.coming.retain(|&(due, volume, near)| {
            if due > now {
                return true;
            }
            arrived.push((volume, near));
            false
        });
        for (volume, near) in arrived.into_iter().take(room) {
            let clip = if near {
                lightning
                    .near
                    .get_or_insert_with(|| audio.add(thunder_source(true)))
                    .clone()
            } else {
                lightning
                    .far
                    .get_or_insert_with(|| audio.add(thunder_source(false)))
                    .clone()
            };
            commands.spawn((
                AudioPlayer::new(clip),
                PlaybackSettings {
                    mode: PlaybackMode::Despawn,
                    volume: Volume::Linear(volume),
                    muted: silent,
                    ..PlaybackSettings::ONCE
                },
                Thunder,
            ));
        }
    }

    // The flash: while a pulse is lit, and once more as it goes out, every
    // colour it touches is held at the flash's - written only where it
    // differs, so a colour the environment's own redraw put back
    // (`apply_environment_state` runs for new water too) is lit again. No
    // pulse lit, nothing to do.
    if flash < FLASH_FLOOR {
        flash = 0.0;
    }
    if flash == 0.0 && lightning.flash == 0.0 {
        return;
    }
    if lightning.flash != flash {
        lightning.flash = flash;
    }
    let added = Vec3::from_array(cfg::FLASH_SKY) * flash;
    let base = crate::sky::sky_uniforms(&drawn.env).zenith;
    let zenith = (base.truncate() + added).extend(flash);
    for handle in &sky_box {
        if skies
            .get(&handle.0)
            .is_some_and(|m| m.uniforms.zenith != zenith)
            && let Some(mut sky) = skies.get_mut(&handle.0)
        {
            sky.uniforms.zenith = zenith;
        }
    }
    for handle in &cloud_layer {
        if clouds
            .get(&handle.0)
            .is_some_and(|m| m.extension.uniforms.sky_zenith != zenith)
            && let Some(mut cloud) = clouds.get_mut(&handle.0)
        {
            cloud.extension.uniforms.sky_zenith = zenith;
        }
    }
    if let Some(mut waters) = waters {
        let unlit: Vec<_> = waters
            .iter()
            .filter(|(_, water)| water.extension.uniforms.sky_zenith != zenith)
            .map(|(id, _)| id)
            .collect();
        for id in unlit {
            if let Some(mut water) = waters.get_mut(id) {
                water.extension.uniforms.sky_zenith = zenith;
            }
        }
    }
    let falling = super::precipitation::uniforms(&drawn.env, &drawn.weather);
    let lit = (falling.light.truncate() + added).extend(falling.light.w);
    for handle in &volume {
        if rain.get(&handle.0).is_some_and(|m| m.uniforms.light != lit)
            && let Some(mut falls) = rain.get_mut(&handle.0)
        {
            falls.uniforms.light = lit;
        }
    }
    // Out with the colours: a flash floored to none opens nothing either.
    let stops = if flash > 0.0 {
        flash_stops(whole, drawn.env.sun_illuminance.0) * share
    } else {
        0.0
    };
    let [r, g, b, a] = drawn.env.fog_color.0;
    let fog = Color::srgba(r, g, b, a).to_linear();
    let fog = Color::LinearRgba(LinearRgba::new(
        fog.red + added.x,
        fog.green + added.y,
        fog.blue + added.z,
        fog.alpha,
    ));
    let ev100 = Exposure::default().ev100 - stops;
    for (camera, exposure, haze) in &mut cameras {
        // `try_`: a camera despawned before the commands land is no fault
        // (#1410).
        if stops > 0.0 {
            if exposure.is_none_or(|e| e.ev100 != ev100) {
                commands.entity(camera).try_insert(Exposure { ev100 });
            }
        } else if exposure.is_some() {
            commands.entity(camera).try_remove::<Exposure>();
        }
        if let Some(mut haze) = haze
            && haze.color != fog
        {
            haze.color = fog;
        }
    }
}
