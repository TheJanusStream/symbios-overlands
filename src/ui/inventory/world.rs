//! Load into my world (#1839): a world item from the Inventory replaces the
//! owner's world, as the World Editor's Seed & re-roll does.
//!
//! The selected world row's Load asks first (`ui::confirm`): it replaces
//! everything - the ground, the sky and everything placed - it can be
//! undone, and nothing is kept until the world is saved. Confirmed, a
//! [`LoadWorldTask`] joins the item's parts - from memory when this session
//! holds them, else fetched - checks and sanitises them
//! ([`crate::pds::inventory::world::load_world`]), and settles the result as
//! every peer would hold it. [`poll_world_load_tasks`] lands it through
//! [`crate::ui::room::replace_world`], the re-roll's own path, whether or
//! not the World Editor is open: one write of `LiveRoomRecord`, so one undo
//! step, and the editor's selections, parked confirms and Raw JSON mirror
//! re-seeded so none of them names the world that was replaced. A failure
//! is a toast and changes nothing.
//!
//! Only in your own world, and one load at a time: the row's Load says why
//! not, painted on its hover.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::pds::RoomRecord;
use crate::pds::inventory::WorldItem;
use crate::state::{CurrentRoomDid, LiveRoomRecord};
use crate::ui::room::RoomEditorState;

/// A world item being loaded (#1839): the joined, checked, sanitised and
/// settled world, or the sentence saying why not. Carries the owner's DID -
/// the world it was asked to load into - and the item's name for the toast.
#[derive(Component)]
pub struct LoadWorldTask {
    pub task: bevy::tasks::Task<Result<RoomRecord, String>>,
    pub did: String,
    pub name: String,
    pub spawned_at: f64,
    /// How long the task may run before the poll declares it dead: a
    /// request's bound per window of parts, beyond the publish deadline.
    pub deadline_secs: f64,
}

/// How each world load ended, for a listener with no screen to read the
/// toast on (#1843): the agent client inserts this to tell its player how a
/// load it asked for went. [`poll_world_load_tasks`] pushes the item's name
/// and the outcome - `Err` holding the toast's reason - only while it is
/// there; a person's client never inserts it, and nothing changes for them.
#[derive(Resource, Default, Debug)]
pub struct WorldLoadsEnded(pub Vec<(String, Result<(), String>)>);

/// Why the selected world cannot be loaded right now, or `None` when it
/// can: it loads into your own world only, and one at a time.
pub(crate) fn world_load_blocked_reason(owns_world: bool, loading: bool) -> Option<&'static str> {
    if !owns_world {
        Some("A world loads into your own world only - go home first.")
    } else if loading {
        Some("A world is loading - wait for it to land.")
    } else {
        None
    }
}

/// The confirmation's title and body for loading `name`.
pub(crate) fn load_confirm_text(name: &str) -> (String, String) {
    (
        format!("Load \"{name}\" into your world?"),
        format!(
            "This replaces everything in your world - the ground, the sky and everything \
             placed - with \"{name}\", as a re-roll does. You can undo it, and nothing is kept \
             until you Save the world in the World Editor."
        ),
    )
}

/// Start loading `item` into `did`'s world.
pub(crate) fn spawn_world_load_task(
    commands: &mut Commands,
    did: &str,
    name: &str,
    item: &WorldItem,
    now: f64,
) {
    // Sizes only: the session log never carries a world's contents.
    info!(
        "Loading a world item: {} parts, {} bytes, {}",
        item.head().parts.len(),
        item.head().bytes,
        if item.slices().is_some() {
            "held in memory"
        } else {
            "fetched"
        }
    );
    // The parts, if held, are shared, not copied.
    let item = item.clone();
    let windows = item
        .head()
        .parts
        .len()
        .div_ceil(crate::pds::inventory::world::PART_FETCH_CONCURRENCY)
        .max(1);
    let deadline_secs = crate::config::http::PUBLISH_TASK_DEADLINE.as_secs_f64()
        + crate::config::http::REQUEST_TIMEOUT.as_secs_f64() * windows as f64;
    let owner = did.to_owned();
    let task = bevy::tasks::IoTaskPool::get().spawn(async move {
        let client = crate::config::http::default_client();
        let mut world = crate::pds::inventory::world::load_world(&client, &owner, &item).await?;
        // Held as every peer holds it, as an editor gesture leaves the
        // world (#1707) - in the task, off the frame.
        crate::ui::room::settle_edit(&mut world);
        Ok(world)
    });
    commands.spawn(LoadWorldTask {
        task,
        did: did.to_owned(),
        name: name.to_owned(),
        spawned_at: now,
        deadline_secs,
    });
}

