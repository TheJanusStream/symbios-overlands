//! Tests for the seeded ambient-recipe deriver.
//!
//! - Same `(scene, seed)` → same recipe (determinism — every other
//!   room deriver in the project honours this contract).
//! - Different seeds diverge — each room's ambient bed must sound
//!   distinct, otherwise the DID seeding is for nothing on the audio
//!   axis.
//! - The default room record carries a non-`None` ambient recipe that
//!   parses cleanly back to a native [`SequenceRecipe`] via the
//!   JSON-stash round trip.

use bevy_symbios_audio::{NodeKind, SequenceRecipe};
use symbios_overlands::pds::{RoomRecord, SovereignAudioConfig};
use symbios_overlands::seeded_defaults::{AmbientRecipe, SceneCharacter};

const TEST_DID: &str = "did:plc:z5yhcebtrvzblrojezn6pjgi";

// ---------------------------------------------------------------------------
// Deriver-level contract.
// ---------------------------------------------------------------------------

#[test]
fn ambient_recipe_is_deterministic_for_same_seed() {
    let scene = SceneCharacter::for_seed(0x1234_5678);
    let a = AmbientRecipe::from_scene(&scene, 0x1234_5678).recipe;
    let b = AmbientRecipe::from_scene(&scene, 0x1234_5678).recipe;
    assert_eq!(a, b, "same scene + seed must produce identical recipes");
}

#[test]
fn ambient_recipe_diverges_across_seeds() {
    let scene_a = SceneCharacter::for_did("did:plc:alice");
    let scene_b = SceneCharacter::for_did("did:plc:bob");
    let a = AmbientRecipe::from_scene(&scene_a, 0xAAAA).recipe;
    let b = AmbientRecipe::from_scene(&scene_b, 0xBBBB).recipe;
    // Different seeds must drive different patch parameters — at the
    // recipe granularity, this surfaces as a non-equal `instruments`
    // vector (different cutoff / LFO rate / amplitude).
    assert_ne!(
        a, b,
        "scenes built from different DIDs must produce distinct ambient recipes"
    );
}

#[test]
fn ambient_recipe_uses_a_loopable_window() {
    let scene = SceneCharacter::for_did(TEST_DID);
    let recipe = AmbientRecipe::from_scene(&scene, 1).recipe;
    assert!(
        recipe.loop_start_beats.is_some(),
        "seeded ambient must be loopable (loop_start_beats set)"
    );
    assert!(
        recipe.loop_crossfade_beats > 0.0,
        "tail crossfade must be non-zero so the loop seam is seamless; got {}",
        recipe.loop_crossfade_beats
    );
    assert!(
        recipe.duration_beats > recipe.loop_crossfade_beats,
        "loop duration must exceed the crossfade window"
    );
}

#[test]
fn ambient_recipe_carries_four_layers_with_bed_filter_chain() {
    let scene = SceneCharacter::for_did(TEST_DID);
    let recipe = AmbientRecipe::from_scene(&scene, 1).recipe;
    // Bed + gusts + chimes + punctuation — see the "Sound design"
    // docstring on `seeded_defaults::room::audio`.
    assert_eq!(
        recipe.instruments.len(),
        4,
        "ambient recipe carries the four-layer soundscape"
    );
    // Layer 1 (the bed, instrument 0) is the sustained voice wired
    // noise → biquad filter (LFO driving cutoff) → reverb, with the
    // reverb tail as the graph's output node.
    let patch = &recipe.instruments[0].patch;
    let output_id = patch.graph.output;
    let output_node = patch
        .graph
        .nodes
        .iter()
        .find(|n| n.id == output_id)
        .expect("output node present");
    assert!(
        matches!(output_node.kind, NodeKind::Reverb(_)),
        "bed patch output must be the reverb tail; got {:?}",
        output_node.kind
    );
    // The reverb's `in` port must be fed by the rest of the chain.
    assert!(
        output_node.inputs.contains_key("in"),
        "reverb `in` input must be wired; got inputs {:?}",
        output_node.inputs.keys().collect::<Vec<_>>()
    );
    // The bed filter family is biome-keyed (lowpass for warm biomes,
    // highpass for arid/tundra) — either way the cutoff must ride the
    // LFO sweep.
    let filter = patch
        .graph
        .nodes
        .iter()
        .find(|n| {
            matches!(
                n.kind,
                NodeKind::BiquadLowpass(_) | NodeKind::BiquadHighpass(_)
            )
        })
        .expect("biquad filter present in the bed chain");
    assert!(
        filter.inputs.contains_key("cutoff_hz"),
        "bed filter `cutoff_hz` input must be wired to the LFO; got inputs {:?}",
        filter.inputs.keys().collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Integration with default_for_did — a fresh room carries an audio
// recipe that survives JSON round-trip and parses back to native.
// ---------------------------------------------------------------------------

#[test]
fn default_room_has_seeded_ambient_audio() {
    let r = RoomRecord::default_for_did(TEST_DID);
    let stash = &r.environment.ambient_audio;
    assert!(
        matches!(stash, SovereignAudioConfig::Sequence { .. }),
        "default room must carry a Sequence ambient track; got {}",
        stash.label()
    );
}

#[test]
fn default_room_ambient_parses_back_to_native_recipe() {
    let r = RoomRecord::default_for_did(TEST_DID);
    let parsed: SequenceRecipe = r
        .environment
        .ambient_audio
        .parse_sequence()
        .expect("Sequence variant present");
    // Sanity-check the recipe we authored survives the Fp-quantised
    // structured round trip. Exact equality holds because every
    // float in the seeded recipe is well within Fp's precision and
    // the deriver is deterministic — but to be defensive against
    // tiny rounding artefacts at field boundaries, compare
    // field-by-field where exact equality could falsely diverge.
    let scene = SceneCharacter::for_did(TEST_DID);
    let did_seed = symbios_overlands::seeded_defaults::fnv1a_64(TEST_DID);
    let expected = AmbientRecipe::from_scene(&scene, did_seed).recipe;
    assert_eq!(parsed.sample_rate, expected.sample_rate);
    assert_eq!(parsed.tracks.len(), expected.tracks.len());
    assert_eq!(parsed.instruments.len(), expected.instruments.len());
    assert!(
        (parsed.bpm - expected.bpm).abs() < 0.01,
        "bpm should round-trip within Fp quantisation"
    );
    assert!(
        (parsed.duration_beats - expected.duration_beats).abs() < 0.01,
        "duration should round-trip within Fp quantisation"
    );
}
