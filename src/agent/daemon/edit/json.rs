//! `agent room get` and `agent room set` (#1422): the world's record as the
//! JSON the World Editor's Raw JSON tab shows, read and written a part at a
//! time.
//!
//! The JSON is the record's wire form, exactly: every decimal a whole number
//! of ten-thousandths, rotations quaternions scaled the same way, 64-bit
//! seeds quoted strings - so a value with a decimal point in it is refused,
//! as the Raw JSON tab refuses one. A part is named by a JSON pointer (RFC
//! 6901): `/environment/fog_visibility`, `/placements/3`, `/generators/oak`,
//! or nothing at all for the whole record. Setting a part rebuilds the
//! record from its JSON and writes it as every other edit is written - see
//! [`super`] - and the answer shows what the world kept, which the
//! sanitiser may have pulled back into range.
//!
//! Reading works in any world; the record of someone else's is their words
//! and says whose (`named_by`). Writing works in the agent's own.

use bevy::prelude::*;
use serde_json::{Value, json};

use crate::pds::RoomRecord;
use crate::state::{CurrentRoomDid, LiveRoomRecord};

/// How the JSON's numbers are written, for a value that would not read.
pub(super) const WIRE_FORM: &str = "numbers are written as the record stores them: whole \
     numbers, a decimal scaled by 10 000 (1.5 is 15000), rotations quaternions [x, y, z, w] \
     scaled the same way, and 64-bit seeds as quoted strings";

/// The world's record, or the part of it at `pointer`.
pub(super) fn room_get(world: &mut World, pointer: &str) -> Result<Value, String> {
    super::in_world(world)?;
    let unsaved = super::room_unsaved(world);
    let named_by = world
        .get_resource::<CurrentRoomDid>()
        .map(|room| room.0.clone());
    let record = &world
        .get_resource::<LiveRoomRecord>()
        .ok_or("the world has no record yet")?
        .0;
    let document =
        serde_json::to_value(record).map_err(|e| format!("the record does not serialise: {e}"))?;
    Ok(json!({
        "named_by": named_by,
        "pointer": pointer,
        "value": part(&document, pointer)?,
        "unsaved": unsaved,
    }))
}

/// Replace the part of the agent's world's record at `pointer` with
/// `value`.
pub(super) fn room_set(world: &mut World, pointer: &str, value: Value) -> Result<Value, String> {
    let record = super::room_for_edit(world)?;
    let mut document =
        serde_json::to_value(&record).map_err(|e| format!("the record does not serialise: {e}"))?;
    set_part(&mut document, pointer, value.clone())?;
    let edited: RoomRecord = serde_json::from_value(document)
        .map_err(|e| format!("that is not a world record: {e}; {WIRE_FORM}"))?;
    let label = if pointer.is_empty() {
        "JSON set of the whole record".to_owned()
    } else {
        format!("JSON set of {pointer}")
    };
    let changed = super::write_room(world, edited, label)?;
    let kept = serde_json::to_value(&world.resource::<LiveRoomRecord>().0)
        .ok()
        .and_then(|document| document.pointer(pointer).cloned());
    Ok(json!({
        "changed": changed,
        "pointer": pointer,
        "adjusted": kept.as_ref() != Some(&value),
        "kept": kept,
    }))
}

/// The part of `document` at `pointer` (RFC 6901).
pub(super) fn part(document: &Value, pointer: &str) -> Result<Value, String> {
    check_pointer(pointer)?;
    document
        .pointer(pointer)
        .cloned()
        .ok_or_else(|| format!("nothing is at {pointer}"))
}

/// Put `value` at `pointer` (RFC 6901) in `document`. An empty pointer is
/// the whole document. The last step may name a member an object does not
/// have yet, which adds it, or - as `-`, or the index one past the end -
/// the end of a list, which appends to it; everything before it has to
/// exist.
pub(super) fn set_part(document: &mut Value, pointer: &str, value: Value) -> Result<(), String> {
    check_pointer(pointer)?;
    let Some((parent, last)) = pointer.rsplit_once('/') else {
        *document = value;
        return Ok(());
    };
    let container = document.pointer_mut(parent).ok_or_else(|| {
        format!(
            "nothing is at {}",
            if parent.is_empty() { "/" } else { parent }
        )
    })?;
    let key = last.replace("~1", "/").replace("~0", "~");
    match container {
        Value::Object(members) => {
            members.insert(key, value);
        }
        Value::Array(items) if key == "-" => items.push(value),
        Value::Array(items) => {
            let index: usize = key
                .parse()
                .map_err(|_| format!("{parent} is a list, and {key:?} is not an index in it"))?;
            match index.cmp(&items.len()) {
                std::cmp::Ordering::Less => items[index] = value,
                std::cmp::Ordering::Equal => items.push(value),
                std::cmp::Ordering::Greater => {
                    return Err(format!(
                        "{parent} holds {} items, so there is no index {index} to set",
                        items.len()
                    ));
                }
            }
        }
        _ => {
            return Err(format!(
                "{} is a single value, with no parts to set",
                if parent.is_empty() { "/" } else { parent }
            ));
        }
    }
    Ok(())
}