/// Land a loaded world (#1839): `result` replaces the live world through
/// the re-roll's path ([`crate::ui::room::replace_world`]) - only while the
/// owner is still in the world the load was asked for - as one write, so
/// one undo step labelled for the item. Anything else changes nothing and
/// returns the sentence to show.
pub(crate) fn land_loaded_world(
    name: &str,
    did: &str,
    result: Result<RoomRecord, String>,
    room_did: Option<&str>,
    live: Option<Mut<'_, LiveRoomRecord>>,
    mut editor: Option<Mut<'_, RoomEditorState>>,
    undo_labels: &mut crate::ui::undo::PendingUndoLabels,
) -> Result<(), String> {
    let world = result?;
    if room_did != Some(did) {
        return Err(String::from(
            "you are no longer in your own world, so nothing was loaded",
        ));
    }
    let Some(mut live) = live else {
        return Err(String::from("your world has not loaded yet"));
    };
    // The editor may never have been opened; its view then has nothing to
    // re-seed, and a blank one stands in.
    let mut blank = RoomEditorState::default();
    let view = match editor.as_mut() {
        Some(editor) => editor.world_view(),
        None => blank.world_view(),
    };
    crate::ui::room::replace_world(
        &mut live.0,
        world,
        view,
        undo_labels,
        format!("load {name}"),
    );
    Ok(())
}

