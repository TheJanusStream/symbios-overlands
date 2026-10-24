//! The weather's own sound (#1771): rain hissing and wind roaring as hard as
//! the drawn weather brings them, and the room's ambient bed sinking under
//! them.
//!
//! The ambient bed is one baked loop (`crate::loading::ambient`) and cannot
//! follow a weather that comes and goes, so the weather sounds as thunder
//! does (`super::lightning`): its two loops are synthesised here, each once
//! on first need, and each plays only while its weather is in the air -
//! fading in as the weather comes and out as it goes, its player despawned
//! once silent, because a silent player still costs the mixer a decode and
//! a sum every sample (`crate::world_builder::voice_budget`). The rain is
//! the drawn rain, a storm's among it; the wind is a storm's or a dust
//! storm's gusts ([`WeatherNow::gusts`]). The bed sinks under the rain,
//! which masks it, and under the snow, which muffles it ([`bed_share`]).
//!
//! Volumes go on the sinks as the voice budget's do, times the global
//! volume; a loop is spawned at its own, which bevy multiplies by the global
//! volume as it builds the sink. The master mute (`crate::audio_mute`) owns
//! every sink's mute and survives this, because `set_volume` on a muted sink
//! changes only the volume an unmute restores; a loop is spawned muted under
//! the mute. Under a silent global volume (the agent's daemon) no loop
//! plays at all. A bed swapped in mid-rain is built at its own volume and
//! sinks under the rain from the next frame.

use std::sync::Arc;

use bevy::audio::{
    AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, GlobalVolume, PlaybackMode,
    PlaybackSettings, Volume,
};
use bevy::prelude::*;

use super::WeatherNow;
use crate::config::lighting::weather::sound as cfg;
use crate::drawn_environment::{DrawEnvironmentSet, DrawnEnvironment};
use crate::loading::AmbientPlayer;
use crate::state::LiveRoomRecord;

/// One of the weather's loops.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherLoop {
    /// Rain on the ground: a hiss with drops in it.
    Rain,
    /// Wind: a low roar in slow gusts.
    Wind,
}

impl WeatherLoop {
    /// Both loops, in [`WeatherSound`]'s order.
    pub const ALL: [WeatherLoop; 2] = [WeatherLoop::Rain, WeatherLoop::Wind];

    /// How loud the loop sounds under `weather`, 0 to 1 of its volume.
    pub fn level(self, weather: &WeatherNow) -> f32 {
        match self {
            WeatherLoop::Rain => weather.rain,
            WeatherLoop::Wind => weather.gusts(),
        }
        .clamp(0.0, 1.0)
    }

    /// The loop's volume at its weather's heaviest.
    fn volume(self) -> f32 {
        match self {
            WeatherLoop::Rain => cfg::RAIN_VOLUME,
            WeatherLoop::Wind => cfg::WIND_VOLUME,
        }
    }

    fn index(self) -> usize {
        match self {
            WeatherLoop::Rain => 0,
            WeatherLoop::Wind => 1,
        }
    }

    /// The loop's samples, the same every time.
    pub fn samples(self) -> Vec<f32> {
        match self {
            WeatherLoop::Rain => rain_samples(),
            WeatherLoop::Wind => wind_samples(),
        }
    }
}

/// The ambient bed's share of its own volume under `weather`: lowered under
/// the rain and the snow, never below silence.
pub fn bed_share(weather: &WeatherNow) -> f32 {
    (1.0 - cfg::BED_UNDER_RAIN * weather.rain.clamp(0.0, 1.0)
        - cfg::BED_UNDER_SNOW * weather.snow.clamp(0.0, 1.0))
    .clamp(0.0, 1.0)
}

/// The weather's sound as it plays: each loop's level now, eased toward its
/// weather's ([`cfg::FADE_SECS`]), each loop's clip once synthesised, and the
/// bed's share now.
#[derive(Resource)]
pub struct WeatherSound {
    levels: [f32; 2],
    clips: [Option<Handle<AudioSource>>; 2],
    bed: f32,
}

impl Default for WeatherSound {
    fn default() -> Self {
        Self {
            levels: [0.0; 2],
            clips: [None, None],
            bed: 1.0,
        }
    }
}

/// The ambient bed's playing sinks, with the volume each was spawned at.
type Beds<'w, 's> = Query<
    'w,
    's,
    (&'static PlaybackSettings, &'static mut AudioSink),
    (With<AmbientPlayer>, Without<WeatherLoop>),