/// A pointer is empty, for the whole document, or starts with a `/`.
fn check_pointer(pointer: &str) -> Result<(), String> {
    if pointer.is_empty() || pointer.starts_with('/') {
        Ok(())
    } else {
        Err(format!(
            "{pointer:?} is not a JSON pointer: one starts with / (as in /environment), \
             or is empty for the whole record"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pointer_reads_a_part_and_nothing_reads_the_whole() {
        let document = json!({ "a": { "b": [10, 20] }, "c/d": 1 });
        assert_eq!(part(&document, "/a/b/1").unwrap(), json!(20));
        assert_eq!(part(&document, "/c~1d").unwrap(), json!(1));
        assert_eq!(part(&document, "").unwrap(), document);
        assert!(
            part(&document, "/a/x")
                .unwrap_err()
                .contains("nothing is at /a/x")
        );
        assert!(part(&document, "a").unwrap_err().contains("starts with /"));
    }

    /// Replace, add a member, append two ways, and the refusals: a missing
    /// parent, an index past the end, a part of a single value.
    #[test]
    fn a_pointer_sets_replaces_adds_and_appends() {
        let mut document = json!({ "a": { "b": [10, 20] }, "n": 5 });
        set_part(&mut document, "/a/b/0", json!(11)).unwrap();
        set_part(&mut document, "/a/new", json!("x")).unwrap();
        set_part(&mut document, "/a/b/-", json!(30)).unwrap();
        set_part(&mut document, "/a/b/3", json!(40)).unwrap();
        assert_eq!(
            document,
            json!({ "a": { "b": [11, 20, 30, 40], "new": "x" }, "n": 5 })
        );

        assert!(set_part(&mut document, "/missing/x", json!(1)).is_err());
        assert!(set_part(&mut document, "/a/b/9", json!(1)).is_err());
        assert!(set_part(&mut document, "/n/x", json!(1)).is_err());
        assert!(set_part(&mut document, "/a/b/x", json!(1)).is_err());

        set_part(&mut document, "", json!([])).unwrap();
        assert_eq!(document, json!([]));
    }
}

#[cfg(test)]
mod world_tests {
    use super::super::harness::{AGENT, app_in};
    use super::*;

    fn fog(app: &App) -> Value {
        serde_json::to_value(&app.world().resource::<LiveRoomRecord>().0.environment)
            .expect("serialises")["fog_visibility"]
            .clone()
    }

    /// A value is written in the record's own wire form: a whole number of
    /// ten-thousandths is taken, a decimal point is refused with how the
    /// numbers go, and the world is left as it was.
    #[test]
    fn a_set_takes_the_wire_form_and_refuses_a_decimal() {
        let (mut app, _) = app_in(AGENT);
        let pointer = "/environment/fog_visibility";
        let before = fog(&app);

        let refused = room_set(app.world_mut(), pointer, json!(123.5)).expect_err("refused");
        assert!(refused.contains("10 000"), "{refused}");
        assert_eq!(fog(&app), before);

        let set = room_set(app.world_mut(), pointer, json!(1_234_500)).expect("set");
        assert_eq!(set["changed"], true);
        assert_eq!(set["adjusted"], false);
        assert_eq!(fog(&app), json!(1_234_500));
        let read = room_get(app.world_mut(), pointer).expect("read");
        assert_eq!(read["value"], json!(1_234_500));
        assert_eq!(read["unsaved"], true);
    }

    /// What the sanitiser pulls back into range is what the world keeps,
    /// and the answer says so rather than echoing what was sent.
    #[test]
    fn a_value_out_of_range_is_kept_in_range_and_said_to_be() {
        let (mut app, _) = app_in(AGENT);
        let sent = json!(i64::from(i32::MAX));

        let set =
            room_set(app.world_mut(), "/environment/fog_visibility", sent.clone()).expect("set");

        assert_eq!(set["adjusted"], true, "{set}");
        assert_ne!(set["kept"], sent);
        assert_eq!(set["kept"], fog(&app));
    }
}
