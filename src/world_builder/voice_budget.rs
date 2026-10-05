//! The looping-voice budget (#1557): the audible construct hums and engine
//! voices that reach the listener loudest hold a player, and the rest leave
//! the mixer until the listener comes near them - fading out and in, not
//! cut (#1562).
//!
//! # Why
//!
//! A construct whose generator carries audio loops a spatial voice from its
//! own position ([`super::spatial_audio`]), and so does an avatar body or a
//! worn part that carries one, since both spawn as constructs. The bake cache
//! shares one buffer between identical constructs, but each entity is its own
//! rodio player: decoded, spatialised, resampled and summed for every output
//! sample however far away it is. In the browser cpal's Web Audio host fills
//! its buffers from `onended` callbacks on the main thread, so that work lands
//! in the frame. Isoline - 61 lot buildings from five catalogue items - mixed
//! 52 voices at once, and its owner's client raised
//! `audio.looping_voices_overload` sixteen times in one visit.
//!
//! # The rule
//!
//! A pass weighs every [`LoopingVoice`] within [`VOICE_AUDIBLE_RADIUS_M`] of
//! the first [`SpatialListener`], the one bevy_audio hears through, by how
//! loud it reaches the listener - its own level over its distance squared,
//! the law rodio attenuates by - and gives a player to at most
//! [`VOICE_BUDGET`] of them, loudest first (#1562: by distance alone, a quiet
//! hum a metre nearer took the slot of an engine four times as loud). A
//! voice's level is measured once, when its bake lands
//! ([`wav_rms`]); a clip fetched from a link is never decoded here, so it is
//! weighed as a typical voice of the scene, the median of the measured ones.
//! A voice holding its slot is weighed [`VOICE_HOLD_RATIO`] nearer than it
//! is, and that is the whole hysteresis: it plays on a little past the
//! radius, and a held-back voice must be clearly louder than a playing one to
//! take its slot, so a voice at either edge does not flap. A voice fading out
//! no longer holds its slot, and competes as held back. Equal weights go
//! to the lower entity index, so the outcome never depends on the order a
//! query yields. A voice the master mute or a peer mute silences is not
//! weighed at all (see Mute). Passes run [`VOICE_BUDGET_PERIOD_SECS`] apart,
//! and at once on the frame a voice is attached, so a new voice never waits
//! for the clock. With no listener there is nobody to rank for, and nothing
//! changes.
//!
//! # Fades (#1562)
//!
//! A voice the budget drops while the listener can still hear it - pushed out
//! of a slot by a louder one, inside the release radius - fades out over
//! [`VOICE_FADE_SECS`] and only then loses its player: stopped dead, a hum
//! cut off with a click. A voice brought back starts silent and fades in, so
//! the loop it starts afresh (see below) swells in rather than cuts in; one
//! that regains its slot while still fading out simply turns and fades back
//! up, its loop never interrupted. A voice leaving the release radius, 43 dB
//! down, or silenced by a mute, is inaudible already and goes at once, and so
//! do the quietest of the voices pushed out past [`VOICE_FADING_OUT_MAX`]
//! fading at once. A fade sets the sink's volume, which a mute does not
//! fight: bevy_audio's `set_volume` on a muted sink only changes the volume an
//! unmute restores ([`crate::audio_mute`] owns the mute).
//!
//! The pass runs in `Last`, after transform propagation, so a voice spawned
//! this frame is weighed where it stands rather than at the origin its
//! `GlobalTransform` holds until `PostUpdate`. Its commands land before the
//! next frame's `PostUpdate`, which is where bevy_audio builds sinks.
//!
//! # What a held-back voice costs
//!
//! Nothing in the mixer, and that rules out the obvious levers. A muted sink
//! is a player at volume 0, so its decoder, its spatial gains, its resampler
//! and the mixer's sum all run for every sample. A paused one has rodio's
//! `Pausable` emit zeros in place of its input, and the wrappers above it,
//! the resampler and the sum still run for every sample (rodio 0.22,
//! `player.rs` and `source/pausable.rs`). What ends the work is dropping the
//! player: `Player::drop` sets `stopped` and turns its queue's keep-alive
//! off, the next 5 ms periodic access stops the source, the queue then ends,
//! and `MixerSource::sum_current_sources` drops a source that has ended from
//! its list (`mixer.rs`).
//!
//! So holding a voice back removes its `SpatialAudioSink`, which drops the
//! player, and its `AudioPlayer` and `PlaybackSettings` with it: bevy_audio
//! 0.19's `play_queued_audio_system` builds a sink for every entity that has
//! an `AudioPlayer` and no sink, so a sink removed alone is rebuilt the next
//! frame and the loop restarts. Bringing the voice back inserts the player
//! and the settings its [`LoopingVoice`] kept, and removes any sink still on
//! the entity first, because inserting an `AudioPlayer` over a playing one
//! replaces the component and leaves the old sink playing the old sound.
//! Nothing is seeked - a loop sink cannot seek, and `try_seek` spins forever
//! in the browser - so a voice that comes back starts its loop afresh, from
//! its loop start (#1341), fading in.
//!
//! # Mute
//!
//! A voice the master toggle or a peer mute silences takes no slot and holds
//! no player: the pass leaves it out of the ranking and holds it back, and
//! the first pass after the mute lifts weighs it again. Weighed with the
//! rest, a muted peer standing close in a body of two dozen voice nodes - the
//! sanitiser allows a thousand - would take every slot with silent players
//! and hold back every voice the listener could hear, their own included,
//! and muting that peer, the one lever a victim has, would give nothing back
//! (the critic's finding on #1557). Held back, a silenced voice also costs
//! nothing in the mixer, so the master mute now does what the Audio card says
//! it does: takes the looping voices out of the frame.
//!
//! [`crate::audio_mute`] still decides, every frame, whether a playing sink
//! is silent - between a mute and the next pass it mutes the sink at once -
//! and the budget sets a sink's volume only while it fades (#1562), which a
//! mute holds through. One gap is older than the
//! budget: a muted peer's node spawned after `sync_mute_audio` ran in a frame
//! is not yet in `SilencedByMute` when this pass runs, so it can be admitted
//! and play for a frame until the reconciler mutes it and the next pass
//! takes it out.
//!
//! # Not budgeted
//!
//! The room's ambient bed is one non-spatial voice, a one-shot (a footstep, a
//! landing, a contact cue) ends on its own, and contact cues have their own
//! cap besides (`config::interaction::audio::MAX_CONCURRENT_VOICES`).

use bevy::audio::{
    AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, GlobalVolume, PlaybackSettings,
    SpatialAudioSink, SpatialListener, Volume,
};
use bevy::ecs::system::SystemParam;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;

use super::spatial_audio::CONSTRUCT_SPATIAL_SCALE;
use crate::audio_mute::{AudioMuted, SilencedByMute};

/// How many looping voices may play at once (#1557).
///
/// Every voice here falls off by the same law, so among voices of a like
/// level the one the budget leaves out is no louder than any of the 24 it
/// keeps, and would raise their summed power by at most a 24th: under
/// 0.2 dB, nothing anyone picks out of a crowd that size. It is the contact
/// cues' own cap too (`config::interaction::audio::MAX_CONCURRENT_VOICES`).
/// The looping gauge `audio.looping_voices_overload` reads counts what
/// plays, so with the budget it reaches 24 plus the few loops outside it -
/// the ambient bed, the editor's auditions - and the rule fires only for a
/// path that bypasses the budget.
pub const VOICE_BUDGET: usize = 24;