>;

pub(crate) fn register(app: &mut App) {
    app.init_resource::<WeatherSound>()
        .add_systems(Update, sound_the_weather.after(DrawEnvironmentSet));
}

/// Fade each loop toward its weather's level - its player spawned when it
/// rises from silence and despawned once it falls back - and sink the bed
/// under the weather (see the module docs). Writes a sink only when its
/// volume moves.
#[allow(clippy::too_many_arguments)]
fn sound_the_weather(
    drawn: Option<Res<DrawnEnvironment>>,
    record: Option<Res<LiveRoomRecord>>,
    time: Res<Time>,
    global: Option<Res<GlobalVolume>>,
    muted: Option<Res<crate::audio_mute::AudioMuted>>,
    mut sound: ResMut<WeatherSound>,
    mut audio: Option<ResMut<Assets<AudioSource>>>,
    mut loops: Query<(Entity, &WeatherLoop, Option<&mut AudioSink>)>,
    mut beds: Beds,
    mut commands: Commands,
) {
    let global = global.map_or(Volume::Linear(1.0), |global| global.volume);
    // No room, no weather: a weather left drawn after a logout falls silent.
    // Nor any under a silent global volume (the agent's daemon), where a
    // player would cost the mixer for nothing.
    let weather = match (record, drawn) {
        (Some(_), Some(drawn)) if global.to_linear() > 0.0 => drawn.weather,
        _ => WeatherNow::default(),
    };
    let step = time.delta_secs() / cfg::FADE_SECS;
    let sound = &mut *sound;
    for kind in WeatherLoop::ALL {
        let level = &mut sound.levels[kind.index()];
        *level += (kind.level(&weather) - *level).clamp(-step, step);
    }

    let mut playing = [false; 2];
    for (entity, kind, sink) in &mut loops {
        let level = sound.levels[kind.index()];
        if level <= cfg::SILENT && kind.level(&weather) <= cfg::SILENT {
            commands.entity(entity).try_despawn();
            continue;
        }
        playing[kind.index()] = true;
        if let Some(mut sink) = sink {
            let volume = global * Volume::Linear(kind.volume() * level);
            if sink.volume() != volume {
                sink.set_volume(volume);
            }
        }
    }
    let silent = muted.is_some_and(|m| m.0);
    for kind in WeatherLoop::ALL {
        let level = sound.levels[kind.index()];
        if playing[kind.index()] || level <= cfg::SILENT {
            continue;
        }
        let Some(audio) = audio.as_deref_mut() else {
            continue;
        };
        let clip = sound.clips[kind.index()]
            .get_or_insert_with(|| audio.add(loop_source(kind)))
            .clone();
        commands.spawn((
            AudioPlayer::new(clip),
            PlaybackSettings {
                mode: PlaybackMode::Loop,
                // Its own volume: bevy multiplies the global volume in once,
                // as it builds the sink.
                volume: Volume::Linear(kind.volume() * level),
                muted: silent,
                ..PlaybackSettings::LOOP
            },
            kind,
        ));
    }

    sound.bed += (bed_share(&weather) - sound.bed).clamp(-step, step);
    for (settings, mut sink) in &mut beds {
        let volume = settings.volume * global * Volume::Linear(sound.bed);
        if sink.volume() != volume {
            sink.set_volume(volume);
        }
    }
}

/// A loop as an `AudioSource`: 16-bit WAV, half the bytes of float on the
/// wasm heap, which never shrinks.
fn loop_source(kind: WeatherLoop) -> AudioSource {
    let bytes = bevy_symbios_audio::samples_to_wav_bytes_pcm16(&kind.samples(), cfg::RATE);
    AudioSource {
        bytes: Arc::from(bytes.into_boxed_slice()),
    }
}

/// A noise source: a value from -1 to 1 a call, the same sequence for a
/// `salt` every time.
fn noise(salt: u64) -> impl FnMut() -> f32 {
    let mut state = salt;
    move || {
        state = super::splitmix64(state);
        (state >> 40) as f32 / (1u64 << 23) as f32 - 1.0
    }
}

