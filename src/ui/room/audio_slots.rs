//! From a bridge salt to the slot in a record it names — so a committed
//! audio edit can be landed without its bridge having been drawn (#1337 A5).
//!
//! The pop-out audio editor is deliberately slot-agnostic: it edits a
//! native working copy and stages the converted sovereign value in
//! [`AudioEditorState`]'s pending map under the bound slot's *salt*, and
//! the matching [`draw_audio_bridge`](super::audio::draw_audio_bridge) call
//! site — the only code that holds the live `&mut SovereignAudioConfig` for
//! that slot — picks it up the next time it runs. That indirection is what
//! makes one window serve the world-ambient bed, every construct in a room,
//! the avatar's visuals and a worn prop's parts, and #1202 made the staged
//! value **delivery**: it survives the window closing, the Esc ladder and
//! the bound slot leaving the screen.
//!
//! It survived everything except the end of the session. `RoomEditorState`
//! and `AvatarEditorState` are both reset wholesale on logout
//! (`ui::logout::clear_editor_state_on_logout`), and the unsaved-edits
//! guard asks whether the live records differ from their stored mirrors —
//! which a staged commit has not caused yet, because it has not reached a
//! record. "Edits are kept but not applied yet" was true right up until
//! they were gone, with no warning anywhere.
//!
//! This module is the missing half: the same salt the bridge would have
//! used, asked of the record instead of the screen.
//!
//! # Asked, not parsed
//!
//! A node's salt is [`node_salt`]'s `gen_<root>_<i>_<j>…`, and a root name
//! is the owner's own string — so `gen_tree_1` is either the root called
//! `tree_1` or child 1 of the root called `tree`, and nothing in the salt
//! says which. Every walk here therefore **recomputes** each slot's salt
//! from the node it is standing on and compares, rather than taking a salt
//! apart. That is also what keeps this honest if the salt's shape ever
//! changes: there is one `node_salt`, and both the bridge and this module
//! call it.

use crate::pds::SovereignAudioConfig;
use crate::pds::avatar::{AvatarBody, AvatarRecord};
use crate::pds::generator::Generator;
use crate::pds::room::RoomRecord;
use crate::ui::room::GenNodeId;
use crate::ui::room::audio::AudioEditorState;
use crate::ui::room::generators::node_salt;

/// The salt the world-ambient bed's bridge is drawn under. The one slot
/// that is not a generator node, and the only one with a fixed name.
pub(crate) const ENVIRONMENT_SALT: &str = "environment";

/// Call `visit` for every audio slot in `root`'s tree, root included, with
/// the salt its bridge would be drawn under.
fn visit_tree(
    root_name: &str,
    node: &Generator,
    path: &mut Vec<usize>,
    visit: &mut impl FnMut(String, &SovereignAudioConfig),
) {
    visit(
        node_salt(&GenNodeId {
            root: root_name.to_string(),
            path: path.clone(),
        }),
        &node.audio,
    );
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        visit_tree(root_name, child, path, visit);
        path.pop();
    }
}

/// The `&mut` twin of [`visit_tree`]. Two walks rather than one generic
/// one: a single visitor that could serve both would have to hand out
/// `&mut` to a reader, and the reader here is the unsaved guard, which
/// must not be able to write.
fn visit_tree_mut(
    root_name: &str,
    node: &mut Generator,
    path: &mut Vec<usize>,
    visit: &mut impl FnMut(String, &mut SovereignAudioConfig),
) {
    visit(
        node_salt(&GenNodeId {
            root: root_name.to_string(),
            path: path.clone(),
        }),
        &mut node.audio,
    );
    for (i, child) in node.children.iter_mut().enumerate() {
        path.push(i);
        visit_tree_mut(root_name, child, path, visit);
        path.pop();
    }
}

/// Whether any commit staged in `editor` names a slot reachable from here
/// **and would change it** — the question the unsaved guard asks.
///
/// `roots` is the record's generator trees by the root name their bridge
/// draws them under: the room's `generators` map, the avatar's single
/// `visuals` root, a worn prop's `item` under its record key. A plain
/// iterator rather than the tree panel's [`GeneratorTreeSource`]: that
/// trait hands out `&mut` to everything, and the reader here is the
/// unsaved guard, which must not be able to write.
///
/// Equality matters, not mere presence. A commit that says what the record
/// already says is not unsaved work, and counting it would block a logout
/// behind a save that writes nothing — the dialog everyone learns to click
/// through. A commit whose slot has since been deleted is not unsaved work
/// either: there is nowhere for it to land, so nothing is being lost by
/// leaving.
///
/// [`GeneratorTreeSource`]: crate::ui::room::generators::GeneratorTreeSource
pub(crate) fn pending_would_change<'a>(
    editor: &AudioEditorState,
    environment: Option<&SovereignAudioConfig>,
    roots: impl Iterator<Item = (&'a str, &'a Generator)>,
) -> bool {
    if editor.pending_is_empty() {
        return false;
    }
    if let Some(env) = environment
        && editor
            .peek_pending(ENVIRONMENT_SALT)
            .is_some_and(|staged| staged != env)
    {
        return true;
    }
    let mut differs = false;
    for (name, root) in roots {
        visit_tree(name, root, &mut Vec::new(), &mut |salt, current| {
            if editor
                .peek_pending(&salt)
                .is_some_and(|staged| staged != current)
            {
                differs = true;
            }
        });
    }
    differs
}

