//! `agent avatar get` and `agent avatar set` (#1422): the agent's avatar as
//! JSON, read and written a part at a time as the world's record is (see
//! [`mod@super::json`]).
//!
//! An avatar is more than its record. A rigged body - every seeded person -
//! keeps its sculpt, and each item it wears, in records of their own that
//! the avatar record only names; the game fetches them beside it and holds
//! them where its wire form cannot see them. So the avatar's JSON has three
//! parts, each in its own wire form:
//!
//! * `record` - the avatar record: its body's kind and references, how it
//!   moves (`locomotion`) and idles (`gait`), and a vehicle's whole body
//!   (`body.visuals`, for a generator body);
//! * `body` - a rigged body's sculpt, the engine's own record; `null` for
//!   any other body;
//! * `worn` - the items a rigged body wears, each with the key of the record
//!   it is kept in, in the order they are drawn.
//!
//! Setting a part rebuilds all three and writes them as one edit, sanitised
//! as the game sanitises an avatar it fetches. What the JSON cannot do is
//! change which records the avatar names: wearing or taking something off,
//! swapping the body for another saved one, or changing its kind. Those
//! parts are refused rather than written half-way.
//!
//! Who sees an edit, and when: the record goes to everyone in the world at
//! once, as the avatar editor's edits do; a rigged body's sculpt and its
//! worn items travel as references, so others see them once they are saved.

use bevy::prelude::*;
use serde_json::{Value, json};

use crate::pds::avatar::{AttachmentRecord, EngineAvatarRecord, ResolvedAttachment, ResolvedRig};
use crate::pds::types::Fp3;
use crate::pds::{AvatarRecord, Generator};
use crate::state::LiveAvatarRecord;

use super::json::{part, set_part, unreadable};

/// The avatar's JSON, or the part of it at `pointer`.
pub(super) fn get(world: &mut World, pointer: &str) -> Result<Value, String> {
    super::in_world(world)?;
    let unsaved = super::avatar_unsaved(world);
    let live = &world
        .get_resource::<LiveAvatarRecord>()
        .ok_or("the agent has no avatar record yet")?
        .0;
    let document = to_json(live)?;
    Ok(json!({
        "pointer": pointer,
        "value": part(&document, pointer)?,
        "unsaved": unsaved,
    }))
}

/// Replace the part of the avatar's JSON at `pointer` with `value`.
pub(super) fn set(world: &mut World, pointer: &str, value: Value) -> Result<Value, String> {
    super::in_world(world)?;
    let live = world
        .get_resource::<LiveAvatarRecord>()
        .ok_or("the agent has no avatar record yet")?
        .0
        .clone();
    let mut document = to_json(&live)?;
    set_part(&mut document, pointer, value.clone())?;
    let edited = from_json(&live, document)?;
    let sent = to_json(&edited)
        .ok()
        .and_then(|document| document.pointer(pointer).cloned());
    let label = if pointer.is_empty() {
        "JSON set of the whole avatar".to_owned()
    } else {
        format!("JSON set of {pointer}")
    };
    let seen = if pointer == "/record" || pointer.starts_with("/record/") {
        "now"
    } else {
        "once saved"
    };
    let changed = super::write_avatar(world, edited, label)?;
    let kept = to_json(&world.resource::<LiveAvatarRecord>().0)
        .ok()
        .and_then(|document| document.pointer(pointer).cloned());
    let adjusted_at = super::json::adjustments(pointer, sent.as_ref(), kept.as_ref());
    let mut answer = json!({
        "changed": changed,
        "pointer": pointer,
        "adjusted": !adjusted_at.is_empty(),
        "kept": kept,
        "others_see_it": seen,
        "record_size": super::size::avatar(&world.resource::<LiveAvatarRecord>().0),
    });
    if !adjusted_at.is_empty() {
        answer["adjusted_at"] = json!(adjusted_at);
    }
    Ok(answer)
}

/// Where a generator body's drawn parts are in the avatar's JSON.
pub(super) const BODY_VISUALS: &str = "/record/body/visuals";