/// A loop of [`cfg::LOOP_SECS`]: `voice` sampled over the loop and a
/// [`cfg::CROSSFADE_SECS`] tail past its end, the tail folded into its start
/// - its weight falling as the start's rises, equal in power, as two
///   unrelated noises' are - so the end runs on into the start without a
///   seam; peak-normalised to 0.9.
fn seamless(mut voice: impl FnMut(f32) -> f32) -> Vec<f32> {
    let rate = cfg::RATE as f32;
    let n = (cfg::LOOP_SECS * rate) as usize;
    let tail = (cfg::CROSSFADE_SECS * rate) as usize;
    let all: Vec<f32> = (0..n + tail).map(|i| voice(i as f32 / rate)).collect();
    let mut out = all[..n].to_vec();
    for (j, start) in out.iter_mut().take(tail).enumerate() {
        let t = j as f32 / tail as f32 * std::f32::consts::FRAC_PI_2;
        *start = all[n + j] * t.cos() + *start * t.sin();
    }
    let peak = out.iter().fold(0.0_f32, |m, s| m.max(s.abs())).max(1.0e-6);
    out.iter_mut().for_each(|s| *s *= 0.9 / peak);
    out
}

/// One pole of smoothing a sample at [`cfg::RATE`] for a corner of `hz`.
fn pole(hz: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * hz / cfg::RATE as f32).exp()
}

/// Rain on the ground: a steady hiss of high-passed, lightly smoothed noise,
/// rising and falling a little, with drops in it - short bright ticks, a few
/// dozen a second, each its own loudness.
fn rain_samples() -> Vec<f32> {
    let mut white = noise(0x7A1_0001);
    let mut chance = noise(0x7A1_0002);
    let (hiss_low, hiss_top) = (pole(450.0), pole(5_500.0));
    let (drop_low_pole, drop_top) = (pole(1_200.0), pole(7_000.0));
    let (mut low, mut top, mut drop_low, mut drop) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
    let mut tick = 0.0_f32;
    let per_sample = 38.0 / cfg::RATE as f32;
    seamless(move |t| {
        let w = white();
        // The hiss: noise with its lows taken out, its very top softened.
        low += (w - low) * hiss_low;
        top += ((w - low) - top) * hiss_top;
        let swell = 0.85 + 0.15 * (t * 0.9).sin() * (t * 0.37).cos();
        // A drop: a tick of its own loudness, decaying in a few ms.
        if (chance() + 1.0) * 0.5 < per_sample {
            tick = 0.4 + 0.6 * (chance() + 1.0) * 0.5;
        }
        tick *= 0.985;
        drop_low += (w - drop_low) * drop_low_pole;
        drop += ((w - drop_low) * tick - drop) * drop_top;
        top * swell + drop * 1.6
    })
}