/// How far from the listener a held-back voice may start, in metres (#1557).
///
/// rodio's spatial gain falls as `1 / d²` of the distance, taking nothing
/// inside one unit, and bevy scales the emitter and the ears by the voice's
/// spatial scale - [`CONSTRUCT_SPATIAL_SCALE`] for every voice here - before
/// rodio sees them. So distance takes nothing from a voice out to 4 m and
/// 12 dB with each doubling after that: at 40 m its louder channel is 40 dB
/// down, an amplitude of 1/100, under the room's ambient bed and every
/// nearer hum (straight ahead, rodio's pan law takes 2.5 dB more). Written as
/// the scaled distance where `1 / d²` reaches 1/100, over the scale, so a new
/// scale moves the radius with it.
pub const VOICE_AUDIBLE_RADIUS_M: f32 = 10.0 / CONSTRUCT_SPATIAL_SCALE;

/// How much nearer than it is a playing voice is weighed, as a ratio of
/// distances (#1557).
///
/// The hysteresis in one number. A playing voice keeps playing out to 1.2
/// times [`VOICE_AUDIBLE_RADIUS_M`] (48 m, 43 dB down), and a held-back voice
/// takes a playing one's slot only from 1.2 times nearer. Under a `1 / d²`
/// gain a ratio of distances is a ratio of levels, 40 log10(1.2) = 3.2 dB at
/// any distance, so the margin is the same to the ear near or far, and a
/// listener swaying a few metres at 40 m cannot carry a voice across it.
pub const VOICE_HOLD_RATIO: f32 = 1.2;

/// Seconds between budget passes (#1557).
///
/// Not to save the ranking, which is a distance per voice and a sort of a
/// few dozen entries. It bounds churn: every admission builds a rodio player
/// and a decoder and every release drops one, and four passes a second keep
/// a listener crossing a crowd of voices from rebuilding players every frame
/// while leaving no voice more than a quarter second late.
pub const VOICE_BUDGET_PERIOD_SECS: f64 = 0.25;

/// Over how long a voice the budget drops while it can still be heard fades
/// out, and a voice it brings back fades in, in seconds (#1562).
///
/// A cut is a click, and a ramp of a few tens of milliseconds still reads as
/// one on a steady hum; four tenths of a second reads as the voice receding.
/// A voice fading out keeps its player meanwhile, so at most
/// [`VOICE_FADING_OUT_MAX`] more voices than the budget play, for under half a
/// second.
pub const VOICE_FADE_SECS: f32 = 0.4;

/// How many voices may fade out at once (#1562).
///
/// A listener walking through a crowd trades a slot or two a pass; a portal
/// hop inside a busy street, or a body of voice nodes walking up, can push out
/// the whole budget in one. A fade-out keeps its player, so the fades are
/// capped: past the cap the quietest of the voices pushed out stop at once, as
/// every one did before #1562 - in a change that size, a few cut-offs go
/// unheard. Bounds what plays at the budget plus this, which the overload rule
/// counts against (`diagnostics::anomaly`'s `LOOPING_VOICES_OVERLOAD_COUNT`).
pub const VOICE_FADING_OUT_MAX: usize = 4;

/// The most one frame moves a fade (#1562): a share of its range. A frame of
/// a twentieth of a second moves it an eighth, and a hitch of a quarter
/// second - the most `Time<Virtual>` steps - moves it no further than this
/// rather than five eighths at once, which is a click of its own.
const VOICE_FADE_MAX_STEP: f32 = 0.15;

/// A looping spatial voice - a construct's hum, an avatar's engine - and
/// what it needs to play again after the budget held it back (#1557).
///
/// On the entity for as long as the voice exists, playing or not: the
/// player's handle and settings are copied out of it each time the voice
/// comes back, so it always comes back as it was attached, loop start and
/// all.
#[derive(Component, Clone, Debug)]
pub struct LoopingVoice {
    /// The buffer it loops.
    pub(crate) clip: Handle<AudioSource>,
    /// The settings it plays with, as attached.
    pub(crate) settings: PlaybackSettings,
    /// How loud its buffer is, as a linear RMS - full scale is 1 - for
    /// ranking it against the others (#1562): the whole bake, a sequence's
    /// run-up included. `None` for a clip never measured - one fetched from a
    /// link, which nothing here decodes - and it is weighed as a typical
    /// voice of the scene, the median of the measured ones, neither louder
    /// nor quieter than the voices around it.
    pub(crate) level: Option<f32>,
}

/// A looping voice on its way up to its own volume or down to silence
/// (#1562): the budget brought it back or dropped it while it could still be
/// heard. [`fade_looping_voices`] steps it every frame and ends it - removing
/// itself at the top, holding the voice back at the bottom.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct VoiceFade {
    /// The share of its own volume the voice plays at now, 0 to 1.
    gain: f32,
    /// Up to its volume, or down to silence and then held back.
    rising: bool,
}

/// On a [`LoopingVoice`] the budget is holding back: the entity has no
/// `AudioPlayer`, `PlaybackSettings` or sink, so rodio has nothing of it to
/// mix (#1557).
#[derive(Component, Clone, Copy, Debug)]
pub struct HeldBackVoice;

/// Give `entity` a looping voice that plays `clip` with `settings`, held
/// back until the budget's next pass, which runs this frame. `level` is the
/// clip's RMS ([`wav_rms`]), `None` for one never measured (#1562, see
/// [`LoopingVoice::level`]).
///
/// The one door into the budget, for the baked and the fetched construct
/// voice alike. A voice arrives held back rather than playing so that one
/// out of budget, or silenced by a mute, never builds a sink at all.
/// Whatever player the entity had goes: attaching a voice replaces
/// the old one rather than playing beside it. `try_*` because the entity
/// can be despawned by the time the commands apply - a room rebuilt under a
/// bake in flight - and Bevy 0.19 panics on a plain insert there (#1410).
pub fn attach_looping_voice(
    commands: &mut Commands,
    entity: Entity,
    clip: Handle<AudioSource>,
    settings: PlaybackSettings,
    level: Option<f32>,
) {
    let level = level.filter(|level| level.is_finite());
    commands
        .entity(entity)
        .try_remove::<(
            AudioPlayer,
            PlaybackSettings,
            AudioSink,
            SpatialAudioSink,
            VoiceFade,
        )>()
        .try_insert((
            LoopingVoice {
                clip,
                settings,
                level,
            },
            HeldBackVoice,
        ));
}

/// The RMS of a WAV file's samples, across its channels, as a linear share
/// of full scale; `None` for bytes that are not a WAV this can read, or hold
/// no sample (#1562). What the budget ranks a baked voice by: the bake is a
/// loop, so its RMS is the level it plays at for as long as it plays.
pub(crate) fn wav_rms(bytes: &[u8]) -> Option<f32> {
    let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes)).ok()?;
    let spec = reader.spec();
    let mut squares = 0.0f64;
    let mut count = 0usize;
    match spec.sample_format {
        hound::SampleFormat::Float => {
            for sample in reader.samples::<f32>() {
                let sample = f64::from(sample.ok()?);
                squares += sample * sample;
                count += 1;
            }
        }
        hound::SampleFormat::Int => {
            let full_scale = f64::from(1u32 << spec.bits_per_sample.clamp(1, 32).saturating_sub(1));
            for sample in reader.samples::<i32>() {
                let sample = f64::from(sample.ok()?) / full_scale;
                squares += sample * sample;
                count += 1;
            }
        }
    }
    (count > 0).then(|| (squares / count as f64).sqrt() as f32)
}

/// Register the budget pass in `Last` (see the module doc for why there),
/// and the fades after it, so a fade the pass starts takes its first step in
/// the same frame.
pub(crate) fn register(app: &mut App) {
    app.add_systems(Last, (budget_looping_voices, fade_looping_voices).chain());
}

/// One looping voice as a pass weighs it.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    entity: Entity,
    /// From the listener, in metres.
    distance: f32,
    /// How loud it plays at a metre, as a linear amplitude: its buffer's RMS
    /// times the volume it plays at.
    level: f32,
    /// Whether it holds its slot now: it plays, and is not fading out. A
    /// voice fading out competes as one held back (#1562's review): weighed
    /// nearer as the one that pushed it out is, the two would trade the slot
    /// at every pass inside the fade with no margin between them.
    playing: bool,
}