/// Land every commit staged in `editor` that names a slot reachable from
/// here, and report how many moved a value.
///
/// The counterpart of the pickup in
/// [`draw_audio_bridge`](super::audio::draw_audio_bridge), doing exactly
/// what it does — assign, and tell the editor its own commit has landed so
/// the record coming back changed does not read as an outside edit
/// (#1333 A9) — for the slots whose bridge is not on screen to do it.
///
/// A staged commit is taken whether or not it changed anything, because
/// taking it is what makes it no longer pending; the count is of the ones
/// that moved a value, which is what a caller reports.
pub(crate) fn land_pending<'a>(
    editor: &mut AudioEditorState,
    environment: Option<&mut SovereignAudioConfig>,
    roots: impl Iterator<Item = (&'a str, &'a mut Generator)>,
) -> usize {
    if editor.pending_is_empty() {
        return 0;
    }
    let mut landed = 0;
    if let Some(env) = environment
        && let Some(staged) = editor.land(ENVIRONMENT_SALT)
    {
        if *env != staged {
            landed += 1;
        }
        *env = staged;
    }
    for (name, root) in roots {
        // The salts first, because the walk holds the tree and `land`
        // needs the editor: one pass to find which slots are spoken for,
        // one to write them.
        let mut wanted: Vec<String> = Vec::new();
        visit_tree(name, root, &mut Vec::new(), &mut |salt, _| {
            if editor.peek_pending(&salt).is_some() {
                wanted.push(salt);
            }
        });
        if wanted.is_empty() {
            continue;
        }
        let staged: Vec<(String, SovereignAudioConfig)> = wanted
            .into_iter()
            .filter_map(|salt| editor.land(&salt).map(|value| (salt, value)))
            .collect();
        visit_tree_mut(name, root, &mut Vec::new(), &mut |salt, current| {
            if let Some((_, value)) = staged.iter().find(|(s, _)| *s == salt) {
                if current != value {
                    landed += 1;
                }
                *current = value.clone();
            }
        });
    }
    landed
}

/// The room record's audio slots, as [`land_pending`] wants them: the
/// world-ambient bed and every generator root. One place that knows the
/// shape, so a caller cannot forget the bed.
pub(crate) fn land_room(editor: &mut AudioEditorState, record: &mut RoomRecord) -> usize {
    let RoomRecord {
        environment,
        generators,
        ..
    } = record;
    land_pending(
        editor,
        Some(&mut environment.ambient_audio),
        generators.iter_mut().map(|(k, v)| (k.as_str(), v)),
    )
}

/// The same question, read-only.
pub(crate) fn room_pending_would_change(editor: &AudioEditorState, record: &RoomRecord) -> bool {
    pending_would_change(
        editor,
        Some(&record.environment.ambient_audio),
        record.generators.iter().map(|(k, v)| (k.as_str(), v)),
    )
}

/// The avatar record's audio slots: the single visuals root under its
/// fixed name, and every worn prop's parts under its record key.
///
/// A worn prop's generator rides the avatar record's `serde(skip)`
/// `resolved` payload — the same field `avatar_is_dirty` exists to see
/// (#1059) — so it is the avatar's record that a commit on a worn part is
/// unsaved work in, not the wardrobe's.
fn avatar_roots(record: &AvatarRecord) -> Vec<(&str, &Generator)> {
    match &record.body {
        AvatarBody::Generator(body) => {
            vec![(crate::pds::avatar::VISUALS_ROOT_NAME, &body.visuals)]
        }
        AvatarBody::Rigged(rig) => rig.resolved.as_ref().map_or_else(Vec::new, |resolved| {
            resolved
                .attachments
                .iter()
                .map(|a| (a.rkey.as_str(), &a.record.item))
                .collect()
        }),
        AvatarBody::Absent | AvatarBody::Unknown => Vec::new(),
    }
}