/// The part of the avatar's body at `pointer` - a node of a generator
/// body's tree, as `avatar get` shows it - as a generator of its own, the
/// nodes under it included (#1444). Where it sat on the body means nothing
/// anywhere else, so its own translation is dropped: its origin becomes
/// the generator's, which stands on the ground where it is placed. Its
/// turn and its scale stay.
pub(super) fn body_part(world: &World, pointer: &str) -> Result<Generator, String> {
    let live = &world
        .get_resource::<LiveAvatarRecord>()
        .ok_or("the agent has no avatar record yet")?
        .0;
    if live.body.rigged_ref().is_some() {
        return Err(
            "a rigged body keeps its shape in a sculpt, not in parts; only a generator \
             body - a vehicle's - has parts to stash"
                .to_owned(),
        );
    }
    let in_body = pointer
        .strip_prefix(BODY_VISUALS)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'));
    if !in_body {
        return Err(format!(
            "{pointer:?} is not a part of the avatar's body: a part is a node under \
             {BODY_VISUALS}, as `agent avatar get {BODY_VISUALS}` shows it"
        ));
    }
    let value = part(&to_json(live)?, pointer)?;
    let mut generator: Generator = serde_json::from_value(value)
        .map_err(|e| format!("{pointer} is not a node of the body's tree: {e}"))?;
    generator.transform.translation = Fp3([0.0; 3]);
    Ok(generator)
}

/// The avatar's three parts, each in its wire form.
fn to_json(avatar: &AvatarRecord) -> Result<Value, String> {
    let record =
        serde_json::to_value(avatar).map_err(|e| format!("the avatar does not serialise: {e}"))?;
    let resolved = avatar
        .body
        .rigged_ref()
        .and_then(|rig| rig.resolved.as_ref());
    let body = resolved
        .map(|rig| serde_json::to_value(&rig.body))
        .transpose()
        .map_err(|e| format!("the body does not serialise: {e}"))?;
    let worn = resolved
        .map(|rig| {
            rig.attachments
                .iter()
                .map(|worn| {
                    Ok(json!({
                        "rkey": worn.rkey,
                        "item": serde_json::to_value(&worn.record)?,
                    }))
                })
                .collect::<Result<Vec<Value>, serde_json::Error>>()
        })
        .transpose()
        .map_err(|e| format!("a worn item does not serialise: {e}"))?
        .unwrap_or_default();
    Ok(json!({ "record": record, "body": body, "worn": worn }))
}

/// The avatar `document` describes, as an edit of `live`: each part read
/// back from its wire form and the whole sanitised - or why not, when the
/// edit would change which records the avatar names.
fn from_json(live: &AvatarRecord, document: Value) -> Result<AvatarRecord, String> {
    let Value::Object(mut parts) = document else {
        return Err("the avatar is an object of record, body and worn".to_owned());
    };
    let record_json = parts.remove("record").unwrap_or_default();
    let mut record: AvatarRecord = serde_json::from_value(record_json.clone())
        .map_err(|e| unreadable::<AvatarRecord>("an avatar record", &e, &record_json, "/record"))?;
    match (live.body.rigged_ref(), record.body.rigged_mut()) {
        (Some(was), Some(rig)) => {
            if rig.avatar != was.avatar {
                return Err(
                    "the body is the saved one the avatar names; wearing another saved body \
                     is not an edit of the JSON"
                        .to_owned(),
                );
            }
            if let Some(resolved) = was.resolved.as_ref() {
                let body: EngineAvatarRecord =
                    serde_json::from_value(parts.remove("body").unwrap_or_default())
                        .map_err(|e| format!("that is not a body: {e}"))?;
                let worn = worn_from_json(parts.remove("worn").unwrap_or_default())?;
                let names = |worn: &[ResolvedAttachment]| {
                    worn.iter().map(|w| w.rkey.clone()).collect::<Vec<_>>()
                };
                if names(&worn) != names(&resolved.attachments)
                    || rig.attachments != was.attachments
                {
                    return Err(
                        "what the avatar wears, and in what order, is not an edit of the JSON: \
                         each worn item's rkey has to stay as it is"
                            .to_owned(),
                    );
                }
                rig.resolved = Some(ResolvedRig {
                    body,
                    attachments: worn,
                });
            } else if rig.attachments != was.attachments {
                return Err("what the avatar wears is not an edit of the JSON".to_owned());
            }
        }
        (None, None) => {}
        _ => {
            return Err("what kind of body the avatar has is not an edit of the JSON".to_owned());
        }
    }
    record.sanitize();
    Ok(record)
}