/// The level to weigh a voice never measured at: the median of the measured
/// ones, 1 when none is (#1562). An unmeasured clip is weighed as a typical
/// voice of the scene - a near-silent clip fetched for a peer's worn part
/// gains no slot over the hums around it, and a loud one loses none.
fn typical_level(measured: &mut [f32]) -> f32 {
    if measured.is_empty() {
        return 1.0;
    }
    measured.sort_unstable_by(f32::total_cmp);
    measured[measured.len() / 2]
}

/// The voices of `candidates` that play, loudest at the listener first: at
/// most `budget` of them, each within [`VOICE_AUDIBLE_RADIUS_M`] once a
/// playing voice is weighed [`VOICE_HOLD_RATIO`] nearer. Loudness is ranked as
/// a distance, the distance at which a full-scale voice would be as loud -
/// the voice's own over the square root of its level, since rodio's gain
/// falls as the distance squared (#1562) - so voices of one level rank
/// exactly by distance. Equal weights go to the lower entity index - two live
/// entities never share one - and only then to `Entity`'s own order, which in
/// Bevy 0.19 runs by generation and then by DESCENDING index (the index is
/// stored complemented). A distance that is not a number weighs past every
/// radius, so its voice is held back, and so is a voice with no level.
fn voices_that_play(candidates: &[Candidate], budget: usize) -> Vec<Entity> {
    let mut weighed: Vec<(f32, Entity)> = candidates
        .iter()
        .filter_map(|voice| {
            let hold = if voice.playing { VOICE_HOLD_RATIO } else { 1.0 };
            let in_range = voice.distance / hold <= VOICE_AUDIBLE_RADIUS_M;
            let loudness = voice.distance / (voice.level.sqrt() * hold);
            (in_range && voice.level > 0.0).then_some((loudness, voice.entity))
        })
        .collect();
    weighed.sort_unstable_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then(a.1.index_u32().cmp(&b.1.index_u32()))
            .then(a.1.cmp(&b.1))
    });
    weighed.truncate(budget);
    weighed.into_iter().map(|(_, entity)| entity).collect()
}

/// The two reasons a sink is silent (#1219 f324), read so the pass leaves a
/// voice either covers out of the ranking. Either resource may be absent in
/// an app without [`crate::audio_mute::AudioMutePlugin`], and then it
/// silences nothing.
#[derive(SystemParam)]
struct Silence<'w> {
    master: Option<Res<'w, AudioMuted>>,
    peers: Option<Res<'w, SilencedByMute>>,
}

impl Silence<'_> {
    fn covers(&self, entity: Entity) -> bool {
        let master = self.master.as_ref().is_some_and(|muted| muted.0);
        match self.peers.as_deref() {
            Some(peers) => crate::audio_mute::sink_is_silenced(master, peers, entity),
            None => master,
        }
    }
}

/// What the budget pass reads of a looping voice: whether it is held back,
/// and the fade it is in, if any.
type BudgetedVoice = (
    Entity,
    &'static LoopingVoice,
    &'static GlobalTransform,
    Has<HeldBackVoice>,
    Option<&'static VoiceFade>,
);

/// What the fades step on a fading voice: its fade, and the sink bevy_audio
/// built for it, spatial or not, once it has one.
type FadingVoice = (
    Entity,
    &'static LoopingVoice,
    &'static mut VoiceFade,
    Option<&'static mut SpatialAudioSink>,
    Option<&'static mut AudioSink>,
);

/// The budget pass (#1557): rank the looping voices, take the player off
/// each voice that falls out of the budget and give one back to each that
/// comes into it - fading either way while the listener can hear it (#1562).
///
/// Touches only the voices that change. A sink's volume is the fades' to set
/// ([`fade_looping_voices`]); the pass only starts, turns and ends them.
fn budget_looping_voices(
    mut commands: Commands,
    time: Res<Time>,
    mut last_pass: Local<Option<f64>>,
    attached: Query<(), Changed<LoopingVoice>>,
    listeners: Query<&GlobalTransform, With<SpatialListener>>,
    voices: Query<BudgetedVoice>,
    silence: Silence,
) {
    let now = time.elapsed_secs_f64();
    let due = last_pass.is_none_or(|at| now - at >= VOICE_BUDGET_PERIOD_SECS);
    if !due && attached.is_empty() {
        return;
    }
    let Some(ear) = listeners.iter().next().map(GlobalTransform::translation) else {
        return;
    };
    *last_pass = Some(now);

    // A silenced voice is not weighed, so it never plays here and one that
    // does is held back below (see the module doc's Mute).
    let heard = || {
        voices
            .iter()
            .filter(|(entity, ..)| !silence.covers(*entity))
    };
    let mut measured: Vec<f32> = heard().filter_map(|(_, voice, ..)| voice.level).collect();
    let typical = typical_level(&mut measured);
    let fading_out = |fade: Option<&VoiceFade>| fade.is_some_and(|fade| !fade.rising);
    let candidates: Vec<Candidate> = heard()
        .map(|(entity, voice, at, held_back, fade)| Candidate {
            entity,
            distance: at.translation().distance(ear),
            level: voice.level.unwrap_or(typical) * voice.settings.volume.to_linear(),
            playing: !held_back && !fading_out(fade),
        })
        .collect();
    let play: HashSet<Entity> = voices_that_play(&candidates, VOICE_BUDGET)
        .into_iter()
        .collect();

    // The voices pushed out that the listener can still hear, loudest first:
    // the first few fade, the rest stop at once (`VOICE_FADING_OUT_MAX`).
    let mut pushed_out: Vec<(f32, Entity, Option<VoiceFade>)> = Vec::new();
    let mut still_fading = 0usize;
    for (entity, voice, at, held_back, fade) in &voices {
        match (held_back, play.contains(&entity)) {
            (true, true) => bring_back(&mut commands, entity, voice),
            // Regained its slot while fading out: back up, from where it is,
            // its loop never interrupted.
            (false, true) => {
                if let Some(fade) = fade.filter(|fade| !fade.rising) {
                    commands.entity(entity).try_insert(VoiceFade {
                        rising: true,
                        ..*fade
                    });
                }
            }
            (false, false) => {
                let distance = at.translation().distance(ear);
                let audible = distance <= VOICE_AUDIBLE_RADIUS_M * VOICE_HOLD_RATIO
                    && !silence.covers(entity);
                if !audible {
                    // Out of earshot, or silent already: nothing to fade.
                    hold_back(&mut commands, entity);
                } else if fading_out(fade) {
                    still_fading += 1;
                } else {
                    let level = voice.level.unwrap_or(typical) * voice.settings.volume.to_linear();
                    let loudness = distance / level.max(f32::MIN_POSITIVE).sqrt();
                    pushed_out.push((loudness, entity, fade.copied()));
                }
            }
            (true, false) => {}
        }
    }
    pushed_out.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let fades_left = VOICE_FADING_OUT_MAX.saturating_sub(still_fading);
    for (index, (_, entity, fade)) in pushed_out.into_iter().enumerate() {
        if index < fades_left {
            commands.entity(entity).try_insert(VoiceFade {
                gain: fade.map_or(1.0, |fade| fade.gain),
                rising: false,
            });
        } else {
            hold_back(&mut commands, entity);
        }
    }
}