/// Wind: a low roar of smoothed brown noise and a hollow band above it,
/// each swelling in slow gusts.
fn wind_samples() -> Vec<f32> {
    let mut white = noise(0x3D1_0001);
    let (roar_pole, band_low, band_top) = (pole(260.0), pole(380.0), pole(900.0));
    let (mut brown, mut roar, mut low, mut band) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
    seamless(move |t| {
        let w = white();
        // Brown noise (white, integrated and leaking), smoothed: the roar.
        brown = (brown + w * 0.05) * 0.997;
        roar += (brown - roar) * roar_pole;
        // A band of the white between two corners: the wind's hollow voice.
        low += (w - low) * band_low;
        band += ((w - low) - band) * band_top;
        let gust = |rate: f32, phase: f32| 0.5 + 0.5 * (t * rate + phase).sin();
        let roar_gust = 0.45 + 0.55 * gust(0.55, 0.0) * gust(0.23, 1.3);
        let band_gust = 0.2 + 0.8 * gust(0.41, 2.1) * gust(0.17, 0.4);
        roar * 4.0 * roar_gust + band * 0.6 * band_gust
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    /// Each loop is as long as asked, peaks at 0.9 with body under it -
    /// neither a few clicks nor a square wave - and runs on from its end
    /// into its start with no step bigger than its own sample-to-sample
    /// moves: the repeat has no seam.
    #[test]
    fn each_loop_runs_on_into_itself() {
        for kind in WeatherLoop::ALL {
            let samples = kind.samples();
            assert_eq!(samples.len(), (cfg::LOOP_SECS * cfg::RATE as f32) as usize);
            let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!((peak - 0.9).abs() < 1e-4, "{kind:?}: peak {peak}");
            let body = rms(&samples);
            assert!((0.08..0.5).contains(&body), "{kind:?}: rms {body}");
            let mut steps: Vec<f32> = samples.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
            steps.sort_by(f32::total_cmp);
            let usual = steps[steps.len() * 99 / 100];
            let seam = (samples[0] - samples[samples.len() - 1]).abs();
            assert!(
                seam <= usual,
                "{kind:?}: the seam steps {seam}, usually under {usual}"
            );
            assert_eq!(samples, kind.samples(), "{kind:?}: the same every time");
        }
    }

    /// The loops follow the weather: rain by its rain, a storm's rain among
    /// it, the wind by a storm's or a dust storm's gusts; snow and fog
    /// bring neither. The bed sinks under rain and snow, and never below
    /// silence.
    #[test]
    fn each_loop_sounds_its_own_weather() {
        let rain = WeatherNow {
            rain: 0.8,
            overcast: 1.0,
            ..default()
        };
        assert_eq!(WeatherLoop::Rain.level(&rain), 0.8);
        assert_eq!(WeatherLoop::Wind.level(&rain), 0.0);
        let dust = WeatherNow {
            dust: 0.6,
            ..default()
        };
        assert_eq!(WeatherLoop::Wind.level(&dust), 0.6);
        assert_eq!(WeatherLoop::Rain.level(&dust), 0.0);
        let quiet = WeatherNow {
            snow: 1.0,
            fog: 1.0,
            ..default()
        };
        assert_eq!(WeatherLoop::Rain.level(&quiet), 0.0);
        assert_eq!(WeatherLoop::Wind.level(&quiet), 0.0);
        assert_eq!(bed_share(&WeatherNow::default()), 1.0);
        assert!((bed_share(&rain) - (1.0 - cfg::BED_UNDER_RAIN * 0.8)).abs() < 1e-6);
        assert!((bed_share(&quiet) - (1.0 - cfg::BED_UNDER_SNOW)).abs() < 1e-6);
        let everything = WeatherNow {
            rain: 1.0,
            snow: 1.0,
            ..default()
        };
        assert!(bed_share(&everything) >= 0.0);
    }

    /// An app that runs the weather's sound over `rain`, a world's record in
    /// place, `hz` frames a second.
    fn sounding(muted: bool) -> App {
        use bevy::time::TimeUpdateStrategy;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<AudioSource>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f32(0.1),
            ))
            .insert_resource(crate::audio_mute::AudioMuted(muted))
            .insert_resource(LiveRoomRecord(crate::pds::RoomRecord::default()))
            .init_resource::<DrawnEnvironment>()
            .init_resource::<WeatherSound>()
            .add_systems(Update, sound_the_weather);
        app
    }

    fn set_weather(app: &mut App, weather: WeatherNow) {
        app.world_mut().resource_mut::<DrawnEnvironment>().weather = weather;
    }

    fn loops(app: &mut App) -> Vec<(WeatherLoop, bool)> {
        let mut query = app.world_mut().query::<(&WeatherLoop, &PlaybackSettings)>();
        query
            .iter(app.world())
            .map(|(kind, settings)| (*kind, settings.muted))
            .collect()
    }

    /// A loop plays only while its weather is in the air: none under a
    /// clear sky, the rain's once it rains - spawned muted under the master
    /// mute - and gone once the rain has stopped and its fade has run out;
    /// a world left behind (no record) falls silent too.
    #[test]
    fn a_loop_plays_only_while_its_weather_is_in_the_air() {
        let mut app = sounding(true);
        app.update();
        app.update();
        assert!(loops(&mut app).is_empty(), "a clear sky is silent");

        set_weather(
            &mut app,
            WeatherNow {
                rain: 0.7,
                ..default()
            },
        );
        app.update();
        app.update();
        assert_eq!(loops(&mut app), vec![(WeatherLoop::Rain, true)]);

        set_weather(&mut app, WeatherNow::default());
        for _ in 0..((cfg::FADE_SECS / 0.1) as usize + 3) {
            app.update();
        }
        assert!(loops(&mut app).is_empty(), "the rain's loop goes with it");

        set_weather(
            &mut app,
            WeatherNow {
                storm: 1.0,
                rain: 1.0,
                ..default()
            },
        );
        app.update();
        app.update();
        let mut playing: Vec<WeatherLoop> = loops(&mut app).into_iter().map(|(k, _)| k).collect();
        playing.sort_by_key(|k| k.index());
        assert_eq!(
            playing,
            WeatherLoop::ALL.to_vec(),
            "a storm rains and blows"
        );
        app.world_mut().remove_resource::<LiveRoomRecord>();
        for _ in 0..((cfg::FADE_SECS / 0.1) as usize + 3) {
            app.update();
        }
        assert!(loops(&mut app).is_empty(), "no world, no weather");
    }
}