/// The worn items, each an rkey and an attachment record.
fn worn_from_json(worn: Value) -> Result<Vec<ResolvedAttachment>, String> {
    let Value::Array(items) = worn else {
        return Err("worn is a list of { rkey, item }".to_owned());
    };
    items
        .into_iter()
        .map(|entry| {
            let rkey = entry["rkey"]
                .as_str()
                .ok_or("each worn item has an rkey")?
                .to_owned();
            let record: AttachmentRecord = serde_json::from_value(entry["item"].clone())
                .map_err(|e| format!("worn item {rkey} is not an attachment record: {e}"))?;
            Ok(ResolvedAttachment { rkey, record })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::harness::{AGENT, app_in};
    use super::*;
    use crate::state::StoredAvatarRecord;

    /// A seeded avatar with a rigged body whose sculpt is in hand, and one
    /// with a generator body - found by DID, as the seeds fall.
    fn seeded(rigged: bool) -> AvatarRecord {
        (0..200)
            .map(|n| AvatarRecord::default_for_did(&format!("did:plc:avatarkind{n}")))
            .find(|avatar| {
                let resolved = avatar
                    .body
                    .rigged_ref()
                    .is_some_and(|rig| rig.resolved.is_some());
                resolved == rigged && (rigged || avatar.body.visuals().is_some())
            })
            .expect("a seeded avatar of that kind")
    }

    fn wearing(app: &mut App, avatar: AvatarRecord) {
        app.world_mut()
            .insert_resource(LiveAvatarRecord(avatar.clone()));
        app.world_mut().insert_resource(StoredAvatarRecord(avatar));
        app.update();
    }

    fn get_all(app: &mut App) -> Value {
        get(app.world_mut(), "").expect("read")["value"].clone()
    }

    /// Read whole and written back whole, an avatar of either kind is
    /// unchanged - nothing to rebuild, no step to undo. For a rigged body
    /// the sculpt is part of what is read, not lost on the way back.
    #[test]
    fn an_avatar_read_and_written_back_is_unchanged() {
        for rigged in [true, false] {
            let (mut app, _) = app_in(AGENT);
            wearing(&mut app, seeded(rigged));
            let whole = get_all(&mut app);
            assert_eq!(whole["body"].is_null(), !rigged, "{rigged}");

            let set = set(app.world_mut(), "", whole).expect("written");

            assert_eq!(set["changed"], false, "rigged {rigged}: {set}");
        }
    }

    /// A body part that will not read is named by its pointer in the
    /// avatar's JSON (#1446) - the live case: a torus without its
    /// `minor_resolution`, one node of a whole new body.
    #[test]
    fn a_refused_set_names_the_body_part_that_would_not_read() {
        let (mut app, _) = app_in(AGENT);
        wearing(&mut app, seeded(false));
        let torus = json!({
            "$type": "network.symbios.gen.torus",
            "major_radius": 2000,
            "minor_radius": 350,
            "major_resolution": 32,
            "solid": false,
        });

        let refused =
            set(app.world_mut(), "/record/body/visuals/children/0", torus).expect_err("refused");

        assert!(
            refused.contains("in the node at /record/body/visuals/children/0;"),
            "{refused}"
        );
    }

    /// A value the record leaves out is no adjustment (#1438): `gait` set
    /// to null is the record's own default, written by leaving it out, and
    /// was answered as `adjusted` because nothing came back to compare.
    #[test]
    fn a_value_the_record_leaves_out_is_not_an_adjustment() {
        let (mut app, _) = app_in(AGENT);
        wearing(&mut app, seeded(false));

        let set = set(app.world_mut(), "/record/gait", Value::Null).expect("written");

        assert_eq!(set["adjusted"], false, "{set}");
        assert!(set.get("adjusted_at").is_none(), "{set}");
    }

    /// A rigged body's sculpt is edited through `body`, and others see it
    /// once it is saved; the record's own parts they see at once.
    #[test]
    fn a_sculpt_is_edited_and_seen_once_saved() {
        let (mut app, _) = app_in(AGENT);
        wearing(&mut app, seeded(true));
        let body = get_all(&mut app)["body"].clone();
        let (pointer, value) = first_number(&body, "/body").expect("a number in the body");

        let set = set(app.world_mut(), &pointer, json!(value + 1)).expect("written");

        assert_eq!(set["changed"], true, "{pointer}: {set}");
        assert_eq!(set["others_see_it"], "once saved");
        let resolved = app.world().resource::<LiveAvatarRecord>().0.clone();
        assert!(
            resolved
                .body
                .rigged_ref()
                .is_some_and(|rig| rig.resolved.is_some()),
            "the sculpt is still in hand"
        );
    }

    /// `avatar`, wearing one wearable catalogue item under a record key of
    /// its own.
    fn dressed(mut avatar: AvatarRecord) -> AvatarRecord {
        let wearable = crate::catalogue::ENTRIES
            .iter()
            .find(|entry| entry.wear_socket().is_some())
            .expect("a wearable entry");
        let record = AttachmentRecord {
            lex_type: crate::pds::AVATAR_ATTACHMENT_COLLECTION.into(),
            item: wearable.build(AGENT),
            socket: "head".into(),
            offset: Default::default(),
            fit_band_mm: 0,
            source: None,
        };
        let rig = avatar.body.rigged_mut().expect("a rigged body");
        rig.attachments.push(WORN.into());
        rig.resolved
            .as_mut()
            .expect("its sculpt")
            .attachments
            .push(ResolvedAttachment {
                rkey: WORN.into(),
                record,
            });
        avatar.sanitize();
        avatar
    }

    const WORN: &str = "3kaaaaaaaaaaa";

    /// Which records the avatar names is not the JSON's to change: another
    /// saved body, another kind of body, or what it wears - a worn item
    /// renamed, one added, all taken off. Its worn item's own record is.
    #[test]
    fn the_records_an_avatar_names_are_not_json_edits() {
        let (mut app, _) = app_in(AGENT);
        wearing(&mut app, dressed(seeded(true)));
        let before = get_all(&mut app);
        assert_eq!(before["worn"][0]["rkey"], WORN);

        let other_body = set(
            app.world_mut(),
            "/record/body/avatar",
            json!("3kzzzzzzzzzzz"),
        );
        assert!(other_body.unwrap_err().contains("another saved body"));
        let other_kind = set(
            app.world_mut(),
            "/record/body",
            serde_json::to_value(&seeded(false).body).expect("serialises"),
        );
        assert!(other_kind.unwrap_err().contains("kind of body"));
        let item = before["worn"][0]["item"].clone();
        for (pointer, value) in [
            ("/worn/0/rkey", json!("3kbbbbbbbbbbb")),
            ("/worn/-", json!({ "rkey": "3kbbbbbbbbbbb", "item": item })),
            ("/worn", json!([])),
        ] {
            let why = set(app.world_mut(), pointer, value).expect_err(pointer);
            assert!(why.contains("rkey has to stay"), "{pointer}: {why}");
        }
        assert_eq!(get_all(&mut app), before, "nothing written");

        let moved = set(app.world_mut(), "/worn/0/item/socket", json!("chest"));
        assert_eq!(moved.expect("written")["changed"], true);
    }

    /// The first whole number in `value`, depth first, and its pointer.
    fn first_number(value: &Value, at: &str) -> Option<(String, i64)> {
        match value {
            Value::Number(n) => n.as_i64().map(|n| (at.to_owned(), n)),
            Value::Object(members) => members.iter().find_map(|(key, member)| {
                let key = key.replace('~', "~0").replace('/', "~1");
                first_number(member, &format!("{at}/{key}"))
            }),
            Value::Array(items) => items
                .iter()
                .enumerate()
                .find_map(|(i, item)| first_number(item, &format!("{at}/{i}"))),
            _ => None,
        }
    }
}