/// The `&mut` twin of [`avatar_roots`].
fn avatar_roots_mut(record: &mut AvatarRecord) -> Vec<(&str, &mut Generator)> {
    match &mut record.body {
        AvatarBody::Generator(body) => {
            vec![(crate::pds::avatar::VISUALS_ROOT_NAME, &mut body.visuals)]
        }
        AvatarBody::Rigged(rig) => rig.resolved.as_mut().map_or_else(Vec::new, |resolved| {
            resolved
                .attachments
                .iter_mut()
                // Split the attachment rather than borrowing it twice: the
                // key is read while the item is written.
                .map(|a| (a.rkey.as_str(), &mut a.record.item))
                .collect()
        }),
        AvatarBody::Absent | AvatarBody::Unknown => Vec::new(),
    }
}

/// Land every commit staged for a slot in the avatar record.
pub(crate) fn land_avatar(editor: &mut AudioEditorState, record: &mut AvatarRecord) -> usize {
    if editor.pending_is_empty() {
        return 0;
    }
    land_pending(editor, None, avatar_roots_mut(record).into_iter())
}

/// The same question, read-only.
pub(crate) fn avatar_pending_would_change(
    editor: &AudioEditorState,
    record: &AvatarRecord,
) -> bool {
    pending_would_change(editor, None, avatar_roots(record).into_iter())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::audio::SovereignAudioPatch;
    use crate::pds::avatar::VISUALS_ROOT_NAME;

    /// A config that is not `None`, so "it changed" is a real difference
    /// rather than a default comparing equal to a default.
    fn patch(seed: u32) -> SovereignAudioConfig {
        SovereignAudioConfig::Patch {
            patch: SovereignAudioPatch {
                seed,
                ..Default::default()
            },
        }
    }

    fn node(children: Vec<Generator>) -> Generator {
        Generator {
            kind: crate::ui::room::construct::make_default_for_kind("Cuboid"),
            children,
            ..Default::default()
        }
    }

    fn room_with(roots: &[(&str, Generator)]) -> RoomRecord {
        let mut record = RoomRecord::default();
        for (name, generator) in roots {
            record
                .generators
                .insert((*name).to_string(), generator.clone());
        }
        record
    }

    /// The world-ambient bed: the one slot that is not a node.
    #[test]
    fn a_commit_for_the_environment_lands_on_the_environment() {
        let mut record = RoomRecord::default();
        let mut editor = AudioEditorState::default();
        editor.stage_for_test(ENVIRONMENT_SALT, patch(1));

        assert_eq!(land_room(&mut editor, &mut record), 1);
        assert_eq!(record.environment.ambient_audio, patch(1));
        assert!(
            !editor.has_pending(ENVIRONMENT_SALT),
            "the commit is still staged after landing"
        );
    }

    /// A generator node, addressed by the salt its own bridge would use —
    /// which is what makes this independent of the tree ever being drawn.
    #[test]
    fn a_commit_for_a_generator_node_lands_on_that_node() {
        let mut record = room_with(&[("oak", node(vec![node(Vec::new()), node(Vec::new())]))]);
        let salt = node_salt(&GenNodeId::child("oak", vec![1]));
        assert_eq!(salt, "gen_oak_1", "the salt's shape moved under this test");

        let mut editor = AudioEditorState::default();
        editor.stage_for_test(&salt, patch(2));
        assert_eq!(land_room(&mut editor, &mut record), 1);
        assert_eq!(record.generators["oak"].children[1].audio, patch(2));
        assert_eq!(
            record.generators["oak"].children[0].audio,
            SovereignAudioConfig::default(),
            "the sibling was written to as well"
        );
    }

    /// The avatar's visuals tree, whose root has a fixed name and whose
    /// record is a different one entirely. A5's whole point: a fix that
    /// lands only the room's commits still loses the avatar's.
    #[test]
    fn a_commit_for_an_avatar_visuals_slot_lands_on_the_avatar() {
        let mut record = AvatarRecord::wearing("3jzfcijpj2z2a");
        record.body = AvatarBody::generator(node(vec![node(Vec::new())]));
        let salt = node_salt(&GenNodeId::child(VISUALS_ROOT_NAME, vec![0]));

        let mut editor = AudioEditorState::default();
        editor.stage_for_test(&salt, patch(3));
        assert!(avatar_pending_would_change(&editor, &record));
        assert_eq!(land_avatar(&mut editor, &mut record), 1);
        assert_eq!(
            record.body.visuals().expect("a generator body").children[0].audio,
            patch(3)
        );
        assert!(editor.pending_is_empty());
    }

    /// A salt no slot claims is not unsaved work and is not landed
    /// anywhere: the node it named has been deleted, and there is nothing
    /// left to lose.
    #[test]
    fn a_commit_whose_slot_is_gone_lands_nowhere() {
        let mut record = room_with(&[("oak", node(Vec::new()))]);
        let mut editor = AudioEditorState::default();
        editor.stage_for_test("gen_birch_3", patch(4));

        assert!(
            !room_pending_would_change(&editor, &record),
            "a commit with nowhere to land counts as unsaved work"
        );
        assert_eq!(land_room(&mut editor, &mut record), 0);
        assert_eq!(
            record.generators["oak"].audio,
            SovereignAudioConfig::default()
        );
    }

    /// A commit that says what the record already says is not unsaved
    /// work — a guard that blocked on it would offer a save that writes
    /// nothing.
    #[test]
    fn a_commit_that_changes_nothing_is_not_unsaved_work() {
        let mut record = room_with(&[("oak", node(Vec::new()))]);
        record.generators.get_mut("oak").unwrap().audio = patch(5);
        let mut editor = AudioEditorState::default();
        editor.stage_for_test("gen_oak", patch(5));

        assert!(!room_pending_would_change(&editor, &record));
    }

    /// And one that says something else is — for a node and for the bed.
    #[test]
    fn a_commit_that_moves_a_value_is_unsaved_work() {
        let mut record = room_with(&[("oak", node(Vec::new()))]);

        let mut editor = AudioEditorState::default();
        editor.stage_for_test("gen_oak", patch(6));
        assert!(room_pending_would_change(&editor, &record));

        let mut editor = AudioEditorState::default();
        editor.stage_for_test(ENVIRONMENT_SALT, patch(1));
        assert!(room_pending_would_change(&editor, &record));
        record.environment.ambient_audio = patch(1);
        assert!(
            !room_pending_would_change(&editor, &record),
            "the bed's commit still counts once the bed already says it"
        );
    }

    /// Several slots at once, including a nested one, so the walk is
    /// tested rather than the first node it meets.
    #[test]
    fn every_staged_commit_lands_in_one_pass() {
        let mut record = room_with(&[
            (
                "oak",
                node(vec![node(vec![node(Vec::new())]), node(Vec::new())]),
            ),
            ("rock", node(Vec::new())),
        ]);
        let mut editor = AudioEditorState::default();
        editor.stage_for_test("gen_oak", patch(7));
        editor.stage_for_test("gen_oak_0_0", patch(8));
        editor.stage_for_test("gen_rock", patch(9));
        editor.stage_for_test(ENVIRONMENT_SALT, patch(1));

        assert_eq!(land_room(&mut editor, &mut record), 4);
        assert_eq!(record.generators["oak"].audio, patch(7));
        assert_eq!(
            record.generators["oak"].children[0].children[0].audio,
            patch(8)
        );
        assert_eq!(record.generators["rock"].audio, patch(9));
        assert_eq!(record.environment.ambient_audio, patch(1));
        assert!(editor.pending_is_empty(), "something was left staged");
    }

    /// A worn prop's parts are the avatar record's too, and they ride the
    /// `serde(skip)` payload — the half `avatar_is_dirty` exists to see.
    #[test]
    fn a_commit_for_a_worn_props_part_lands_on_the_avatar() {
        // `rigged_seeded` resolves locally, so this needs no wardrobe and
        // no network — the same door the seeded default comes through.
        let mut record = AvatarRecord::wearing("3jzfcijpj2z2a");
        record.body = AvatarBody::rigged_seeded(1);
        let Some(rig) = record.body.rigged_mut() else {
            panic!("a worn body is rigged");
        };
        let Some(resolved) = rig.resolved.as_mut() else {
            panic!("the seeded rig resolves locally");
        };
        resolved
            .attachments
            .push(crate::pds::avatar::ResolvedAttachment {
                rkey: "hat1".to_string(),
                record: crate::pds::avatar::AttachmentRecord::new(
                    node(vec![node(Vec::new())]),
                    symbios_avatar::Socket::Crown,
                ),
            });
        let salt = node_salt(&GenNodeId::child("hat1", vec![0]));

        let mut editor = AudioEditorState::default();
        editor.stage_for_test(&salt, patch(2));
        assert!(avatar_pending_would_change(&editor, &record));
        assert_eq!(land_avatar(&mut editor, &mut record), 1);
        let worn = &record
            .body
            .rigged_ref()
            .unwrap()
            .resolved
            .as_ref()
            .unwrap()
            .attachments[0];
        assert_eq!(worn.record.item.children[0].audio, patch(2));
    }
}