/// Step every fading voice's volume, and end the fades that are done: at the
/// top the voice plays at its own volume, and its own settings go back on it
/// for a sink bevy_audio has yet to build; at the bottom it is held back
/// (#1562). The volume is the one bevy_audio built the sink at - the voice's
/// own times the [`GlobalVolume`] - scaled by the fade's gain; set on a muted
/// sink it only changes what an unmute restores. No frame moves a fade more
/// than [`VOICE_FADE_MAX_STEP`].
pub(crate) fn fade_looping_voices(
    mut commands: Commands,
    time: Res<Time>,
    global: Option<Res<GlobalVolume>>,
    mut voices: Query<FadingVoice>,
) {
    let step = (time.delta_secs() / VOICE_FADE_SECS).min(VOICE_FADE_MAX_STEP);
    let global = global.map_or(Volume::Linear(1.0), |global| global.volume);
    for (entity, voice, mut fade, spatial, plain) in &mut voices {
        fade.gain = if fade.rising {
            (fade.gain + step).min(1.0)
        } else {
            (fade.gain - step).max(0.0)
        };
        let volume = voice.settings.volume * global * Volume::Linear(fade.gain);
        if let Some(mut sink) = spatial {
            sink.set_volume(volume);
        } else if let Some(mut sink) = plain {
            sink.set_volume(volume);
        }
        if fade.rising && fade.gain >= 1.0 {
            // The voice was brought back with silent settings (`bring_back`);
            // a sink built from here on is built at its own volume.
            commands
                .entity(entity)
                .try_remove::<VoiceFade>()
                .try_insert(voice.settings);
        } else if !fade.rising && fade.gain <= 0.0 {
            hold_back(&mut commands, entity);
        }
    }
}

/// Take `entity`'s voice out of the mix: the sink goes, which drops its
/// rodio player, and the player and settings go with it so bevy_audio has
/// nothing to rebuild a sink from (see the module doc). A fade goes too.
fn hold_back(commands: &mut Commands, entity: Entity) {
    commands
        .entity(entity)
        .try_remove::<(
            AudioPlayer,
            PlaybackSettings,
            AudioSink,
            SpatialAudioSink,
            VoiceFade,
        )>()
        .try_insert(HeldBackVoice);
}