/// Poll the world loads in flight (#1839) and land each one.
#[allow(clippy::too_many_arguments)]
pub fn poll_world_load_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut LoadWorldTask)>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut live: Option<ResMut<LiveRoomRecord>>,
    mut editor: Option<ResMut<RoomEditorState>>,
    mut undo_labels: ResMut<crate::ui::undo::PendingUndoLabels>,
    mut toasts: ResMut<crate::notify::Toasts>,
    mut ended: Option<ResMut<WorldLoadsEnded>>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    for (entity, mut task) in tasks.iter_mut() {
        let result =
            match futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task)) {
                Some(result) => result,
                None if now - task.spawned_at > task.deadline_secs => {
                    Err(crate::config::http::timed_out_after(
                        "Loading the world",
                        std::time::Duration::from_secs_f64(task.deadline_secs),
                    ))
                }
                None => continue,
            };
        commands.entity(entity).despawn();
        if crate::ui::room::stale_result(
            "Loading a world",
            &task.did,
            session.as_deref().map(|s| s.did.as_str()),
        ) {
            continue;
        }
        let name = task.name.clone();
        let landed = land_loaded_world(
            &name,
            &task.did,
            result,
            room_did.as_deref().map(|room| room.0.as_str()),
            live.as_mut().map(|live| live.reborrow()),
            editor.as_mut().map(|editor| editor.reborrow()),
            &mut undo_labels,
        );
        if let Some(ended) = ended.as_mut() {
            ended.0.push((name.clone(), landed.clone()));
        }
        match landed {
            Ok(()) => {
                commands.insert_resource(crate::world_builder::RebuildAnnounce {
                    what: format!("Loaded \"{name}\""),
                    since_secs: now,
                });
                toasts.success(
                    format!(
                        "Loaded \"{name}\" into your world - Save in the World Editor to keep it, \
                         or undo it."
                    ),
                    now,
                );
            }
            Err(reason) => toasts.warn(format!("Could not load \"{name}\" - {reason}"), now),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::inventory::world::tests::seeded_world;
    use crate::ui::room::placement_focus_tests::session;
    use crate::ui::undo::{PendingUndoLabels, RoomUndoHistory};
    use bevy::ecs::system::RunSystemOnce;

    const OWNER: &str = "did:plc:worlds";

    fn value(record: &RoomRecord) -> serde_json::Value {
        serde_json::to_value(record).expect("serializes")
    }

    /// An owner in their own world with the World Editor holding a
    /// selection, a multi-selection and a seeded Raw JSON tab, and the room
    /// undo ring watching - as the app runs it.
    fn app_with(world: &RoomRecord) -> App {
        let mut app = App::new();
        app.insert_resource(LiveRoomRecord(world.clone()));
        app.insert_resource(session(OWNER));
        app.insert_resource(CurrentRoomDid(String::from(OWNER)));
        let mut editor = RoomEditorState::default();
        editor.selected_placement = Some(0);
        editor.extra_placements = vec![1];
        editor.tree.selection.root = world.generators.keys().next().cloned();
        editor.tree.selection.path = Some(Vec::new());
        editor.seed_raw_for_test(world);
        app.insert_resource(editor);
        app.init_resource::<RoomUndoHistory>();
        app.init_resource::<crate::state::RoomWriteSignals>();
        app.init_resource::<PendingUndoLabels>();
        app.add_systems(Update, crate::ui::undo::capture_room_history);
        app.update();
        app
    }

    /// Land `result` as the poll does, with the owner in `room`.
    fn land(app: &mut App, result: Result<RoomRecord, String>, room: &str) -> Result<(), String> {
        let room = room.to_owned();
        app.world_mut()
            .run_system_once(
                move |live: Option<ResMut<LiveRoomRecord>>,
                      editor: Option<ResMut<RoomEditorState>>,
                      mut labels: ResMut<PendingUndoLabels>| {
                    land_loaded_world(
                        "harbour",
                        OWNER,
                        result.clone(),
                        Some(room.as_str()),
                        live.map(Into::into),
                        editor.map(Into::into),
                        &mut labels,
                    )
                },
            )
            .expect("the land runs")
    }

    /// Load replaces `LiveRoomRecord` through the re-roll's own path: the
    /// loaded world, as one undo step that one undo takes back, with no
    /// World Editor selection left naming the world it replaced and the
    /// Raw JSON tab showing the new one.
    #[test]
    fn a_loaded_world_replaces_the_live_one_in_one_undo_step_with_no_stale_selection() {
        let old = seeded_world(31);
        let loaded = {
            let item =
                crate::pds::inventory::WorldItem::snapshot(&seeded_world(32)).expect("copies");
            let mut world =
                crate::pds::inventory::world::join_parts(item.head(), item.slices().expect("held"))
                    .expect("loads");
            crate::ui::room::settle_edit(&mut world);
            world
        };
        let mut app = app_with(&old);
        let before = app.world().resource::<RoomUndoHistory>().len();

        assert_eq!(land(&mut app, Ok(loaded.clone()), OWNER), Ok(()));
        app.update();
        app.update();

        assert_eq!(
            value(&app.world().resource::<LiveRoomRecord>().0),
            value(&loaded),
            "the loaded world is the live one"
        );
        let editor = app.world().resource::<RoomEditorState>();
        assert!(!editor.has_selection(), "no selection names the old world");
        assert!(editor.extra_placements.is_empty());
        assert_eq!(
            editor.raw_text_for_test(),
            serde_json::to_string_pretty(&loaded).expect("serializes"),
            "the Raw JSON tab shows the loaded world"
        );
        let mut history = app.world_mut().resource_mut::<RoomUndoHistory>();
        assert_eq!(history.len(), before + 1, "one undo step");
        let (entry, label) = history.undo().expect("undoable");
        assert_eq!(label, "load harbour");
        assert_eq!(value(&entry.record), value(&old), "one undo takes it back");
    }

    /// A loaded world closes the audio editor and drops the commits it had
    /// staged: they are keyed by a slot's salt, and the loaded world may
    /// give that salt to another slot - landing one would write the old
    /// world's sound into the new one.
    #[test]
    fn a_loaded_world_closes_the_audio_editor_and_drops_its_commits() {
        let old = seeded_world(35);
        let mut app = app_with(&old);
        {
            let mut editor = app.world_mut().resource_mut::<RoomEditorState>();
            editor
                .audio_editor
                .stage_for_test("room_ambient", crate::pds::SovereignAudioConfig::None);
            editor.audio_editor.open = true;
        }
        assert_eq!(land(&mut app, Ok(seeded_world(36)), OWNER), Ok(()));
        let editor = app.world().resource::<RoomEditorState>();
        assert!(!editor.audio_editor.open, "the window is closed");
        assert!(
            editor.audio_editor.pending_is_empty(),
            "no commit for the old world is left to land in the new one"
        );
    }

    /// A load that failed, or one that lands after the owner left their
    /// world, changes nothing: the live world, its undo ring and the
    /// editor's selection are all as they were.
    #[test]
    fn a_failed_or_misplaced_load_changes_nothing() {
        let old = seeded_world(33);
        let mut app = app_with(&old);
        let before = app.world().resource::<RoomUndoHistory>().len();
        let hostile = crate::pds::inventory::world::join_parts(
            &crate::pds::inventory::WorldHead::default(),
            &[],
        )
        .map(|_| old.clone());
        assert!(hostile.is_err(), "a hostile head fails the load");
        assert!(land(&mut app, hostile, OWNER).is_err());
        let elsewhere = land(&mut app, Ok(seeded_world(34)), "did:plc:someone-else");
        assert!(elsewhere.is_err_and(|why| why.contains("no longer in your own world")));
        app.update();
        app.update();
        assert_eq!(
            value(&app.world().resource::<LiveRoomRecord>().0),
            value(&old)
        );
        assert_eq!(app.world().resource::<RoomUndoHistory>().len(), before);
        assert!(app.world().resource::<RoomEditorState>().has_selection());
    }

    /// The row's Load is offered in your own world, one load at a time.
    #[test]
    fn load_says_why_it_cannot() {
        assert_eq!(world_load_blocked_reason(true, false), None);
        assert!(
            world_load_blocked_reason(false, false).is_some_and(|why| why.contains("own world"))
        );
        assert!(world_load_blocked_reason(true, true).is_some_and(|why| why.contains("loading")));
        let (title, body) = load_confirm_text("harbour");
        assert!(title.contains("\"harbour\""));
        for promise in [
            "replaces everything",
            "ground",
            "sky",
            "re-roll",
            "undo",
            "Save",
        ] {
            assert!(body.contains(promise), "{promise}: {body}");
        }
    }
}
