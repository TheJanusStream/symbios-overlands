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
//!
//! A set that changes a generator also checks it for faces drawn twice in
//! one place - two of its primitives sharing a plane and a facing direction
//! where it can be seen - and names each pair by pointer (`z_fighting`):
//! they flicker as anyone moves, which a still picture barely shows (#1436,
//! [`super::zfight`]).

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
    let before = super::settle_room(record)?;
    let edited: RoomRecord = serde_json::from_value(document)
        .map_err(|e| format!("that is not a world record: {e}; {WIRE_FORM}"))?;
    let sent = serde_json::to_value(&edited)
        .ok()
        .and_then(|document| document.pointer(pointer).cloned());
    let label = if pointer.is_empty() {
        "JSON set of the whole record".to_owned()
    } else {
        format!("JSON set of {pointer}")
    };
    let changed = super::write_room(world, edited, label)?;
    let live = &world.resource::<LiveRoomRecord>().0;
    let kept = serde_json::to_value(live)
        .ok()
        .and_then(|document| document.pointer(pointer).cloned());
    // Against the record as it was, settled as the write settled the new
    // one: the first write of a never-saved world puts every generator on
    // the wire's grid, and none of those is the set's to answer for.
    let (z_fighting, found) = super::zfight::report(&before.generators, &live.generators);
    let named = z_fighting.len();
    let adjusted_at = adjustments(pointer, sent.as_ref(), kept.as_ref());
    let mut answer = json!({
        "changed": changed,
        "pointer": pointer,
        "adjusted": !adjusted_at.is_empty(),
        "kept": kept,
        "z_fighting": z_fighting,
    });
    if !adjusted_at.is_empty() {
        answer["adjusted_at"] = json!(adjusted_at);
    }
    if found > named {
        answer["z_fighting_total"] = json!(found);
    }
    Ok(answer)
}

/// At most this many adjusted places are named in one answer.
const MAX_ADJUSTMENTS: usize = 16;

/// Where the world kept something other than what was sent (#1438): the
/// pointer, under `pointer`, of each value the sanitiser changed, added or
/// took away. `sent` is what was sent as the record writes it - read in and
/// written out again, before sanitising - so a value the record leaves out
/// because it is the default is no adjustment: a raw comparison called one
/// in the live garage build, where a material's default roughness was
/// written and then, rightly, left out.
pub(super) fn adjustments(
    pointer: &str,
    sent: Option<&Value>,
    kept: Option<&Value>,
) -> Vec<String> {
    let mut at = Vec::new();
    match (sent, kept) {
        (Some(sent), Some(kept)) => differences(sent, kept, pointer, &mut at),
        (None, None) => {}
        _ => at.push(pointer.to_owned()),
    }
    at
}

fn differences(sent: &Value, kept: &Value, at: &str, out: &mut Vec<String>) {
    if out.len() >= MAX_ADJUSTMENTS {
        return;
    }
    match (sent, kept) {
        (Value::Object(sent), Value::Object(kept)) => {
            let mut keys: Vec<&String> = sent.keys().chain(kept.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                let path = format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"));
                match (sent.get(key), kept.get(key)) {
                    (Some(a), Some(b)) => differences(a, b, &path, out),
                    _ if out.len() < MAX_ADJUSTMENTS => out.push(path),
                    _ => {}
                }
            }
        }
        (Value::Array(sent), Value::Array(kept)) if sent.len() == kept.len() => {
            for (i, (a, b)) in sent.iter().zip(kept).enumerate() {
                differences(a, b, &format!("{at}/{i}"), out);
            }
        }
        _ if sent != kept => out.push(at.to_owned()),
        _ => {}
    }
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

    /// A header (2.5..3.5 m up) over a panel (from 1 m) whose top runs up
    /// into the header's plane, as on the live garage's front (#1436);
    /// `clear` stops the panel's top 2 cm under the header's foot instead.
    fn header_and_panel(clear: bool) -> Value {
        let panel_y = if clear { -12_600 } else { -10_000 };
        json!({
            "$type": "network.symbios.gen.cuboid",
            "size": [20_000, 10_000, 1_000],
            "solid": true,
            "material": {},
            "transform": { "translation": [0, 30_000, 0] },
            "children": [{
                "$type": "network.symbios.gen.cuboid",
                "size": [6_000, if clear { 14_800 } else { 20_000 }, 1_000],
                "solid": true,
                "material": {},
                "transform": { "translation": [5_000, panel_y, 0] },
            }],
        })
    }

    /// A set that writes a generator names each pair of its primitives
    /// drawing faces in one place, by pointer into the record, with the
    /// area; a clean one names none, and neither does a set that writes no
    /// generator at all.
    #[test]
    fn a_set_names_faces_drawn_twice_in_one_place() {
        let (mut app, _) = app_in(AGENT);

        let set =
            room_set(app.world_mut(), "/generators/shed", header_and_panel(false)).expect("set");
        let named = set["z_fighting"].as_array().expect("a list");
        assert_eq!(named.len(), 1, "{set}");
        assert_eq!(named[0]["a"], "/generators/shed");
        assert_eq!(named[0]["b"], "/generators/shed/children/0");
        let area = named[0]["area_m2"].as_f64().expect("an area");
        assert!(
            (area - 0.6).abs() < 1e-3,
            "front and back, 0.6 m x 0.5 m each: {area}"
        );
        assert!(
            set.get("z_fighting_total").is_none(),
            "all of them are named"
        );

        let set =
            room_set(app.world_mut(), "/generators/shed", header_and_panel(true)).expect("set");
        assert_eq!(set["changed"], true);
        assert_eq!(set["z_fighting"], json!([]), "{set}");

        let set = room_set(
            app.world_mut(),
            "/environment/fog_visibility",
            json!(1_234_500),
        )
        .expect("set");
        assert_eq!(set["z_fighting"], json!([]), "{set}");
    }

    /// A default the record leaves out is no adjustment (#1438): the live
    /// garage's worklights carried `roughness` 0.5, the default, and the
    /// answer said `adjusted` though nothing had been pulled into range.
    #[test]
    fn a_default_the_record_leaves_out_is_not_an_adjustment() {
        let (mut app, _) = app_in(AGENT);
        let lamp = json!({
            "$type": "network.symbios.gen.cuboid",
            "size": [1_200, 500, 12_000],
            "solid": false,
            "material": { "emission_strength": 60_000, "roughness": 5_000 },
        });

        let set = room_set(app.world_mut(), "/generators/lamp", lamp).expect("set");

        assert_eq!(set["changed"], true);
        assert_eq!(set["adjusted"], false, "{set}");
        assert!(set.get("adjusted_at").is_none(), "{set}");
    }

    /// A real adjustment names where: a sphere asked for more subdivisions
    /// than the sanitiser allows is kept at its most, and the answer points
    /// at that one field, not the whole generator.
    #[test]
    fn an_adjustment_names_the_field_the_sanitiser_changed() {
        let (mut app, _) = app_in(AGENT);
        let ball = json!({
            "$type": "network.symbios.gen.sphere",
            "radius": 9_000,
            "resolution": 24,
            "solid": true,
            "material": { "base_color": [4_000, 4_000, 4_200] },
        });

        let set = room_set(app.world_mut(), "/generators/ball", ball).expect("set");

        assert_eq!(set["adjusted"], true, "{set}");
        assert_eq!(set["adjusted_at"], json!(["/generators/ball/resolution"]));
        assert_eq!(set["kept"]["resolution"], 6);
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