/// Put `entity`'s voice back in the mix as it was attached, but silent, to
/// fade in from there (#1562): the loop starts afresh from its loop start, so
/// it swells in rather than cuts in. A sink left on the entity is removed
/// first: bevy_audio builds a sink only for a player that has none. Never a
/// silenced voice: the pass does not weigh one.
fn bring_back(commands: &mut Commands, entity: Entity, voice: &LoopingVoice) {
    commands
        .entity(entity)
        .try_remove::<(HeldBackVoice, AudioSink, SpatialAudioSink)>()
        .try_insert((
            AudioPlayer::new(voice.clip.clone()),
            PlaybackSettings {
                volume: Volume::SILENT,
                ..voice.settings
            },
            VoiceFade {
                gain: 0.0,
                rising: true,
            },
        ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_builder::spatial_audio::looping_construct_playback;
    use std::time::Duration;

    fn entities(n: usize) -> Vec<Entity> {
        let mut world = World::new();
        (0..n).map(|_| world.spawn_empty().id()).collect()
    }

    fn held(entity: Entity, distance: f32) -> Candidate {
        Candidate {
            entity,
            distance,
            level: 1.0,
            playing: false,
        }
    }

    fn playing(entity: Entity, distance: f32) -> Candidate {
        Candidate {
            entity,
            distance,
            level: 1.0,
            playing: true,
        }
    }

    /// The budget itself: of thirty voices in range, the 24 nearest play,
    /// nearest first, whether they start held back or all playing.
    #[test]
    fn the_nearest_voices_play_up_to_the_budget() {
        let es = entities(30);
        // Listed farthest first, so the order kept is the ranking's own.
        let at = |i: usize| (i + 1) as f32;
        let held_back: Vec<_> = es
            .iter()
            .enumerate()
            .rev()
            .map(|(i, e)| held(*e, at(i)))
            .collect();
        assert_eq!(
            voices_that_play(&held_back, VOICE_BUDGET),
            es[..VOICE_BUDGET].to_vec(),
            "the 24 nearest, nearest first"
        );
        let all_playing: Vec<_> = es
            .iter()
            .enumerate()
            .rev()
            .map(|(i, e)| playing(*e, at(i)))
            .collect();
        assert_eq!(
            voices_that_play(&all_playing, VOICE_BUDGET),
            es[..VOICE_BUDGET].to_vec(),
            "voices already playing are held to the budget too"
        );
    }

    /// Nothing outside the audible range: a held-back voice starts only
    /// within 40 m, a playing one stops past 48, and a voice whose distance
    /// is not a number never plays.
    #[test]
    fn no_voice_plays_outside_the_audible_range() {
        let es = entities(6);
        let release = VOICE_AUDIBLE_RADIUS_M * VOICE_HOLD_RATIO;
        let voices = [
            held(es[0], VOICE_AUDIBLE_RADIUS_M - 0.1),
            held(es[1], VOICE_AUDIBLE_RADIUS_M),
            held(es[2], VOICE_AUDIBLE_RADIUS_M + 0.1),
            playing(es[3], release - 0.1),
            playing(es[4], release + 0.1),
            held(es[5], f32::NAN),
        ];
        let mut kept = voices_that_play(&voices, VOICE_BUDGET);
        kept.sort_unstable_by_key(|e| e.index_u32());
        assert_eq!(kept, vec![es[0], es[1], es[3]]);
    }

    /// A voice at an edge does not flap. The listener sways two metres either
    /// way across the audible radius, across the release radius, and across
    /// the distance of the voice holding the last slot; each voice changes
    /// state at most once, however many times the sway crosses the edge.
    #[test]
    fn a_voice_at_an_edge_does_not_flap() {
        // Carry one voice's state through a sway of distances, counting how
        // often it flips.
        fn flips(start_playing: bool, sway: &[f32]) -> usize {
            let es = entities(1);
            let mut on = start_playing;
            let mut flips = 0;
            for d in sway {
                let now = voices_that_play(
                    &[Candidate {
                        entity: es[0],
                        distance: *d,
                        level: 1.0,
                        playing: on,
                    }],
                    1,
                )
                .contains(&es[0]);
                flips += usize::from(now != on);
                on = now;
            }
            flips
        }
        let across = |edge: f32| -> Vec<f32> {
            (0..12)
                .map(|i| if i % 2 == 0 { edge - 2.0 } else { edge + 2.0 })
                .collect()
        };
        assert_eq!(
            flips(false, &across(VOICE_AUDIBLE_RADIUS_M)),
            1,
            "the audible radius"
        );
        assert_eq!(
            flips(true, &across(VOICE_AUDIBLE_RADIUS_M * VOICE_HOLD_RATIO)),
            1,
            "the release radius"
        );

        // The last slot: a budget of one, held by a voice at 10 m, while a
        // held-back rival sways between 9 and 11.
        let es = entities(2);
        let (holder, rival) = (es[0], es[1]);
        let mut holding = holder;
        let mut changes = 0;
        for rival_at in [9.0, 11.0, 9.0, 11.0, 9.0, 11.0] {
            let voices = [
                Candidate {
                    entity: holder,
                    distance: 10.0,
                    level: 1.0,
                    playing: holding == holder,
                },
                Candidate {
                    entity: rival,
                    distance: rival_at,
                    level: 1.0,
                    playing: holding == rival,
                },
            ];
            let now = voices_that_play(&voices, 1)[0];
            changes += usize::from(now != holding);
            holding = now;
        }
        assert_eq!(changes, 0, "a rival a metre nearer does not take the slot");
        let voices = [playing(holder, 10.0), held(rival, 8.0)];
        assert_eq!(
            voices_that_play(&voices, 1),
            vec![rival],
            "one more than 1.2 times nearer does"
        );
    }

    /// Ties are deterministic: three voices at one distance compete for two
    /// slots, and the two lowest entity indices win in whatever order the
    /// voices arrive.
    #[test]
    fn voices_at_one_distance_are_kept_by_entity_index_whatever_order_they_arrive_in() {
        let es = entities(3);
        let orders = [[0, 1, 2], [2, 1, 0], [1, 2, 0], [2, 0, 1]];
        for order in orders {
            let voices: Vec<_> = order.iter().map(|&i| held(es[i], 12.0)).collect();
            assert_eq!(
                voices_that_play(&voices, 2),
                vec![es[0], es[1]],
                "arriving as {order:?}"
            );
        }
    }

    /// The radius is where rodio's own falloff puts a voice 40 dB down: a
    /// constant signal through rodio's `Spatial`, with the emitter and the
    /// ears scaled the way bevy_audio scales them, read in its louder
    /// channel. To one side - the pan law's loudest - it is 40 dB down at the
    /// radius and 12 dB further down at twice it; straight ahead the pan law
    /// takes 2.5 dB more.
    #[test]
    fn the_audible_radius_is_where_rodio_puts_a_voice_40_db_down() {
        use rodio::Source as _;
        use std::num::NonZero;

        // Gain in dB of the louder channel for an emitter at `at`, in metres
        // from a listener at the origin with ears along X.
        let louder_db = |at: Vec3| -> f32 {
            let s = CONSTRUCT_SPATIAL_SCALE;
            let half_gap = crate::config::interaction::audio::LISTENER_EAR_GAP / 2.0;
            let tone = rodio::buffer::SamplesBuffer::new(
                NonZero::new(1).expect("one channel"),
                NonZero::new(22_050).expect("a rate"),
                vec![1.0; 64],
            );
            let mut spatial = rodio::source::Spatial::new(
                tone,
                (at * s).to_array(),
                [-half_gap * s, 0.0, 0.0],
                [half_gap * s, 0.0, 0.0],
            );
            assert_eq!(spatial.channels().get(), 2);
            let left = spatial.next().expect("a left sample");
            let right = spatial.next().expect("a right sample");
            20.0 * left.max(right).log10()
        };
        let near = |db: f32, want: f32, what: &str| {
            assert!((db - want).abs() < 0.5, "{what}: {db:.2} dB, not {want}");
        };
        let r = VOICE_AUDIBLE_RADIUS_M;
        near(louder_db(Vec3::X * r), -40.0, "to one side at the radius");
        near(
            louder_db(Vec3::X * 2.0 * r),
            -52.0,
            "to one side at twice it",
        );
        near(
            louder_db(Vec3::NEG_Z * r),
            -42.5,
            "straight ahead at the radius",
        );
    }

    /// The app the game registers the pass in, with the clock under the
    /// test's hand, nobody muted, and a listener at the origin.
    fn app() -> (App, Entity) {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.insert_resource(AudioMuted(false));
        app.init_resource::<SilencedByMute>();
        register(&mut app);
        let listener = app
            .world_mut()
            .spawn((SpatialListener::default(), GlobalTransform::IDENTITY))
            .id();
        (app, listener)
    }

    /// A voice attached through the door the construct path uses.
    fn voice(
        app: &mut App,
        at: Vec3,
        clip: Handle<AudioSource>,
        settings: PlaybackSettings,
    ) -> Entity {
        let world = app.world_mut();
        let entity = world.spawn(GlobalTransform::from_translation(at)).id();
        attach_looping_voice(&mut world.commands(), entity, clip, settings, None);
        world.flush();
        entity
    }

    fn move_to(app: &mut App, entity: Entity, at: Vec3) {
        *app.world_mut()
            .get_mut::<GlobalTransform>(entity)
            .expect("a transform") = GlobalTransform::from_translation(at);
    }

    /// Run the next pass the clock asks for.
    fn next_pass(app: &mut App) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f64(VOICE_BUDGET_PERIOD_SECS));
        app.update();
    }

    fn plays(app: &App, entity: Entity) -> bool {
        let world = app.world();
        let has_player = world.get::<AudioPlayer>(entity).is_some();
        let held_back = world.get::<HeldBackVoice>(entity).is_some();
        assert_ne!(
            has_player, held_back,
            "a voice either plays or is held back"
        );
        assert_eq!(
            world.get::<PlaybackSettings>(entity).is_some(),
            has_player,
            "settings go and come with the player"
        );
        has_player
    }

    fn settings(app: &App, entity: Entity) -> PlaybackSettings {
        *app.world()
            .get::<PlaybackSettings>(entity)
            .expect("a playing voice")
    }

    /// The whole loop in an app: the nearest 24 of a street's voices play,
    /// then the listener walks to two far voices and the street goes quiet
    /// while the unmuted one starts. The far voice a peer mute covers stays
    /// held back until the mute lifts, then plays its own clip from its own
    /// loop start; the master mute holds every voice back, and lifting it
    /// brings the street back.
    #[test]
    fn moving_the_listener_swaps_which_voices_hold_a_player() {
        let (mut app, listener) = app();
        let mut clips = Assets::<AudioSource>::default();
        let mut clip = || {
            clips.add(AudioSource {
                bytes: std::sync::Arc::from(Vec::new()),
            })
        };
        let street: Vec<Entity> = (1..=26)
            .map(|x| {
                let c = clip();
                voice(
                    &mut app,
                    Vec3::X * x as f32,
                    c,
                    looping_construct_playback(None),
                )
            })
            .collect();
        let loop_start = Some(Duration::from_secs(2));
        let far_clip = clip();
        let far = voice(
            &mut app,
            Vec3::X * 200.0,
            far_clip.clone(),
            looping_construct_playback(loop_start),
        );
        let farther_clip = clip();
        let farther = voice(
            &mut app,
            Vec3::X * 205.0,
            farther_clip,
            looping_construct_playback(None),
        );
        app.world_mut()
            .resource_mut::<SilencedByMute>()
            .0
            .insert(far);

        // Attached this frame: weighed this frame, with no clock tick.
        app.update();
        for (i, v) in street.iter().enumerate() {
            assert_eq!(
                plays(&app, *v),
                i < VOICE_BUDGET,
                "street voice {} m away",
                i + 1
            );
        }
        assert!(!plays(&app, far));
        assert!(!plays(&app, farther));

        move_to(&mut app, listener, Vec3::X * 200.0);
        next_pass(&mut app);
        assert!(
            street.iter().all(|v| !plays(&app, *v)),
            "the street is 174 m off and more"
        );
        assert!(plays(&app, farther), "the nearest voice nobody muted plays");
        assert!(!plays(&app, far), "a peer-muted voice holds no player");

        app.world_mut()
            .resource_mut::<SilencedByMute>()
            .0
            .remove(&far);
        next_pass(&mut app);
        assert!(plays(&app, far), "the first pass after the mute lifts");
        let back = settings(&app, far);
        assert!(!back.muted, "it plays as it was attached");
        assert_eq!(back.start_position, loop_start, "its loop start (#1341)");
        assert!(matches!(back.mode, bevy::audio::PlaybackMode::Loop));
        assert!(back.spatial);
        assert_eq!(
            back.spatial_scale.map(|s| s.0),
            Some(Vec3::splat(CONSTRUCT_SPATIAL_SCALE))
        );
        assert_eq!(
            app.world().get::<AudioPlayer>(far).expect("a player").0,
            far_clip,
            "its own clip"
        );

        app.world_mut().resource_mut::<AudioMuted>().0 = true;
        move_to(&mut app, listener, Vec3::ZERO);
        next_pass(&mut app);
        assert!(
            street
                .iter()
                .chain([&far, &farther])
                .all(|v| !plays(&app, *v)),
            "the master mute holds every voice back"
        );
        app.world_mut().resource_mut::<AudioMuted>().0 = false;
        next_pass(&mut app);
        for (i, v) in street.iter().enumerate() {
            assert_eq!(
                plays(&app, *v),
                i < VOICE_BUDGET,
                "street voice {} m away",
                i + 1
            );
        }
    }

    /// #1557 critic: a peer the listener muted, standing close in a body of
    /// as many voice nodes as the budget holds, takes no slot - the building
    /// behind them still plays. Weighed by distance alone, their silent
    /// voices took every player and muting them, a victim's one lever, gave
    /// the room nothing back.
    #[test]
    fn a_muted_peer_takes_no_slot_from_the_voices_the_listener_can_hear() {
        let (mut app, _) = app();
        let mut clips = Assets::<AudioSource>::default();
        let mut clip = || {
            clips.add(AudioSource {
                bytes: std::sync::Arc::from(Vec::new()),
            })
        };
        let harasser: Vec<Entity> = (0..VOICE_BUDGET)
            .map(|i| {
                let c = clip();
                voice(
                    &mut app,
                    Vec3::new(2.0, 0.0, i as f32 * 0.01),
                    c,
                    looping_construct_playback(None),
                )
            })
            .collect();
        app.world_mut()
            .resource_mut::<SilencedByMute>()
            .0
            .extend(harasser.iter().copied());
        let c = clip();
        let building = voice(&mut app, Vec3::X * 5.0, c, looping_construct_playback(None));
        app.update();
        assert!(plays(&app, building), "the building 5 m off plays");
        assert!(
            harasser.iter().all(|v| !plays(&app, *v)),
            "the muted peer's voices hold no player"
        );
    }

    /// #1410 under the budget: an entity despawned between a pass and the
    /// moment its commands apply - a room rebuilt that frame - takes the
    /// hold-back and the bring-back without a panic. Bevy 0.19 panics on a
    /// plain insert into a despawned entity through the command error
    /// handler; only `try_*` is safe there.
    #[test]
    fn a_voice_despawned_before_the_pass_applies_does_not_panic() {
        let (mut app, listener) = app();
        let mut clips = Assets::<AudioSource>::default();
        let near = voice(
            &mut app,
            Vec3::X,
            clips.add(AudioSource {
                bytes: std::sync::Arc::from(Vec::new()),
            }),
            looping_construct_playback(None),
        );
        let far = voice(
            &mut app,
            Vec3::X * 200.0,
            clips.add(AudioSource {
                bytes: std::sync::Arc::from(Vec::new()),
            }),
            looping_construct_playback(None),
        );
        app.update();
        assert!(plays(&app, near) && !plays(&app, far));

        // The listener walks to the far voice: the pass queues a hold-back
        // for `near` and a bring-back for `far`, and both entities go before
        // the commands apply.
        move_to(&mut app, listener, Vec3::X * 200.0);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f64(VOICE_BUDGET_PERIOD_SECS));
        let mut pass = IntoSystem::into_system(budget_looping_voices);
        let _ = pass.initialize(app.world_mut());
        let world = app.world_mut();
        pass.run_without_applying_deferred((), world)
            .expect("the pass runs");
        world.despawn(near);
        world.despawn(far);
        pass.apply_deferred(world);
        world.flush();
    }

    /// What holding a voice back buys, asked of real rodio players: a voice
    /// the budget holds back leaves its mixer within a few samples, while a
    /// muted voice - and a paused one - stays in its mixer and is summed for
    /// every sample.
    #[test]
    fn a_held_back_voice_leaves_the_mixer_and_a_muted_one_does_not() {
        use bevy::audio::AudioSinkPlayback as _;
        use rodio::Source as _;
        use std::num::NonZero;

        // One mixer per voice, so a mixer that runs dry says which voice left.
        let sink_on_own_mixer = || {
            let (mixer, mixed) = rodio::mixer::mixer(
                NonZero::new(2).expect("stereo"),
                NonZero::new(48_000).expect("a rate"),
            );
            let player = rodio::SpatialPlayer::connect_new(
                &mixer,
                [0.0, 0.0, -1.0],
                [-0.15, 0.0, 0.0],
                [0.15, 0.0, 0.0],
            );
            player.append(
                rodio::buffer::SamplesBuffer::new(
                    NonZero::new(1).expect("mono"),
                    NonZero::new(22_050).expect("a rate"),
                    vec![0.5; 2_205],
                )
                .repeat_infinite(),
            );
            (SpatialAudioSink::new(player), mixed)
        };
        // Whether the mixer is still summing something after a tenth of a
        // second of output.
        let still_mixing =
            |mixed: &mut rodio::mixer::MixerSource| (0..9_600).all(|_| mixed.next().is_some());

        let (mut app, listener) = app();
        let mut clips = Assets::<AudioSource>::default();
        let clip = clips.add(AudioSource {
            bytes: std::sync::Arc::from(Vec::new()),
        });
        let playing_voice = |app: &mut App, at: Vec3| {
            let entity = voice(app, at, clip.clone(), looping_construct_playback(None));
            app.update();
            let (sink, mixed) = sink_on_own_mixer();
            // The sink bevy_audio would have built for the player.
            app.world_mut().entity_mut(entity).insert(sink);
            (entity, mixed)
        };
        let (leaving, mut leaving_mix) = playing_voice(&mut app, Vec3::X * 30.0);
        let (muted, mut muted_mix) = playing_voice(&mut app, Vec3::NEG_X * 10.0);
        let (paused, mut paused_mix) = playing_voice(&mut app, Vec3::NEG_X * 12.0);
        app.world_mut()
            .get_mut::<SpatialAudioSink>(muted)
            .expect("a sink")
            .mute();
        app.world()
            .get::<SpatialAudioSink>(paused)
            .expect("a sink")
            .pause();

        // The listener steps away from the one at +30 m.
        move_to(&mut app, listener, Vec3::NEG_X * 20.0);
        next_pass(&mut app);
        assert!(
            !still_mixing(&mut leaving_mix),
            "a held-back voice has left its mixer"
        );
        assert!(still_mixing(&mut muted_mix), "a muted voice is still mixed");
        assert!(
            still_mixing(&mut paused_mix),
            "a paused voice is still mixed"
        );

        assert!(!plays(&app, leaving), "50 m off: held back");
        assert!(app.world().get::<SpatialAudioSink>(leaving).is_none());
        assert!(plays(&app, muted) && plays(&app, paused));
    }

    /// The cadence: a listener moving between passes changes nothing until
    /// the next one, and a voice attached between passes is weighed at once.
    #[test]
    fn a_new_voice_is_weighed_at_once_and_a_moving_listener_at_the_next_pass() {
        let (mut app, listener) = app();
        let mut clips = Assets::<AudioSource>::default();
        let clip = clips.add(AudioSource {
            bytes: std::sync::Arc::from(Vec::new()),
        });
        let near = voice(
            &mut app,
            Vec3::X * 10.0,
            clip.clone(),
            looping_construct_playback(None),
        );
        app.update();
        assert!(plays(&app, near));

        move_to(&mut app, listener, Vec3::X * 1_000.0);
        app.update();
        assert!(plays(&app, near), "no pass is due yet");
        next_pass(&mut app);
        assert!(!plays(&app, near), "the next pass");

        let arrival = voice(
            &mut app,
            Vec3::X * 1_005.0,
            clip,
            looping_construct_playback(None),
        );
        app.update();
        assert!(
            plays(&app, arrival),
            "attached between passes, weighed at once"
        );
    }

    /// #1562: the budget keeps the voices the listener hears loudest, not the
    /// nearest. An engine at full scale 20 m off reaches the listener louder
    /// than a hum 28 dB down 5 m off - rodio's gain falls as the distance
    /// squared, 1/400 against 0.04/25 - so with one slot the engine plays; at
    /// one level the nearer plays, as it always did; a silent buffer takes no
    /// slot; and the audible radius is a distance still.
    #[test]
    fn the_loudest_voices_at_the_listener_play_not_the_nearest() {
        let es = entities(3);
        let at = |entity, distance, level| Candidate {
            entity,
            distance,
            level,
            playing: false,
        };
        let engine = at(es[0], 20.0, 1.0);
        let hum = at(es[1], 5.0, 0.04);
        assert_eq!(
            voices_that_play(&[hum, engine], 1),
            vec![es[0]],
            "the louder one"
        );
        let near_engine = at(es[1], 5.0, 1.0);
        assert_eq!(
            voices_that_play(&[near_engine, engine], 1),
            vec![es[1]],
            "at one level, the nearer"
        );
        assert!(
            voices_that_play(&[at(es[2], 1.0, 0.0)], 1).is_empty(),
            "a silent buffer holds no player"
        );
        assert!(
            voices_that_play(&[at(es[0], VOICE_AUDIBLE_RADIUS_M + 1.0, 100.0)], 1).is_empty(),
            "however loud, past the radius it waits"
        );
    }

    /// A WAV written into memory with `spec`, its samples by `write`.
    fn wav(
        spec: hound::WavSpec,
        write: impl FnOnce(&mut hound::WavWriter<&mut std::io::Cursor<Vec<u8>>>),
    ) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        let mut writer = hound::WavWriter::new(&mut bytes, spec).expect("a writer");
        write(&mut writer);
        writer.finalize().expect("finalised");
        bytes.into_inner()
    }

    /// The level a baked voice is ranked by (#1562): a WAV's RMS as a share
    /// of full scale, for the 16-bit PCM a bake writes and for float samples;
    /// nothing for bytes that are not a WAV, or a WAV with no sample.
    #[test]
    fn a_bakes_level_is_its_rms() {
        let pcm = hound::WavSpec {
            channels: 1,
            sample_rate: 22_050,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let half_scale = wav(pcm, |w| {
            for _ in 0..1_000 {
                w.write_sample(16_384i16).expect("a sample");
            }
        });
        let level = wav_rms(&half_scale).expect("a level");
        assert!((level - 0.5).abs() < 1e-6, "{level}");

        let float = hound::WavSpec {
            channels: 2,
            sample_rate: 22_050,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let sine = wav(float, |w| {
            for i in 0..22_050 {
                let s = 0.8 * (i as f32 * std::f32::consts::TAU * 441.0 / 22_050.0).sin();
                w.write_sample(s).expect("left");
                w.write_sample(s).expect("right");
            }
        });
        let level = wav_rms(&sine).expect("a level");
        assert!((level - 0.8 / 2f32.sqrt()).abs() < 1e-3, "{level}");

        assert_eq!(wav_rms(b"not a wav at all"), None);
        assert_eq!(wav_rms(&wav(pcm, |_| {})), None, "no sample, no level");
    }

    /// Advance the clock by `secs` and run a frame, whatever the pass's
    /// cadence; `0.0` runs a frame that moves no fade.
    fn run_for(app: &mut App, secs: f32) {
        const FRAME: f32 = 1.0 / 60.0;
        if secs <= 0.0 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::ZERO);
            app.update();
            return;
        }
        let mut left = secs;
        while left > 1e-6 {
            let frame = left.min(FRAME);
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs_f32(frame));
            app.update();
            left -= frame;
        }
    }

    fn fade_of(app: &App, entity: Entity) -> Option<VoiceFade> {
        app.world().get::<VoiceFade>(entity).copied()
    }

    /// #1562, the case that asked for it: a voice a louder one pushes out of
    /// its slot while the listener can still hear it fades out over
    /// [`VOICE_FADE_SECS`] and only then loses its player; brought back it
    /// starts silent and fades in; and one that regains its slot while still
    /// fading out turns and fades back up on the player it has, its loop
    /// never restarted.
    #[test]
    fn a_voice_pushed_out_of_its_slot_fades_out_and_comes_back_fading_in() {
        let (mut app, _) = app();
        let mut clips = Assets::<AudioSource>::default();
        let mut clip = || {
            clips.add(AudioSource {
                bytes: std::sync::Arc::from(Vec::new()),
            })
        };
        // A full budget, 5 to 28 m off: playing, settled at their volume.
        let crowd: Vec<Entity> = (0..VOICE_BUDGET)
            .map(|i| {
                let c = clip();
                voice(
                    &mut app,
                    Vec3::X * (5.0 + i as f32),
                    c,
                    looping_construct_playback(None),
                )
            })
            .collect();
        run_for(&mut app, 0.0);
        run_for(&mut app, VOICE_FADE_SECS + 0.1);
        let last = *crowd.last().expect("a crowd");
        assert!(plays(&app, last) && fade_of(&app, last).is_none());

        // A voice arrives 2 m off: the crowd's last, 28 m off and audible,
        // gives up its slot fading.
        let c = clip();
        let arrival = voice(&mut app, Vec3::X * 2.0, c, looping_construct_playback(None));
        run_for(&mut app, 0.0);
        assert!(plays(&app, last), "it keeps its player while it fades");
        assert_eq!(fade_of(&app, last).map(|f| f.rising), Some(false));
        run_for(&mut app, VOICE_FADE_SECS / 2.0);
        let half = fade_of(&app, last).expect("still fading");
        assert!((half.gain - 0.5).abs() < 0.05, "{half:?}");
        run_for(&mut app, VOICE_FADE_SECS);
        assert!(!plays(&app, last), "faded out, then held back");
        assert!(fade_of(&app, last).is_none());

        // The arrival goes: the last comes back silent and fades in.
        app.world_mut().despawn(arrival);
        run_for(&mut app, VOICE_BUDGET_PERIOD_SECS as f32);
        assert!(plays(&app, last));
        assert_eq!(
            settings(&app, last).volume,
            Volume::SILENT,
            "it starts silent"
        );
        assert_eq!(fade_of(&app, last).map(|f| f.rising), Some(true));
        run_for(&mut app, VOICE_FADE_SECS);
        assert!(fade_of(&app, last).is_none(), "and rises to its own volume");
        assert_eq!(
            settings(&app, last).volume,
            looping_construct_playback(None).volume,
            "its own settings are back, for a sink built from here on"
        );

        // Pushed out again, and back before the fade ends: it turns.
        let c = clip();
        let arrival = voice(&mut app, Vec3::X * 2.0, c, looping_construct_playback(None));
        run_for(&mut app, 0.0);
        let player_set_at = app
            .world()
            .entity(last)
            .get_ref::<AudioPlayer>()
            .expect("a player")
            .last_changed();
        run_for(&mut app, 0.1);
        let falling = fade_of(&app, last).expect("fading out");
        assert!(!falling.rising && falling.gain < 1.0, "{falling:?}");
        app.world_mut().despawn(arrival);
        run_for(&mut app, VOICE_BUDGET_PERIOD_SECS as f32 - 0.1);
        assert!(plays(&app, last), "never held back");
        assert!(fade_of(&app, last).is_none_or(|fade| fade.rising));
        assert_eq!(
            app.world()
                .entity(last)
                .get_ref::<AudioPlayer>()
                .expect("a player")
                .last_changed(),
            player_set_at,
            "the same player: its loop was never restarted"
        );
    }

    /// The fade sets the volume a sink plays at - the voice's own times the
    /// global volume, scaled by the fade - and a mute holds through it: set
    /// on a muted sink, a volume only changes what the unmute restores
    /// (#1562, composing with `audio_mute`'s reconciler).
    #[test]
    fn a_fade_sets_the_sinks_volume_and_a_mute_holds_through_it() {
        use std::num::NonZero;

        let mut app = App::new();
        app.init_resource::<Time>()
            .insert_resource(GlobalVolume::new(Volume::Linear(0.5)))
            .add_systems(Update, fade_looping_voices);
        let (mixer, _mixed) = rodio::mixer::mixer(
            NonZero::new(2).expect("stereo"),
            NonZero::new(48_000).expect("a rate"),
        );
        let player = rodio::SpatialPlayer::connect_new(
            &mixer,
            [0.0, 0.0, -1.0],
            [-0.15, 0.0, 0.0],
            [0.15, 0.0, 0.0],
        );
        let voice = LoopingVoice {
            clip: Handle::default(),
            settings: PlaybackSettings {
                volume: Volume::Linear(0.8),
                ..looping_construct_playback(None)
            },
            level: None,
        };
        let entity = app
            .world_mut()
            .spawn((
                voice,
                VoiceFade {
                    gain: 1.0,
                    rising: false,
                },
                SpatialAudioSink::new(player),
            ))
            .id();
        app.world_mut()
            .get_mut::<SpatialAudioSink>(entity)
            .expect("a sink")
            .mute();

        run_for(&mut app, VOICE_FADE_SECS / 2.0);
        let sink = app.world().get::<SpatialAudioSink>(entity).expect("a sink");
        assert!(sink.is_muted(), "a fade does not unmute");
        let halfway = 0.8 * 0.5 * 0.5;
        assert!(
            (sink.volume().to_linear() - halfway).abs() < 1e-3,
            "{:?}",
            sink.volume()
        );
        app.world_mut()
            .get_mut::<SpatialAudioSink>(entity)
            .expect("a sink")
            .unmute();
        let sink = app.world().get::<SpatialAudioSink>(entity).expect("a sink");
        assert!(
            (sink.volume().to_linear() - halfway).abs() < 1e-3,
            "the unmute restores the faded volume"
        );
    }

    /// A voice attached at `level` - a bake's measured RMS.
    fn voice_at_level(app: &mut App, at: Vec3, clip: Handle<AudioSource>, level: f32) -> Entity {
        let world = app.world_mut();
        let entity = world.spawn(GlobalTransform::from_translation(at)).id();
        attach_looping_voice(
            &mut world.commands(),
            entity,
            clip,
            looping_construct_playback(None),
            Some(level),
        );
        world.flush();
        entity
    }

    /// Whether `entity` holds its slot: it plays and is not fading out.
    fn holds(app: &App, entity: Entity) -> bool {
        plays(app, entity) && fade_of(app, entity).is_none_or(|fade| fade.rising)
    }

    fn one_clip() -> Handle<AudioSource> {
        Assets::<AudioSource>::default().add(AudioSource {
            bytes: std::sync::Arc::from(Vec::new()),
        })
    }

    /// #1562's review: a voice fading out competes as held back, so the slot
    /// it gave up keeps its hysteresis. The budget's last slot is held 30 m
    /// off; a rival sways between 24 m and 32 m - near enough to take the slot
    /// (under 30 / 1.2 = 25), never far enough to give it back (32 / 1.2 =
    /// 26.7 is nearer than 30). The slot changes hands once. Weighed nearer
    /// while it faded, the old holder took the slot back at 32 m, and the two
    /// traded it at every pass, fading each way.
    #[test]
    fn a_slot_changes_hands_once_while_its_last_holder_fades() {
        let (mut app, _) = app();
        let clip = one_clip();
        // 23 voices hold the nearer slots throughout.
        for d in 1..VOICE_BUDGET {
            voice(
                &mut app,
                Vec3::X * d as f32,
                clip.clone(),
                looping_construct_playback(None),
            );
        }
        let holder = voice(
            &mut app,
            Vec3::NEG_X * 30.0,
            clip.clone(),
            looping_construct_playback(None),
        );
        run_for(&mut app, 0.0);
        run_for(&mut app, VOICE_FADE_SECS + 0.1);
        let rival = voice(
            &mut app,
            Vec3::Z * 32.0,
            clip.clone(),
            looping_construct_playback(None),
        );
        run_for(&mut app, 0.0);
        assert!(holds(&app, holder) && !plays(&app, rival));

        let mut owner = holder;
        let mut changes = 0;
        for sway in 0..12 {
            let at = if sway % 2 == 0 { 24.0 } else { 32.0 };
            move_to(&mut app, rival, Vec3::Z * at);
            run_for(&mut app, VOICE_BUDGET_PERIOD_SECS as f32);
            let now = if holds(&app, rival) { rival } else { holder };
            changes += usize::from(now != owner);
            owner = now;
        }
        assert_eq!(changes, 1, "the slot changes hands once");
        assert_eq!(owner, rival);
    }

    /// #1562's review: a change that pushes the whole budget out at once - a
    /// portal hop inside a busy street - fades the loudest few out and stops
    /// the rest at once, so what plays never passes the budget by more than
    /// [`VOICE_FADING_OUT_MAX`].
    #[test]
    fn at_most_a_few_voices_fade_out_at_once() {
        let (mut app, _) = app();
        let clip = one_clip();
        // A settled budget 15 to 38 m off, inside the audible radius: each
        // of them, pushed out alone, would fade.
        let settled: Vec<Entity> = (0..VOICE_BUDGET)
            .map(|i| {
                voice(
                    &mut app,
                    Vec3::X * (15.0 + i as f32),
                    clip.clone(),
                    looping_construct_playback(None),
                )
            })
            .collect();
        run_for(&mut app, 0.0);
        run_for(&mut app, VOICE_FADE_SECS + 0.1);
        assert!(settled.iter().all(|voice| holds(&app, *voice)), "settled");
        // A budget's worth of nearer voices arrives in one frame, all of them
        // nearer than 15 m over the hold ratio, so every settled voice goes.
        let arrivals: Vec<Entity> = (0..VOICE_BUDGET)
            .map(|i| {
                voice(
                    &mut app,
                    Vec3::NEG_X * (1.0 + i as f32 * 0.4),
                    clip.clone(),
                    looping_construct_playback(None),
                )
            })
            .collect();
        run_for(&mut app, 0.0);

        let fading: Vec<Entity> = settled
            .iter()
            .copied()
            .filter(|voice| fade_of(&app, *voice).is_some_and(|fade| !fade.rising))
            .collect();
        assert_eq!(
            fading,
            settled[..VOICE_FADING_OUT_MAX].to_vec(),
            "the loudest - here the nearest - fade"
        );
        let players = |app: &App| {
            settled
                .iter()
                .chain(&arrivals)
                .filter(|voice| app.world().get::<AudioPlayer>(**voice).is_some())
                .count()
        };
        assert_eq!(players(&app), VOICE_BUDGET + VOICE_FADING_OUT_MAX);
        run_for(&mut app, VOICE_FADE_SECS + 0.1);
        assert_eq!(players(&app), VOICE_BUDGET, "and the fades end");
    }

    /// #1562's review: a clip nobody measured - fetched from a link, a peer's
    /// worn part among them - is weighed as a typical voice of the scene, the
    /// median of the measured ones. Weighed as full scale, a near-silent one
    /// 34 m off took the slot of a hum 33 m off (34 against 33 / sqrt(0.1) =
    /// 104); weighed as the hums are, it is the farthest voice there, and
    /// waits.
    #[test]
    fn a_clip_never_measured_is_weighed_as_a_typical_voice() {
        assert_eq!(typical_level(&mut []), 1.0, "nothing measured: all alike");
        assert_eq!(typical_level(&mut [0.4, 0.1, 0.2]), 0.2);

        let (mut app, _) = app();
        let clip = one_clip();
        let hums: Vec<Entity> = (0..VOICE_BUDGET)
            .map(|i| voice_at_level(&mut app, Vec3::X * (10.0 + i as f32), clip.clone(), 0.1))
            .collect();
        let fetched = voice(
            &mut app,
            Vec3::NEG_X * 34.0,
            clip,
            looping_construct_playback(None),
        );
        run_for(&mut app, 0.0);
        assert!(hums.iter().all(|hum| plays(&app, *hum)));
        assert!(!plays(&app, fetched), "the farthest typical voice waits");
    }

    /// #1562's review: a hitch - one long frame - moves a fade no further
    /// than one capped step, where a quarter second used to move it five
    /// eighths at once, itself a click.
    #[test]
    fn a_hitch_moves_a_fade_one_step_at_most() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .add_systems(Update, fade_looping_voices);
        let entity = app
            .world_mut()
            .spawn((
                LoopingVoice {
                    clip: Handle::default(),
                    settings: looping_construct_playback(None),
                    level: None,
                },
                VoiceFade {
                    gain: 1.0,
                    rising: false,
                },
            ))
            .id();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(0.25));
        app.update();
        let fade = fade_of(&app, entity).expect("still fading");
        assert!(
            (fade.gain - (1.0 - VOICE_FADE_MAX_STEP)).abs() < 1e-6,
            "{fade:?}"
        );
    }
}
