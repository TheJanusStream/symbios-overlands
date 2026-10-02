//! `--rigged FILE` and `--walker-avatar FILE` (#1482): a rigged body drawn
//! from its record, as the game draws it.
//!
//! The tool drew seeded rigged bodies only (`--wear`, `--walker`), so an
//! agent sculpting its own person with `agent avatar set /body/...` had no
//! way to see a step before bringing it live. The file is what `agent avatar
//! get ""` answers, or its `value` alone as `rec.py pull avatar` writes it:
//! `{record, body, worn}` - the avatar record, the body's sculpt (the
//! engine's own record) and the items it wears, each an `rkey` and an
//! attachment record. Read, the three are put back together the way the
//! game holds an avatar it fetched - the sculpt and the worn items as the
//! rigged body's resolution - and sanitised as the game sanitises one
//! ([`parse_rigged`]). Anything that will not read is refused with the key
//! it is at. What reads but is not drawn as the file writes it - a misspelt
//! key the reader skips, a style name or `$type` this build does not know,
//! a value the sanitiser pulls into range - is drawn as the game would draw
//! it and named, one line a place ([`not_as_written`]): the game says
//! nothing about any of it, and the sheet is where it is seen first.
//!
//! The body is then built by the game's own job
//! ([`crate::player::visuals::rigged_build_job`]) at the atlas a settled
//! body is built at, far hair tier included, hung by the game's own root
//! transform and dressed from [`crate::player::attachments::dressed_by`]
//! through the game's own seating ([`crate::player::attachments::placements`]):
//! see [`build_rigged`]. A worn item that cannot be drawn - a socket this
//! build does not know, a part the body does not have, a reference that did
//! not resolve when the file was pulled - is named, never dropped silently.
//!
//! The sheet (`--rigged`) is two rows: four full-body views (front,
//! three-quarter, side, back) at one fixed scale, so a sculpt's stature
//! shows as stature (`headless::rigged_stage`), then three head close-ups
//! (front, three-quarter, side) framed on the rig's own head, from the base
//! of the neck to [`HAIR_ROOM`] skull radii above the head joint
//! ([`head_frame`]) and grown to hold what the head wears, so a tall body
//! and a short one both fill them. The body stands as the game stands a player who is not
//! moving, in the rest pose with its arms hung at its sides
//! ([`standing_pose`]), but without the idle's breath and sway, so two
//! sheets of one record are the same picture and an edit is the only thing
//! that differs between two.
//!
//! `--walker-avatar` puts the same built body at the head of a `--world`
//! walk (see `world.rs`).

use std::sync::Mutex;

use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use bevy_symbios_avatar::{AvatarPose, spawn_avatar};
use serde::de::DeserializeOwned;
use serde_json::Value;
use symbios_avatar::Socket;

use crate::offload::GenResult;
use crate::pds::AvatarRecord;
use crate::pds::avatar::{
    AttachmentRecord, AvatarBody, EngineAvatarRecord, MAX_AVATAR_ATTACHMENTS, ResolvedAttachment,
    ResolvedRig,
};
use crate::player::attachments::{dressed_by, placements};
use crate::player::visuals::{
    rigged_build_job, rigged_root_offset, rigged_root_transform, settled_atlas,
};

use super::headless::PendingWear;

/// A rigged avatar read from a file.
#[derive(Clone, Debug)]
pub(crate) struct RiggedAvatar {
    /// The avatar record, its rigged body resolved from the file's `body`
    /// and `worn` and sanitised as the game sanitises an avatar it fetches.
    pub(crate) record: AvatarRecord,
    /// What the file names that the body will not wear as the file has it,
    /// one sentence each.
    pub(crate) unworn: Vec<String>,
    /// What the file writes that the body is not drawn as, one sentence a
    /// place ([`not_as_written`]).
    pub(crate) unread: Vec<String>,
}

impl RiggedAvatar {
    /// The rigged body's resolution: its sculpt and what it wears.
    fn resolved(&self) -> Option<&ResolvedRig> {
        self.record
            .body
            .rigged_ref()
            .and_then(|rig| rig.resolved.as_ref())
    }
}

/// Read the avatar in the file at `path` (see [`parse_rigged`]).
pub(crate) fn read_rigged(path: &str) -> Result<RiggedAvatar, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read it: {e}"))?;
    parse_rigged(&text)
}

/// The avatar in `text`: `{record, body, worn}`, bare or as the `value` of
/// an `agent avatar get ""` answer. A refusal names the JSON pointer of what
/// is wrong, in the file as written, and says what it should have been.
pub(crate) fn parse_rigged(text: &str) -> Result<RiggedAvatar, String> {
    let document: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    let (avatar, at) = unwrap_answer(document)?;
    let Value::Object(mut parts) = avatar else {
        return Err(format!(
            "{}: {}, not an object - an avatar is {{record, body, worn}}",
            place(at),
            kind_of(&avatar)
        ));
    };

    let record_json = parts
        .remove("record")
        .ok_or_else(|| format!("{at}/record: missing - the avatar record"))?;
    let mut record: AvatarRecord =
        read_part(&record_json, &format!("{at}/record"), "an avatar record")?;
    match &record.body {
        AvatarBody::Rigged(_) => {}
        AvatarBody::Generator(_) => {
            return Err(format!(
                "{at}/record/body: a generator body - a vehicle's - not a rigged person; \
                 draw its tree with --generator, from {at}/record/body/visuals"
            ));
        }
        AvatarBody::Unknown => {
            return Err(format!(
                "{at}/record/body: a kind of body this build does not know"
            ));
        }
        AvatarBody::Absent => {
            return Err(format!(
                "{at}/record/body: missing - the record names no body"
            ));
        }
    }

    let body_json = parts
        .remove("body")
        .ok_or_else(|| format!("{at}/body: missing - the body's sculpt, the engine's record"))?;
    if body_json.is_null() {
        return Err(format!(
            "{at}/body: null - the file carries no sculpt (the record's body did not resolve \
             when it was pulled)"
        ));
    }
    let body: EngineAvatarRecord = read_part(
        &body_json,
        &format!("{at}/body"),
        "a sculpt (the engine's avatar record)",
    )?;

    let worn_json = match parts.remove("worn") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items,
        Some(other) => {
            return Err(format!(
                "{at}/worn: {}, not a list - worn is a list of {{rkey, item}}",
                kind_of(&other)
            ));
        }
    };
    let worn = worn_json
        .iter()
        .enumerate()
        .map(|(i, entry)| read_worn(entry, &format!("{at}/worn/{i}")))
        .collect::<Result<Vec<_>, _>>()?;
    let in_file: Vec<String> = worn.iter().map(|w| w.rkey.clone()).collect();

    if let Some(rig) = record.body.rigged_mut() {
        rig.resolved = Some(ResolvedRig {
            body,
            attachments: worn,
        });
    }
    // The file as written, and as this build reads it and would write it
    // back: before the sanitiser, then after.
    let written = serde_json::json!({
        "record": record_json,
        "body": body_json,
        "worn": worn_json,
    });
    let read = avatar_json(&record);
    // As a fetched avatar is: clamped, bounded and deduplicated.
    record.sanitize();
    let kept = avatar_json(&record);
    let mut unread = not_as_written(&written, &read, &kept, &record, at);
    stand_ins(&record, at, &mut unread);
    let unworn = unworn(&record, &in_file, at);
    Ok(RiggedAvatar {
        record,
        unworn,
        unread,
    })
}

/// A worn item holding a value an open union reads as its stand-in (a blob
/// shape, a prop mapping this build does not know) cannot be written back,
/// so it is `null` on both sides of every comparison and nothing in it was
/// named - the stand-in, a misspelt key or a clamp beside it (the session
/// 879 review; #1487 is the `--generator` twin). It is named here, unless a
/// note already names that item.
fn stand_ins(record: &AvatarRecord, at: &str, notes: &mut Vec<String>) {
    let Some(resolved) = record
        .body
        .rigged_ref()
        .and_then(|rig| rig.resolved.as_ref())
    else {
        return;
    };
    for (i, worn) in resolved.attachments.iter().enumerate() {
        let Err(e) = serde_json::to_value(&worn.record) else {
            continue;
        };
        let item = format!("{at}/worn/{i}/");
        if notes.iter().any(|note| note.starts_with(&item)) {
            continue;
        }
        notes.push(format!(
            "{at}/worn/{i}/item: worn {} holds a value this build reads as a stand-in and \
             cannot write back ({e}) - drawn as the stand-in, as the game draws it, and \
             nothing else in it can be compared; `agent avatar set` refuses to save it (a \
             misspelt kind or $type?)",
            worn.rkey
        ));
    }
}

/// `record`'s avatar in the three parts `agent avatar get ""` writes it in,
/// each in its wire form: `{record, body, worn}`. A worn item that cannot be
/// written back - one with a part of a kind this build does not know, which
/// the wire refuses to write - is `null` in its `item`.
pub(crate) fn avatar_json(record: &AvatarRecord) -> Value {
    let resolved = record
        .body
        .rigged_ref()
        .and_then(|rig| rig.resolved.as_ref());
    serde_json::json!({
        "record": serde_json::to_value(record).unwrap_or_default(),
        "body": resolved
            .and_then(|rig| serde_json::to_value(&rig.body).ok())
            .unwrap_or_default(),
        "worn": resolved.map_or_else(Vec::new, |rig| {
            rig.attachments
                .iter()
                .map(|w| {
                    serde_json::json!({
                        "rkey": w.rkey,
                        "item": serde_json::to_value(&w.record).unwrap_or_default(),
                    })
                })
                .collect()
        }),
    })
}

/// The avatar a file holds and the pointer it is at: the whole file, as
/// `rec.py pull avatar` writes it, or the `result.value` of an `agent avatar
/// get ""` answer.
pub(super) fn unwrap_answer(document: Value) -> Result<(Value, &'static str), String> {
    let Value::Object(mut top) = document else {
        return Err(format!(
            "the top level: {}, not an object - an avatar is {{record, body, worn}}, or the \
             answer `agent avatar get \"\"` prints",
            kind_of(&document)
        ));
    };
    if !top.contains_key("ok") && !top.contains_key("result") {
        return Ok((Value::Object(top), ""));
    }
    if top.get("ok").and_then(Value::as_bool) == Some(false) {
        let why = top
            .get("error")
            .map_or_else(|| "it gives no error".to_owned(), Value::to_string);
        return Err(format!(
            "/ok: false - the file is a refused command's answer ({why}), not an avatar"
        ));
    }
    let Some(Value::Object(mut result)) = top.remove("result") else {
        return Err(
            "/result: missing or not an object - an `agent avatar get` answer carries the \
             avatar in result.value"
                .to_owned(),
        );
    };
    if let Some(pointer) = result
        .get("pointer")
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
    {
        return Err(format!(
            "/result/pointer: {pointer:?} - the file answers `agent avatar get {pointer}`, one \
             part of the avatar; draw the whole of it, `agent avatar get \"\"`"
        ));
    }
    let value = result
        .remove("value")
        .ok_or("/result/value: missing - an `agent avatar get` answer carries the avatar there")?;
    Ok((value, "/result/value"))
}

/// One worn item, `{rkey, item}`, at pointer `at`.
fn read_worn(entry: &Value, at: &str) -> Result<ResolvedAttachment, String> {
    let Value::Object(members) = entry else {
        return Err(format!(
            "{at}: {}, not an object - a worn item is {{rkey, item}}",
            kind_of(entry)
        ));
    };
    let rkey = members
        .get("rkey")
        .ok_or_else(|| format!("{at}/rkey: missing - the key of the record the item is kept in"))?
        .as_str()
        .ok_or_else(|| format!("{at}/rkey: not a string"))?
        .to_owned();
    let item = members
        .get("item")
        .ok_or_else(|| format!("{at}/item: missing - the attachment record"))?;
    let record =
        read_part::<AttachmentRecord>(item, &format!("{at}/item"), "an attachment record")?;
    Ok(ResolvedAttachment { rkey, record })
}

/// `value` read as a `T`, or a refusal naming where in it the read failed
/// (`at` is the pointer of `value` itself).
pub(super) fn read_part<T: DeserializeOwned>(
    value: &Value,
    at: &str,
    what: &str,
) -> Result<T, String> {
    serde_json::from_value::<T>(value.clone()).map_err(|e| {
        let inner = failing_pointer::<T>(value).unwrap_or_default();
        format!(
            "{}: {e} - that is not {what}",
            place(&format!("{at}{inner}"))
        )
    })
}

/// A pointer as a refusal names it: the empty pointer is the whole file.
pub(super) fn place(pointer: &str) -> String {
    if pointer.is_empty() {
        "the top level".to_owned()
    } else {
        pointer.to_owned()
    }
}

/// What kind of JSON value `value` is, for a refusal.
pub(super) fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

/// Where serde gave up reading `document` as a `T`, as a JSON pointer inside
/// it. Read from a `Value`, serde's error names a field but never where it
/// is; read from text it carries a line. So the document is written one
/// member to a line, read again, and the failing line walked back to its
/// path. The agent daemon's `edit::json` does the same for `avatar set`
/// (#1457); this is the render tool's own copy, since that module is the
/// daemon's alone.
///
/// One step further than the daemon's: a `$type`-tagged part - a sculpt's
/// `archetype`, or its `hair`, read through a value of its own - is read
/// whole before its fields are, so its error's line is the part's last and
/// names no field. Inside it, the one value the error describes is found by
/// what it says it found ([`described_value`]), and the one object a missing
/// field was wanted in by giving each the field ([`missing_field_place`]).
fn failing_pointer<T: DeserializeOwned>(document: &Value) -> Option<String> {
    let text = serde_json::to_string_pretty(document).ok()?;
    let error = serde_json::from_str::<T>(&text).err()?;
    let at = pointer_of_line(&text, error.line())?;
    let message = error.to_string();
    let inner = document
        .pointer(&at)
        .and_then(|part| described_value(part, &message))
        .or_else(|| {
            let field = message.strip_prefix("missing field `")?.split('`').next()?;
            missing_field_place::<T>(document, &at, field)
        })
        .unwrap_or_default();
    Some(format!("{at}{inner}"))
}

/// The pointer, inside the part of `document` at `at`, of the one object a
/// `field` serde missed belongs in: each object under the part that lacks it
/// is given it (as `null`) and the document read again, and the object
/// whose gift changes the error is the one. `None` unless exactly one does.
/// An object that is not the one ignores the stranger - an unknown key is
/// skipped - so the error stays as it was.
fn missing_field_place<T: DeserializeOwned>(
    document: &Value,
    at: &str,
    field: &str,
) -> Option<String> {
    let before = serde_json::from_value::<T>(document.clone())
        .err()?
        .to_string();
    let mut lacking = Vec::new();
    let mut open = vec![(String::new(), document.pointer(at)?)];
    while let Some((here, value)) = open.pop() {
        match value {
            Value::Object(members) => {
                if !members.contains_key(field) {
                    lacking.push(here.clone());
                }
                for (key, member) in members {
                    let key = key.replace('~', "~0").replace('/', "~1");
                    open.push((format!("{here}/{key}"), member));
                }
            }
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    open.push((format!("{here}/{i}"), item));
                }
            }
            _ => {}
        }
    }
    let mut fits = lacking.into_iter().filter(|here| {
        let mut probe = document.clone();
        let Some(Value::Object(members)) = probe.pointer_mut(&format!("{at}{here}")) else {
            return false;
        };
        members.insert(field.to_owned(), Value::Null);
        serde_json::from_value::<T>(probe)
            .err()
            .map(|e| e.to_string())
            != Some(before.clone())
    });
    let one = fits.next()?;
    fits.next().is_none().then_some(one)
}

/// The pointer, inside `part`, of the one value serde's `error` says it
/// found where it wanted another ("invalid type: string \"tall\", expected
/// i64"), when exactly one value under `part` fits that description; `None`
/// when none does or several do, so a guess is never passed off as a place.
fn described_value(part: &Value, error: &str) -> Option<String> {
    let found = error
        .strip_prefix("invalid type: ")
        .or_else(|| error.strip_prefix("invalid value: "))?;
    let mut fits = Vec::new();
    let mut open = vec![(String::new(), part)];
    while let Some((at, value)) = open.pop() {
        if is_described(value, found) {
            fits.push(at.clone());
        }
        match value {
            Value::Object(members) => {
                for (key, member) in members {
                    let key = key.replace('~', "~0").replace('/', "~1");
                    open.push((format!("{at}/{key}"), member));
                }
            }
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    open.push((format!("{at}/{i}"), item));
                }
            }
            _ => {}
        }
    }
    (fits.len() == 1).then(|| fits.remove(0))
}

/// Whether `value` is what serde's `found` - the part of an "invalid type"
/// error after its colon - describes, in serde's own words for it.
fn is_described(value: &Value, found: &str) -> bool {
    match value {
        Value::String(s) => found.starts_with(&format!("string {s:?}")),
        Value::Number(n) => {
            let integer = n
                .as_i64()
                .map(|i| i.to_string())
                .or_else(|| n.as_u64().map(|u| u.to_string()));
            match integer {
                Some(i) => found.starts_with(&format!("integer `{i}`")),
                None => n.as_f64().is_some_and(|f| {
                    found.starts_with(&format!("floating point `{f}`"))
                        || found.starts_with(&format!("floating point `{f}.0`"))
                }),
            }
        }
        Value::Bool(b) => found.starts_with(&format!("boolean `{b}`")),
        Value::Null => found.starts_with("null") || found.starts_with("unit value"),
        Value::Array(_) => found.starts_with("sequence"),
        Value::Object(_) => found.starts_with("map"),
    }
}

/// The JSON pointer of what line `target` (from 1) of pretty-printed JSON
/// holds: the member or element on it, or - on a closing bracket, where
/// serde reports a missing field - the object or array that line closes.
fn pointer_of_line(text: &str, target: usize) -> Option<String> {
    // Each open container: its path, and for a list the next index.
    let mut open: Vec<(Vec<String>, Option<usize>)> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        let here: Vec<String> = if matches!(line, "}" | "}," | "]" | "],") {
            let (path, _) = open.pop()?;
            path
        } else {
            match open.last_mut() {
                None => Vec::new(),
                Some((path, Some(index))) => {
                    let mut here = path.clone();
                    here.push(index.to_string());
                    *index += 1;
                    here
                }
                Some((path, None)) => {
                    let mut here = path.clone();
                    here.push(member_key(line)?);
                    here
                }
            }
        };
        if number + 1 == target {
            return Some(here.iter().map(|s| format!("/{s}")).collect());
        }
        if line.ends_with('{') {
            open.push((here, None));
        } else if line.ends_with('[') {
            open.push((here, Some(0)));
        }
    }
    None
}

/// The key of a pretty-printed member line (`"key": ...`), escaped for a
/// JSON pointer.
fn member_key(line: &str) -> Option<String> {
    let mut escaped = false;
    let end = line
        .char_indices()
        .skip(1)
        .find(|&(_, c)| {
            let closes = c == '"' && !escaped;
            escaped = c == '\\' && !escaped;
            closes
        })?
        .0;
    let key: String = serde_json::from_str(&line[..=end]).ok()?;
    Some(key.replace('~', "~0").replace('/', "~1"))
}

/// What the file names that the body will not wear as the file has it, one
/// sentence each: an item past the most a body may wear, a reference the
/// record makes that no worn item answers (it did not resolve when the file
/// was pulled), and a worn item the record does not name. `in_file` is the
/// worn items' keys as the file lists them, before the sanitiser ran.
fn unworn(record: &AvatarRecord, in_file: &[String], at: &str) -> Vec<String> {
    let Some(rig) = record.body.rigged_ref() else {
        return Vec::new();
    };
    let kept: Vec<&str> = rig
        .resolved
        .as_ref()
        .map(|resolved| {
            resolved
                .attachments
                .iter()
                .map(|w| w.rkey.as_str())
                .collect()
        })
        .unwrap_or_default();
    let mut out = Vec::new();
    for (i, rkey) in in_file.iter().enumerate().skip(kept.len()) {
        out.push(format!(
            "worn item {rkey:?} ({at}/worn/{i}): past the {MAX_AVATAR_ATTACHMENTS} items a body \
             may wear - not drawn"
        ));
    }
    for rkey in &rig.attachments {
        if !kept.contains(&rkey.as_str()) {
            out.push(format!(
                "attachment {rkey:?} ({at}/record/body/attachments): the record names it, but \
                 the file has no worn item for it - it did not resolve when the file was pulled; \
                 not drawn"
            ));
        }
    }
    for rkey in &kept {
        if !rig.attachments.iter().any(|named| named == rkey) {
            out.push(format!(
                "worn item {rkey:?}: not named in {at}/record/body/attachments - drawn, as the \
                 game draws what resolved, but the saved record would not wear it"
            ));
        }
    }
    out
}

/// At most this many places are named as not drawn as written; past it, a
/// count.
const MAX_UNREAD: usize = 24;

/// What the file writes that the body is not drawn as, one sentence a place,
/// each at its pointer in the file (`at` is where the avatar is in it).
/// `written` is the file's `{record, body, worn}` as written; `read` is what
/// this build read of it, written back out before the sanitiser ran, and
/// `kept` the same after it ([`avatar_json`]); `record` is the sanitised
/// avatar.
///
/// Three kinds, because a reader that never fails - the engine's records
/// degrade rather than refuse, as the game needs them to - has three ways to
/// draw something else than the file says:
///
/// * what did not read as written (`written` against `read`): a key the
///   reader skipped - a misspelt one, whose real key then takes its default
///   and is named too - and a value read as another
///   ([`unread_on_the_way_in`]);
/// * what the sanitiser changed (`read` against `kept`): a value pulled into
///   range, a repeated reference dropped - what `agent avatar set` answers as
///   `adjusted_at`;
/// * what reads and writes back unchanged but draws nothing: a field kept
///   for a newer build, a body plan, hair style or garment surface this build
///   does not know - and what cannot be written back at all, a locomotion or
///   a worn part of a kind it does not know ([`set_aside`]).
pub(crate) fn not_as_written(
    written: &Value,
    read: &Value,
    kept: &Value,
    record: &AvatarRecord,
    at: &str,
) -> Vec<String> {
    let mut notes = unread_on_the_way_in(written, read, at);
    let mut changed = Vec::new();
    json_differences(read, kept, "", &mut changed);
    for difference in changed {
        let (here, before, after) = (
            difference.pointer(),
            read.pointer(difference.pointer()),
            kept.pointer(difference.pointer()),
        );
        // A worn item past the most a body wears: `unworn` names it.
        if is_worn_item(here) {
            continue;
        }
        notes.push(match (before, after) {
            (Some(before), Some(after)) => format!(
                "{at}{here}: {} is out of range - drawn as {}, as the game draws it",
                shown(before),
                shown(after)
            ),
            (Some(before), None) => format!(
                "{at}{here}: {} is dropped, as the game drops it",
                shown(before)
            ),
            (None, Some(after)) => {
                format!("{at}{here}: set to {}, as the game sets it", shown(after))
            }
            (None, None) => continue,
        });
    }
    set_aside(record, written, at, &mut notes);
    if notes.len() > MAX_UNREAD {
        let more = notes.len() - (MAX_UNREAD - 1);
        notes.truncate(MAX_UNREAD - 1);
        notes.push(format!("... and {more} more"));
    }
    notes
}

/// Whether `pointer` is a whole worn item, `/worn/<i>`.
fn is_worn_item(pointer: &str) -> bool {
    pointer
        .strip_prefix("/worn/")
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

/// What of `written` did not read as written, against `read`, the same
/// avatar read and written back: see [`not_as_written`].
fn unread_on_the_way_in(written: &Value, read: &Value, at: &str) -> Vec<String> {
    let mut differences = Vec::new();
    json_differences(written, read, "", &mut differences);
    // A part that cannot be written back - a record whose locomotion, or a
    // worn item one of whose parts, is of a kind this build does not know,
    // which the wire refuses to write - has nothing to compare against;
    // `set_aside` names what it holds that this build does not know.
    let mut unwritten: Vec<String> = (0..written["worn"].as_array().map_or(0, Vec::len))
        .map(|i| format!("/worn/{i}/item"))
        .collect();
    unwritten.push("/record".to_owned());
    unwritten.retain(|part| read.pointer(part).is_none_or(Value::is_null));
    let mut notes = Vec::new();
    for difference in &differences {
        let here = difference.pointer();
        if unwritten.iter().any(|part| {
            here.strip_prefix(part.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        }) {
            continue;
        }
        let (was, now) = (written.pointer(here), read.pointer(here));
        match (difference, was, now) {
            (Difference::Changed(_), Some(was), Some(now)) => {
                if !same_number(was, now) {
                    notes.push(format!(
                        "{at}{here}: {} does not read as written - drawn as {}",
                        shown(was),
                        shown(now)
                    ));
                }
            }
            (Difference::Dropped(_), Some(was), _) => {
                if is_ignored(written, here) {
                    notes.push(format!(
                        "{at}{here}: not a key this build reads (misspelt?) - {} is ignored",
                        shown(was)
                    ));
                }
            }
            (Difference::Added(_), _, Some(now)) => {
                notes.push(format!(
                    "{at}{here}: not in the file - drawn as {}",
                    shown(now)
                ));
            }
            _ => {}
        }
    }
    notes
}

/// One place two JSON values differ, by its pointer.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Difference {
    /// A value the first has and the second does not.
    Dropped(String),
    /// A value the second has and the first does not.
    Added(String),
    /// A value both have, different.
    Changed(String),
}

impl Difference {
    pub(super) fn pointer(&self) -> &str {
        match self {
            Self::Dropped(at) | Self::Added(at) | Self::Changed(at) => at,
        }
    }
}

/// Every place `first` and `second` differ, under `at`: into objects member
/// by member and into lists item by item, so a list that grew or shrank is
/// its extra items, not the whole list.
pub(super) fn json_differences(first: &Value, second: &Value, at: &str, out: &mut Vec<Difference>) {
    match (first, second) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                let here = format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"));
                match (a.get(key), b.get(key)) {
                    (Some(x), Some(y)) => json_differences(x, y, &here, out),
                    (Some(_), None) => out.push(Difference::Dropped(here)),
                    (None, Some(_)) => out.push(Difference::Added(here)),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                json_differences(x, y, &format!("{at}/{i}"), out);
            }
            out.extend((b.len()..a.len()).map(|i| Difference::Dropped(format!("{at}/{i}"))));
            out.extend((a.len()..b.len()).map(|i| Difference::Added(format!("{at}/{i}"))));
        }
        _ if first != second => out.push(Difference::Changed(at.to_owned())),
        _ => {}
    }
}

/// Whether two values are one number written two ways - a seed the record
/// writes as a string, read from a bare number; `1.0` read as `1` - which is
/// no difference in what is drawn.
pub(super) fn same_number(a: &Value, b: &Value) -> bool {
    let text = |v: &Value| match v {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) => Some(s.clone()),
        _ => None,
    };
    let (Some(x), Some(y)) = (text(a), text(b)) else {
        return false;
    };
    let numeric = |s: &str| s.parse::<f64>().ok();
    (a.is_number() || b.is_number())
        && (x == y || matches!((numeric(&x), numeric(&y)), (Some(p), Some(q)) if p == q))
}

/// Whether the key at `pointer` in `written` - one this build did not write
/// back - is one its reader skips, rather than a field written at the value
/// the record leaves out. Asked of the reader itself: the key's value is
/// replaced by one no field could take, and the part it is in read again.
/// A field would refuse it or read otherwise; a skipped key changes nothing.
fn is_ignored(written: &Value, pointer: &str) -> bool {
    let segments: Vec<&str> = pointer.split('/').skip(1).collect();
    // The part the key is in, and how that part is read: the whole avatar
    // record, the whole sculpt, one worn item's record. A worn item's own
    // `{rkey, item}` reads nothing else.
    type Reread = fn(&Value) -> Option<Value>;
    let (part, reread): (String, Reread) = match segments.as_slice() {
        ["record", _, ..] => ("/record".to_owned(), reread::<AvatarRecord>),
        ["body", _, ..] => ("/body".to_owned(), reread::<EngineAvatarRecord>),
        ["worn", i, "item", _, ..] => (format!("/worn/{i}/item"), reread::<AttachmentRecord>),
        ["worn", _, _] => return true,
        _ => return false,
    };
    let Some(original) = written.pointer(&part) else {
        return false;
    };
    let mut probe = original.clone();
    let Some(slot) = probe.pointer_mut(&pointer[part.len()..]) else {
        return false;
    };
    *slot = Value::String("\u{1}not a value any field takes".to_owned());
    match (reread(original), reread(&probe)) {
        (Some(before), Some(after)) => before == after,
        _ => false,
    }
}

/// `value` read as a `T` and written back out, or `None` if either fails.
fn reread<T: DeserializeOwned + serde::Serialize>(value: &Value) -> Option<Value> {
    serde_json::from_value::<T>(value.clone())
        .ok()
        .and_then(|read| serde_json::to_value(read).ok())
}

/// What reads and writes back as the file has it but is not drawn: see
/// [`not_as_written`]. `written` is the file's `{record, body, worn}`, for
/// the `$type` a worn part came in with.
fn set_aside(record: &AvatarRecord, written: &Value, at: &str, notes: &mut Vec<String>) {
    let Some(resolved) = record
        .body
        .rigged_ref()
        .and_then(|rig| rig.resolved.as_ref())
    else {
        return;
    };
    if matches!(
        record.locomotion,
        crate::pds::avatar::LocomotionConfig::Unknown
    ) {
        let kind = written
            .pointer("/record/locomotion/$type")
            .map_or_else(|| "no $type".to_owned(), shown);
        notes.push(format!(
            "{at}/record/locomotion/$type: {kind} is not a locomotion this build knows - the \
             game hangs the body under a stand-in chassis, and so does this sheet"
        ));
    }
    let body = &resolved.body;
    for (key, value) in body.extra.iter() {
        // the collection's own NSID, which a sculpt fetched from the PDS
        // carries (`engine_record_wire`): every real pulled avatar was told
        // it was misspelt (the session 879 review)
        if key == "$type" && value.as_str() == Some(crate::pds::WARDROBE_COLLECTION) {
            continue;
        }
        notes.push(format!(
            "{at}/body/{}: not a key this build reads (misspelt?) - kept for a newer \
             build, draws nothing",
            key.replace('~', "~0").replace('/', "~1")
        ));
    }
    if let symbios_avatar::Archetype::Unknown { type_name, .. } = &body.archetype {
        notes.push(format!(
            "{at}/body/archetype/$type: {type_name:?} is not a body plan this build knows - \
             the game stands its default humanoid in its place, and so does this sheet"
        ));
    }
    let hair = &body.hair.unrecognised;
    for (region, style) in [
        ("scalp", &hair.scalp),
        ("brows", &hair.brows),
        ("moustache", &hair.moustache),
        ("chin", &hair.chin),
        ("flanks", &hair.flanks),
    ] {
        if let Some(style) = style {
            let name = style.get("name").map_or_else(|| "none".to_owned(), shown);
            notes.push(format!(
                "{at}/body/hair/{region}/style/name: {name} is not a {region} style this build \
                 knows - that region grows no hair"
            ));
        }
    }
    for (garment, params) in [
        ("top", &body.outfit.top),
        ("trousers", &body.outfit.trousers),
    ] {
        if let Some(texture) = params.texture.as_ref().filter(|t| !t.surface.is_known()) {
            notes.push(format!(
                "{at}/body/outfit/{garment}/texture/surface: {:?} is not a surface this build \
                 knows, or its fields do not read - the {garment} is plain cloth",
                texture.surface.name()
            ));
        }
    }
    for (i, worn) in resolved.attachments.iter().enumerate() {
        let mut parts = Vec::new();
        unknown_parts(
            &worn.record.item,
            format!("/worn/{i}/item/item"),
            &mut parts,
        );
        for here in parts {
            let kind = written
                .pointer(&format!("{here}/$type"))
                .map_or_else(|| "no $type".to_owned(), shown);
            notes.push(format!(
                "{at}{here}: worn {}'s part of kind {kind} is not a kind this build knows - \
                 it is not drawn, nor anything under it",
                worn.rkey
            ));
        }
    }
}

/// The pointer, under `at` (the pointer of `generator` itself), of each part
/// of the tree of a kind this build does not know - the spawn path draws
/// none of them, nor anything under them, so their children are not looked
/// in.
fn unknown_parts(generator: &crate::pds::Generator, at: String, out: &mut Vec<String>) {
    if matches!(generator.kind, crate::pds::GeneratorKind::Unknown) {
        out.push(at);
        return;
    }
    for (i, child) in generator.children.iter().enumerate() {
        unknown_parts(child, format!("{at}/children/{i}"), out);
    }
}

/// A value for a note: its JSON, cut short past sixty characters.
pub(super) fn shown(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() > 60 {
        format!("{}...", text.chars().take(57).collect::<String>())
    } else {
        text
    }
}

/// A rigged body built as the game builds it, ready to stand.
pub(crate) struct RiggedBuilt {
    /// The built body.
    pub(crate) avatar: symbios_avatar::Avatar,
    /// How far above the feet a chassis's centre is: half the collider the
    /// record's locomotion gives it (the game's root drop).
    pub(crate) offset: f32,
    /// What it wears, as the game dresses it.
    pub(crate) worn: Vec<ResolvedAttachment>,
    /// One line per worn item: the socket it hangs at, or why it does not.
    pub(crate) seating: Vec<String>,
    /// Where the head is, in the body's own space: what the close-ups frame.
    pub(crate) head: Option<HeadFrame>,
    /// The box the body fills in its [`standing_pose`], in its own space.
    /// Bevy bounds a skinned mesh by its bind pose - the A-pose, arms out -
    /// so the drawn meshes' boxes would say a body is half again as wide as
    /// it stands; this is the posed body's own.
    pub(crate) standing_box: (Vec3, Vec3),
    /// The seed the walker's idle and blinks run on: the sculpt's own.
    pub(crate) idle_seed: u64,
}

/// Build `avatar`'s body as the game builds a settled one - its own job, at
/// the full atlas, with the far hair tier - and dress it as the game dresses
/// it. `Err` is the engine's one failure: a body it cannot mesh.
pub(crate) fn build_rigged(avatar: &RiggedAvatar) -> Result<RiggedBuilt, String> {
    let resolved = avatar
        .resolved()
        .ok_or("the avatar has no resolved rigged body")?;
    let built = match rigged_build_job(&resolved.body, settled_atlas()).run() {
        GenResult::Avatar(Some(body)) => *body,
        GenResult::Avatar(None) => {
            return Err(
                "the engine cannot build this body at these proportions (its one failure: \
                 limbs overlapping at a joint), so the game would stand no body here either - \
                 move the last sculpt edit back"
                    .to_owned(),
            );
        }
        _ => return Err("the avatar build returned another job's result".to_owned()),
    };
    let worn = dressed_by(&avatar.record).unwrap_or_default().to_vec();
    let seating = seating(&built, &worn);
    Ok(RiggedBuilt {
        head: head_frame(&built.rig),
        standing_box: standing_box(&built),
        offset: rigged_root_offset(&avatar.record),
        seating,
        worn,
        idle_seed: resolved.body.seed as u64,
        avatar: built,
    })
}

/// The box `avatar` fills in its [`standing_pose`], in its own space: every
/// drawn mesh posed as the engine poses it (the far hair tier is not drawn
/// beside the near one, so it is not here either).
fn standing_box(avatar: &symbios_avatar::Avatar) -> (Vec3, Vec3) {
    let pose = standing_pose(&avatar.rig);
    avatar
        .posed(&pose, 0.0)
        .iter()
        .flat_map(|drawn| drawn.mesh.positions.iter().copied())
        .fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), p| (min.min(p), max.max(p)),
        )
}

/// Where a `--rigged` sheet's cameras look, in the studio's world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RiggedFraming {
    /// The standing body's box, from the engine's posed meshes. The sheet
    /// adds the worn items' boxes to it once they are on
    /// (`headless::frame_rigged`) and stands the whole on a fixed stage.
    pub(crate) body: (Vec3, Vec3),
    /// The head: the bottom row's subject, before the sheet grows it to hold
    /// what the head wears.
    pub(crate) head: Option<HeadFrame>,
}

impl RiggedBuilt {
    /// Where the sheet's cameras look once [`spawn_standing`] has stood this
    /// body in the studio.
    pub(crate) fn studio_framing(&self) -> RiggedFraming {
        let root = studio_root(self.offset);
        let (min, max) = self.standing_box;
        let corners = (0..8).map(|i| {
            root.transform_point(Vec3::new(
                if i & 1 == 0 { min.x } else { max.x },
                if i & 2 == 0 { min.y } else { max.y },
                if i & 4 == 0 { min.z } else { max.z },
            ))
        });
        let body = corners.fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), p| (min.min(p), max.max(p)),
        );
        RiggedFraming {
            body,
            head: self.head.map(|head| head.placed(&root)),
        }
    }
}

/// Where each worn item hangs on `avatar`, or why it does not: the game's
/// own seating, [`placements`], which skips an item whose socket this build
/// does not know and one whose socket the body has no part for - and the
/// spawn path's, which draws no part of a kind this build does not know,
/// nor anything under it.
fn seating(avatar: &symbios_avatar::Avatar, worn: &[ResolvedAttachment]) -> Vec<String> {
    let seated = placements(avatar, worn);
    worn.iter()
        .map(|item| {
            let rkey = &item.rkey;
            let socket = &item.record.socket;
            let mut unknown = Vec::new();
            unknown_parts(&item.record.item, String::new(), &mut unknown);
            let lacking = match unknown.len() {
                0 => String::new(),
                n => format!(", less {n} part(s) of a kind this build does not know"),
            };
            match seated.iter().find(|(_, _, a)| std::ptr::eq(*a, item)) {
                _ if unknown.first().is_some_and(String::is_empty) => format!(
                    "worn {rkey}: NOT drawn - its root part is of a kind this build does not know"
                ),
                Some((joint, _, _)) if item.record.offset.is_identity() => format!(
                    "worn {rkey}: at {socket} (joint {joint}), seated where a fresh wear \
                     lands{lacking}"
                ),
                Some((joint, _, _)) => {
                    format!("worn {rkey}: at {socket} (joint {joint}), at its own offset{lacking}")
                }
                None if item.record.socket().is_none() => {
                    format!("worn {rkey}: NOT drawn - {socket:?} is not a socket this build knows")
                }
                None => format!("worn {rkey}: NOT drawn - this body has no {socket}"),
            }
        })
        .collect()
}

/// A head close-up's subject: a sphere around the head.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HeadFrame {
    pub(crate) centre: Vec3,
    pub(crate) radius: f32,
}

impl HeadFrame {
    /// This frame carried through `to_world`.
    pub(crate) fn placed(self, to_world: &Transform) -> Self {
        Self {
            centre: to_world.transform_point(self.centre),
            radius: self.radius * to_world.scale.max_element(),
        }
    }

    /// This frame grown to hold the box `(min, max)` as well - what the head
    /// wears, a hat taller than [`HAIR_ROOM`] allows for. The centre keeps
    /// its footing and rises to the middle of the two's height, and the
    /// sphere holds this one's and every corner of the box.
    pub(crate) fn grown_by(self, (min, max): (Vec3, Vec3)) -> Self {
        let bottom = (self.centre.y - self.radius).min(min.y);
        let top = (self.centre.y + self.radius).max(max.y);
        let centre = Vec3::new(self.centre.x, (bottom + top) * 0.5, self.centre.z);
        let corners = (0..8).map(|i| {
            Vec3::new(
                if i & 1 == 0 { min.x } else { max.x },
                if i & 2 == 0 { min.y } else { max.y },
                if i & 4 == 0 { min.z } else { max.z },
            )
        });
        let radius = corners
            .map(|corner| corner.distance(centre))
            .fold(self.radius + self.centre.distance(centre), f32::max);
        Self { centre, radius }
    }
}

/// How far above the skull's centre the head close-up reaches, in skull
/// radii: the skull itself (its skin sits about one radius up, measured on
/// seeded bodies) and room above it for the hair, which reached 1.75 radii on
/// the tallest of them.
const HAIR_ROOM: f32 = 1.8;

/// How much larger than half the neck-to-hair span the close-up's sphere is:
/// air around the head, so a jaw ahead of the neck's line and an ear beside
/// it are not cropped.
const HEAD_MARGIN: f32 = 1.1;

/// The head, from the rig's own landmarks: the sphere over the span from the
/// base of the neck ([`Socket::Neck`]'s anchor) to [`HAIR_ROOM`] skull radii
/// above the skull's centre ([`Socket::Crown`]'s anchor is the head joint,
/// carrying the skull's radius). A tall body and a short one both fill the
/// tile, because the span is theirs. `None` for a body with no head.
pub(crate) fn head_frame(rig: &symbios_avatar::Rig) -> Option<HeadFrame> {
    let skull = Socket::Crown.anchor(rig)?;
    let neck = Socket::Neck.anchor(rig)?.position;
    let top = skull.position + Vec3::Y * skull.radius * HAIR_ROOM;
    Some(HeadFrame {
        centre: (top + neck) * 0.5,
        radius: top.distance(neck) * 0.5 * HEAD_MARGIN,
    })
}

/// The pose a body the game shows standing still is built on: the rest pose
/// with the arms hung at its sides, the engine's own carriage
/// ([`symbios_avatar::anim::gait::hang_arms`]) and the first thing the
/// game's idle does every frame. The idle's breath, sway, weight shift and
/// fidgets move over it with time, and are left out so two sheets of one
/// record are the same picture. (The bind pose is the A-pose a body is
/// modelled in, arms 50 degrees out; nothing standing in the game shows it.)
pub(crate) fn standing_pose(rig: &symbios_avatar::Rig) -> symbios_avatar::Pose {
    let mut pose = symbios_avatar::Pose::rest(rig);
    symbios_avatar::anim::gait::hang_arms(rig, &mut pose);
    pose
}

/// Where the studio stands a body's chassis: its centre `offset` above the
/// floor, as a capsule's is, so the game's root drop puts the feet on it.
pub(crate) fn studio_chassis(offset: f32) -> Transform {
    Transform::from_xyz(0.0, offset, 0.0)
}

/// The body's root in the studio's world: the chassis, then the game's
/// root transform under it.
pub(crate) fn studio_root(offset: f32) -> Transform {
    studio_chassis(offset).mul_transform(rigged_root_transform(offset))
}

/// Stand `built` in the studio as the game hangs a body under a chassis:
/// the chassis where [`studio_chassis`] puts it, the root under it by the
/// game's own [`rigged_root_transform`], the worn items queued for the
/// dressing pass (`headless::dress_wear_bodies`, the game's seating), and
/// the body in the [`standing_pose`]. Returns the root.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_standing(
    commands: &mut Commands,
    built: RiggedBuilt,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
) -> Entity {
    let chassis = commands
        .spawn((studio_chassis(built.offset), Visibility::default()))
        .id();
    let mut root = commands.spawn((
        rigged_root_transform(built.offset),
        Visibility::default(),
        AvatarPose(standing_pose(&built.avatar.rig)),
        ChildOf(chassis),
    ));
    if !built.worn.is_empty() {
        root.insert(PendingWear(built.worn));
    }
    let root = root.id();
    spawn_avatar(
        commands,
        root,
        built.avatar,
        0.0,
        meshes,
        materials,
        images,
        bindposes,
    );
    root
}

/// A body built from `--rigged` or `--walker-avatar` before the app stood
/// up, waiting for the system that stands it. Taken once.
#[derive(Resource)]
pub(crate) struct FileBody(pub(crate) Mutex<Option<RiggedBuilt>>);

impl FileBody {
    pub(crate) fn new(built: RiggedBuilt) -> Self {
        Self(Mutex::new(Some(built)))
    }

    /// The body, the first time it is asked for.
    pub(crate) fn take(&self) -> Option<RiggedBuilt> {
        self.0.lock().ok()?.take()
    }
}

/// `flag`'s file read, or what is wrong with it and where, and exit. A
/// malformed file is the author's to fix, and a backtrace would bury the one
/// line that says how.
pub(super) fn read_or_exit(flag: &str, path: &str) -> RiggedAvatar {
    read_rigged(path).unwrap_or_else(|e| {
        eprintln!("{flag} {path}: {e}");
        std::process::exit(2);
    })
}

/// `avatar` built, with what it wears and does not said aloud, or why it
/// cannot be built, and exit.
pub(super) fn build_or_exit(flag: &str, path: &str, avatar: &RiggedAvatar) -> RiggedBuilt {
    let built = build_rigged(avatar).unwrap_or_else(|e| {
        eprintln!("{flag} {path}: {e}");
        std::process::exit(1);
    });
    let rkey = avatar
        .record
        .body
        .rigged_ref()
        .map_or("?", |rig| rig.avatar.as_str());
    println!(
        "rigged body from {path}: wardrobe record {rkey}, {} worn item(s), built at the {} atlas",
        built.worn.len(),
        settled_atlas()
    );
    if !avatar.unread.is_empty() {
        println!(
            "  {} place(s) in the file are not drawn as written (the game reads the file the \
             same way, and says nothing):",
            avatar.unread.len()
        );
        for line in &avatar.unread {
            println!("    {line}");
        }
    }
    for line in avatar.unworn.iter().chain(&built.seating) {
        println!("  {line}");
    }
    built
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::pds::avatar::wardrobe::engine_default_for_seed;
    use serde_json::json;

    /// The avatar's JSON as `agent avatar get ""` writes its value: the
    /// record, the sculpt and the worn items, each in its wire form.
    pub(crate) fn document(record: &AvatarRecord) -> Value {
        let resolved = record
            .body
            .rigged_ref()
            .and_then(|rig| rig.resolved.as_ref())
            .expect("a resolved rigged record");
        let value = avatar_json(record);
        assert!(!value["body"].is_null(), "the sculpt serialises");
        for (i, worn) in resolved.attachments.iter().enumerate() {
            assert!(
                !value["worn"][i]["item"].is_null(),
                "{} serialises",
                worn.rkey
            );
        }
        value
    }

    /// A fresh rigged person, as seed `seed` rolls one, wearing `worn`.
    pub(crate) fn person(seed: u64, worn: Vec<ResolvedAttachment>) -> AvatarRecord {
        let mut record = AvatarRecord::wearing("3jzfcijpj2z2a");
        if let Some(rig) = record.body.rigged_mut() {
            rig.attachments = worn.iter().map(|w| w.rkey.clone()).collect();
            rig.resolved = Some(ResolvedRig {
                body: engine_default_for_seed(seed),
                attachments: worn,
            });
        }
        record.sanitize();
        record
    }

    /// A small box worn at `socket`, at `offset` from its joint (identity
    /// for a fresh wear).
    pub(crate) fn worn_box(rkey: &str, socket: &str, offset: [f32; 3]) -> ResolvedAttachment {
        let mut item = crate::pds::Generator::from_kind(
            crate::pds::GeneratorKind::default_primitive_for_tag("Cuboid")
                .expect("a cuboid is a primitive"),
        );
        item.transform.scale = crate::pds::types::Fp3([0.1, 0.1, 0.1]);
        let mut record = AttachmentRecord::new(item, Socket::Crown);
        record.socket = socket.to_owned();
        record.offset.translation = crate::pds::types::Fp3(offset);
        record.sanitize();
        ResolvedAttachment {
            rkey: rkey.to_owned(),
            record,
        }
    }

    fn parse(value: &Value) -> Result<RiggedAvatar, String> {
        parse_rigged(&serde_json::to_string(value).expect("serialises"))
    }

    /// The two shapes an agent's file comes in - the CLI's whole answer, and
    /// the `value` `rec.py pull avatar` keeps - read to the same avatar, and
    /// it is the one the JSON describes.
    #[test]
    fn both_file_forms_read_to_the_same_avatar() {
        let record = person(3, vec![worn_box("3kaaaaaaaaaaa", "left-hand", [0.0; 3])]);
        let bare = document(&record);
        let answer = json!({
            "ok": true,
            "result": { "pointer": "", "unsaved": true, "value": bare.clone() },
        });
        let from_bare = parse(&bare).expect("the bare value reads");
        let from_answer = parse(&answer).expect("the CLI answer reads");
        assert_eq!(from_bare.record, from_answer.record);
        assert_eq!(from_bare.unworn, from_answer.unworn);
        assert!(
            from_bare.unread.is_empty() && from_answer.unread.is_empty(),
            "a file as `agent avatar get` writes it is drawn as written: {:?} {:?}",
            from_bare.unread,
            from_answer.unread
        );
        assert_eq!(
            document(&from_bare.record),
            bare,
            "and it is the avatar written, sculpt and worn item included"
        );
        assert!(from_bare.unworn.is_empty(), "{:?}", from_bare.unworn);
    }

    /// The session 879 review: a sculpt fetched from the PDS carries its
    /// collection's `$type` (`engine_record_wire`), and every real pulled
    /// avatar was told `/body/$type` was misspelt. The NSID is not named; a
    /// real stray key still is.
    #[test]
    fn a_fetched_sculpts_own_type_is_not_called_misspelt() {
        let record = person(3, vec![worn_box("3kaaaaaaaaaaa", "left-hand", [0.0; 3])]);
        let mut pulled = document(&record);
        pulled["body"]["$type"] = json!(crate::pds::WARDROBE_COLLECTION);
        let read = parse(&pulled).expect("reads");
        assert!(read.unread.is_empty(), "{:?}", read.unread);

        pulled["body"]["colour_scheme"] = json!(1);
        let read = parse(&pulled).expect("reads");
        assert_eq!(read.unread.len(), 1, "{:?}", read.unread);
        assert!(
            read.unread[0].starts_with("/body/colour_scheme"),
            "{:?}",
            read.unread
        );
    }

    /// The session 879 review: a worn item holding a stand-in (a blob shape
    /// this build does not know) is written as nothing on both sides, and
    /// nothing in it was named. It is named, once.
    #[test]
    fn a_worn_item_holding_a_stand_in_is_named() {
        let record = person(3, vec![worn_box("3kaaaaaaaaaaa", "left-hand", [0.0; 3])]);
        let mut pulled = document(&record);
        pulled["worn"][0]["item"]["item"] = json!({
            "$type": "network.symbios.gen.blob_group",
            "resolution": 16,
            "solid": false,
            "elements": [{
                "shape": { "$type": "network.symbios.blob.hyperboloid" },
                "position": [0, 0, 0],
                "rotation": [0, 0, 0, 10000],
                "radii": [1000, 1000, 1000],
                "subtract": false,
                "blend": 500
            }]
        });
        let read = parse(&pulled).expect("a file the game reads is drawn");
        let named: Vec<_> = read
            .unread
            .iter()
            .filter(|n| n.contains("stand-in"))
            .collect();
        assert_eq!(named.len(), 1, "{:?}", read.unread);
        assert!(named[0].starts_with("/worn/0/item:"), "{:?}", read.unread);
    }

    /// A malformed file is refused with the pointer of what is wrong, in the
    /// file as written - through the CLI answer's wrapper too - never with a
    /// panic.
    #[test]
    fn a_malformed_file_is_refused_with_the_key_that_is_wrong() {
        let record = person(3, vec![worn_box("3kaaaaaaaaaaa", "left-hand", [0.0; 3])]);
        let good = document(&record);
        let wrapped =
            |value: Value| json!({ "ok": true, "result": { "pointer": "", "value": value } });
        let edited = |edit: &dyn Fn(&mut Value)| {
            let mut value = good.clone();
            edit(&mut value);
            value
        };
        let cases: Vec<(&str, Value, &str)> = vec![
            (
                "a sculpt axis that is not a number",
                edited(&|v| v["body"]["archetype"]["height"] = json!("tall")),
                "/body/archetype/height: ",
            ),
            (
                "a worn item with no socket",
                edited(&|v| {
                    v["worn"][0]["item"]
                        .as_object_mut()
                        .expect("an item")
                        .remove("socket");
                }),
                "/worn/0/item: missing field `socket`",
            ),
            (
                "a worn item's key that is not a string",
                edited(&|v| v["worn"][0]["rkey"] = json!(5)),
                "/worn/0/rkey: not a string",
            ),
            (
                "a record with no locomotion",
                edited(&|v| {
                    v["record"]
                        .as_object_mut()
                        .expect("a record")
                        .remove("locomotion");
                }),
                "/record: missing field `locomotion`",
            ),
            (
                "no sculpt",
                edited(&|v| v["body"] = Value::Null),
                "/body: null",
            ),
            (
                "worn that is not a list",
                edited(&|v| v["worn"] = json!({})),
                "/worn: an object, not a list",
            ),
            (
                "a generator body",
                edited(&|v| {
                    v["record"] = serde_json::to_value(AvatarRecord::default_for_seed(40))
                        .expect("serialises");
                }),
                "/record/body: a generator body",
            ),
            (
                "a sculpt axis wrong inside the CLI answer",
                wrapped(edited(&|v| v["body"]["eyes"]["size"] = json!([1]))),
                "/result/value/body/eyes/size: ",
            ),
            (
                "an answer for one part of the avatar",
                json!({ "ok": true, "result": { "pointer": "/body", "value": good["body"] } }),
                "/result/pointer: \"/body\"",
            ),
            (
                "a refused command's answer",
                json!({ "ok": false, "error": "no avatar yet" }),
                "/ok: false",
            ),
        ];
        for (what, value, expect) in cases {
            let err = parse(&value).expect_err(what);
            assert!(err.starts_with(expect), "{what}: {err}");
        }
        let err = parse_rigged("{ \"record\": ").expect_err("truncated");
        assert!(err.starts_with("not JSON: "), "{err}");
    }

    /// Inside a part serde read whole, the refusal is placed on the one value
    /// the error describes - and on nothing when two values fit, since a
    /// guess is not a place.
    #[test]
    fn a_tagged_parts_error_is_placed_on_the_one_value_it_describes() {
        let error = "invalid type: string \"tall\", expected i64";
        assert_eq!(
            described_value(&json!({ "height": "tall", "hipWidth": 5 }), error),
            Some("/height".to_owned())
        );
        assert_eq!(
            described_value(&json!({ "a": { "b": [1, "tall"] } }), error),
            Some("/a/b/1".to_owned())
        );
        assert_eq!(
            described_value(&json!({ "height": "tall", "neck": "tall" }), error),
            None
        );
        assert_eq!(
            described_value(
                &json!({ "size": 1.5, "n": 2 }),
                "invalid type: floating point `1.5`, expected i64"
            ),
            Some("/size".to_owned())
        );
        assert_eq!(
            described_value(&json!({ "a": 1 }), "missing field `b`"),
            None
        );
    }

    /// What the file names that the body will not wear is said, never
    /// dropped: a reference with no worn item behind it, a worn item the
    /// record does not name, one at a socket this build does not know and
    /// one at a part the body does not have.
    #[test]
    fn a_worn_item_that_will_not_be_drawn_is_named() {
        let mut record = person(
            3,
            vec![
                worn_box("3kaaaaaaaaaaa", "crown", [0.0; 3]),
                worn_box("3kbbbbbbbbbbb", "elbow", [0.0; 3]),
                worn_box("3kccccccccccc", "tail", [0.0; 3]),
            ],
        );
        if let Some(rig) = record.body.rigged_mut() {
            rig.attachments.retain(|rkey| rkey != "3kccccccccccc");
            rig.attachments.push("3kddddddddddd".to_owned());
        }
        let avatar = parse(&document(&record)).expect("reads");
        let said = avatar.unworn.join("\n");
        assert!(
            said.contains("\"3kddddddddddd\"") && said.contains("did not resolve"),
            "{said}"
        );
        assert!(
            said.contains("\"3kccccccccccc\"") && said.contains("not named"),
            "{said}"
        );
        let built = build_rigged(&avatar).expect("builds");
        assert_eq!(built.seating.len(), 3, "{:?}", built.seating);
        assert!(
            built.seating[0].contains("at crown"),
            "{}",
            built.seating[0]
        );
        assert!(
            built.seating[1].contains("NOT drawn") && built.seating[1].contains("\"elbow\""),
            "{}",
            built.seating[1]
        );
        assert!(
            built.seating[2].contains("NOT drawn - this body has no tail"),
            "{}",
            built.seating[2]
        );
    }

    /// A sculpt edit shows: the same person built at another height stands
    /// at that height, crown and all.
    #[test]
    fn a_taller_sculpt_builds_a_taller_body() {
        let crown_at = |height: i64| {
            let mut value = document(&person(3, Vec::new()));
            value["body"]["archetype"]["height"] = json!(height);
            let built = build_rigged(&parse(&value).expect("reads")).expect("builds");
            Socket::Crown
                .anchor(&built.avatar.rig)
                .expect("a head")
                .position
                .y
        };
        let (short, tall) = (crown_at(1_300), crown_at(2_100));
        assert!(
            tall - short > 0.6,
            "0.8 m more stature moved the crown {:.3} m ({short:.3} -> {tall:.3})",
            tall - short
        );
    }

    /// The close-ups are framed on the rig's own head: on a short body and a
    /// tall one the frame holds every vertex of the skin above the neck's
    /// base and is not much larger than that head.
    #[test]
    fn the_head_close_ups_are_framed_on_the_rigs_own_head() {
        for height in [1_250, 2_150] {
            let mut value = document(&person(7, Vec::new()));
            value["body"]["archetype"]["height"] = json!(height);
            let built = build_rigged(&parse(&value).expect("reads")).expect("builds");
            let frame = built.head.expect("a head to frame");
            let rig = &built.avatar.rig;
            let neck = Socket::Neck.anchor(rig).expect("a neck").position;
            let skull = Socket::Crown.anchor(rig).expect("a head");
            // The skin above the neck's base and within reach of the head's
            // axis - not a shoulder or a splayed arm of the bind pose.
            let head: Vec<Vec3> = built
                .avatar
                .meshes
                .iter()
                .filter(|m| m.kind == symbios_avatar::MeshKind::Skin)
                .flat_map(|m| m.mesh.positions.iter().copied())
                .filter(|p| {
                    p.y > neck.y + 0.02
                        && Vec2::new(p.x - skull.position.x, p.z - skull.position.z).length()
                            < skull.radius * 1.6
                })
                .collect();
            assert!(!head.is_empty(), "{height}: a skin above the neck");
            let reach = head
                .iter()
                .map(|p| p.distance(frame.centre))
                .fold(0.0f32, f32::max);
            assert!(
                reach <= frame.radius,
                "{height}: the head reaches {reach:.3} m from the frame's centre, past its \
                 {:.3} m",
                frame.radius
            );
            assert!(
                frame.radius < reach * 1.6,
                "{height}: a {:.3} m frame for a head {reach:.3} m across is not a close-up",
                frame.radius
            );
        }
    }

    /// The body stands as the game stands a player who is not moving - arms
    /// hung at its sides, not spread in the A-pose it is modelled in - and
    /// its box, the one the sheet frames and prints, is the standing body's.
    #[test]
    fn a_rigged_body_stands_with_its_arms_hung() {
        use bevy::ecs::system::RunSystemOnce;
        let built = build_rigged(&parse(&document(&person(3, Vec::new()))).expect("reads"))
            .expect("builds");
        let bind_width = built
            .avatar
            .meshes
            .iter()
            .flat_map(|m| m.mesh.positions.iter().map(|p| p.x))
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), x| {
                (lo.min(x), hi.max(x))
            });
        let bind_width = bind_width.1 - bind_width.0;
        let (min, max) = built.studio_framing().body;
        assert!(
            max.x - min.x < bind_width * 0.75,
            "the standing body is {:.2} m across, the A-pose {bind_width:.2} m: the arms \
             are not hung",
            max.x - min.x
        );
        assert!(
            min.y.abs() < 0.02 && max.y > 1.0,
            "standing on the studio floor: {min} to {max}"
        );
        let expected = standing_pose(&built.avatar.rig);
        let mut app = crate::player::visuals::spawn_path_app();
        let mut built = Some(built);
        let root = app
            .world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut meshes: ResMut<Assets<Mesh>>,
                      mut materials: ResMut<Assets<StandardMaterial>>,
                      mut images: ResMut<Assets<Image>>,
                      mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                    spawn_standing(
                        &mut commands,
                        built.take().expect("once"),
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut bindposes,
                    )
                },
            )
            .expect("stands");
        let posed = &app
            .world()
            .get::<AvatarPose>(root)
            .expect("the body is posed")
            .0;
        assert_eq!(posed, &expected, "in the standing pose");
    }

    /// A worn item hangs off the joint its socket names, at its own offset,
    /// through the tool's stand-and-dress path.
    #[test]
    fn a_worn_item_hangs_off_its_sockets_joint() {
        use bevy::ecs::system::RunSystemOnce;
        let offset = [0.05, -0.02, 0.03];
        let avatar = parse(&document(&person(
            3,
            vec![worn_box("3kaaaaaaaaaaa", "left-hand", offset)],
        )))
        .expect("reads");
        let built = build_rigged(&avatar).expect("builds");
        let joint = Socket::LeftHand
            .joint(&built.avatar.rig)
            .expect("a left hand");
        let mut app = crate::player::visuals::spawn_path_app();
        let mut built = Some(built);
        let root = app
            .world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut meshes: ResMut<Assets<Mesh>>,
                      mut materials: ResMut<Assets<StandardMaterial>>,
                      mut images: ResMut<Assets<Image>>,
                      mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                    spawn_standing(
                        &mut commands,
                        built.take().expect("once"),
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut bindposes,
                    )
                },
            )
            .expect("stands");
        app.world_mut()
            .run_system_once(super::super::headless::dress_wear_bodies)
            .expect("dresses");
        let joints = app
            .world()
            .get::<bevy_symbios_avatar::AvatarJoints>(root)
            .expect("the body's joints")
            .0
            .clone();
        let carrier = joints[joint];
        let props: Vec<(Transform, Option<super::super::headless::WornAt>)> = app
            .world_mut()
            .query::<(
                Entity,
                &ChildOf,
                &Transform,
                Option<&super::super::headless::WornAt>,
            )>()
            .iter(app.world())
            .filter(|(e, parent, _, _)| parent.parent() == carrier && !joints.contains(e))
            .map(|(_, _, t, at)| (*t, at.copied()))
            .collect();
        assert_eq!(props.len(), 1, "one prop on the left hand's joint");
        assert!(
            (props[0].0.translation - Vec3::from_array(offset)).length() < 1e-3,
            "at its own offset: {}",
            props[0].0.translation
        );
        assert_eq!(
            props[0].1,
            Some(super::super::headless::WornAt(Socket::LeftHand)),
            "marked with its socket, which the head close-ups look for"
        );
    }

    /// #1482 (critic D1): what reads but is not drawn as the file writes it
    /// is named at its pointer - the game reads the file the same way and
    /// says nothing, so the sheet is where it is seen first. And what is
    /// drawn as written is not named: a number written as the other of the
    /// record's two spellings, a field written at the value the record
    /// leaves out.
    #[test]
    fn what_the_file_writes_that_is_not_drawn_is_named() {
        let record = person(3, vec![worn_box("3kaaaaaaaaaaa", "left-hand", [0.0; 3])]);
        let good = document(&record);
        let edited = |edit: &dyn Fn(&mut Value)| {
            let mut value = good.clone();
            edit(&mut value);
            value
        };
        // A surface this build knows, written whole as the record writes it.
        let known_surface = symbios_avatar::SurfaceConfig::all().remove(0);
        let cases: Vec<(&str, Value, Vec<&str>)> = vec![
            (
                "a misspelt sculpt key beside the real one",
                edited(&|v| v["body"]["archetype"]["heigth"] = json!(2150)),
                vec!["/body/archetype/heigth: not a key this build reads"],
            ),
            (
                "a misspelt sculpt key in place of the real one",
                edited(&|v| {
                    let archetype = v["body"]["archetype"].as_object_mut().expect("an object");
                    let height = archetype.remove("height").expect("a height");
                    archetype.insert("heigth".to_owned(), height);
                }),
                vec![
                    "/body/archetype/heigth: not a key this build reads",
                    "/body/archetype/height: not in the file - drawn as",
                ],
            ),
            (
                "a sculpt block under a name the engine does not read",
                edited(&|v| v["body"]["outfti"] = json!({ "top": {} })),
                vec!["/body/outfti: not a key this build reads"],
            ),
            (
                "a misspelt record key",
                edited(&|v| v["record"]["gaitt"] = json!({})),
                vec!["/record/gaitt: not a key this build reads"],
            ),
            (
                "a misspelt worn item key",
                edited(&|v| v["worn"][0]["item"]["sockett"] = json!("crown")),
                vec!["/worn/0/item/sockett: not a key this build reads"],
            ),
            (
                "a stature out of range",
                edited(&|v| v["body"]["archetype"]["height"] = json!(-5000)),
                vec!["/body/archetype/height: -5000 is out of range - drawn as"],
            ),
            (
                "a hair style this build does not know",
                edited(&|v| v["body"]["hair"]["scalp"]["style"] = json!({ "name": "afroo" })),
                vec!["/body/hair/scalp/style/name: \"afroo\" is not a scalp style"],
            ),
            (
                "a body plan this build does not know",
                edited(&|v| {
                    v["body"]["archetype"]["$type"] = json!("network.symbios.avatar.defs#octopus");
                }),
                vec![
                    "/body/archetype/$type: \"network.symbios.avatar.defs#octopus\" is not a body plan",
                ],
            ),
            (
                "a garment surface this build does not know",
                edited(&|v| {
                    v["body"]["outfit"]["trousers"]["texture"] =
                        json!({ "surface": { "$type": "Tartan" }, "scale": 2000, "rotation": 0 });
                }),
                vec!["/body/outfit/trousers/texture/surface: \"Tartan\" is not a surface"],
            ),
            (
                "a garment texture axis that does not read",
                edited(&|v| {
                    v["body"]["outfit"]["trousers"]["texture"] = json!({
                        "surface": Value::Object(known_surface.to_wire()),
                        "scale": "x",
                        "rotation": 0,
                    });
                }),
                vec!["/body/outfit/trousers/texture/scale: \"x\" does not read as written"],
            ),
            (
                "a worn part of a kind this build does not know",
                edited(&|v| v["worn"][0]["item"]["item"]["$type"] = json!("Blob9")),
                vec!["/worn/0/item/item: worn 3kaaaaaaaaaaa's part of kind \"Blob9\""],
            ),
            (
                "a locomotion this build does not know, which cannot be written back",
                edited(&|v| {
                    v["record"]["locomotion"]["$type"] =
                        json!("network.symbios.locomotion.jetpack");
                    v["record"]["gaitt"] = json!({});
                }),
                vec![
                    "/record/locomotion/$type: \"network.symbios.locomotion.jetpack\" is not a locomotion",
                ],
            ),
            (
                "a place named through the CLI answer's wrapper",
                json!({
                    "ok": true,
                    "result": {
                        "pointer": "",
                        "value": edited(&|v| v["body"]["archetype"]["heigth"] = json!(2150)),
                    },
                }),
                vec!["/result/value/body/archetype/heigth: not a key this build reads"],
            ),
        ];
        for (what, value, expected) in cases {
            let avatar = parse(&value).unwrap_or_else(|e| panic!("{what}: refused: {e}"));
            for expect in &expected {
                assert!(
                    avatar.unread.iter().any(|note| note.starts_with(expect)),
                    "{what}: no note starts {expect:?} in {:#?}",
                    avatar.unread
                );
            }
            assert_eq!(
                avatar.unread.len(),
                expected.len(),
                "{what}: one note a place, and no other: {:#?}",
                avatar.unread
            );
        }

        // Drawn as written, so not named.
        let seed = good["body"]["seed"]
            .as_str()
            .expect("the record writes its seed as a string")
            .parse::<i64>()
            .expect("a seed");
        let quiet: Vec<(&str, Value)> = vec![
            (
                "the seed as a bare number, which the engine reads too",
                edited(&|v| v["body"]["seed"] = json!(seed)),
            ),
            (
                "a worn item's offset written out at its identity, which the record elides",
                edited(&|v| {
                    v["worn"][0]["item"]["offset"] = json!({ "translation": [0, 0, 0] });
                }),
            ),
        ];
        for (what, value) in quiet {
            let avatar = parse(&value).unwrap_or_else(|e| panic!("{what}: refused: {e}"));
            assert!(avatar.unread.is_empty(), "{what}: {:#?}", avatar.unread);
        }
    }

    /// #1482 (critic minor 1): a worn item whose root part is of a kind this
    /// build does not know draws nothing, and its seating line says so rather
    /// than calling it seated; one with such a part lower down says what it
    /// is less.
    #[test]
    fn a_worn_item_of_a_kind_this_build_does_not_know_is_not_called_seated() {
        let mut item = worn_box("3kaaaaaaaaaaa", "left-hand", [0.0; 3]);
        item.record.item.children = vec![crate::pds::Generator::from_kind(
            crate::pds::GeneratorKind::Unknown,
        )];
        let lower = build_rigged(&RiggedAvatar {
            record: person(3, vec![item.clone()]),
            unworn: Vec::new(),
            unread: Vec::new(),
        })
        .expect("builds");
        assert!(
            lower.seating[0].contains("at left-hand")
                && lower.seating[0].contains("less 1 part(s) of a kind"),
            "{}",
            lower.seating[0]
        );
        item.record.item.kind = crate::pds::GeneratorKind::Unknown;
        let root = build_rigged(&RiggedAvatar {
            record: person(3, vec![item]),
            unworn: Vec::new(),
            unread: Vec::new(),
        })
        .expect("builds");
        assert!(
            root.seating[0].contains("NOT drawn - its root part"),
            "{}",
            root.seating[0]
        );
    }

    /// #1482 (critic minor 6): a field missing inside a part read whole - a
    /// hair style, read through a value of its own - is refused at the
    /// object it is missing from, not at the whole of the hair.
    #[test]
    fn a_missing_field_is_placed_in_the_object_it_is_missing_from() {
        let mut value = document(&person(3, Vec::new()));
        value["body"]["hair"]["scalp"]["style"] = json!({ "name": "long" });
        let err = parse(&value).expect_err("a long scalp with no weight");
        assert!(
            err.starts_with("/body/hair/scalp/style: missing field `weight`"),
            "{err}"
        );
    }

    /// #1482 (critic D2): the full-body views draw every body that fits the
    /// stage at one scale - a short body and a tall one are shot by the same
    /// cameras, so an edit to stature shows as stature - and every corner of
    /// each, and of a body taller than the stage, is in frame.
    #[test]
    fn a_short_and_a_tall_body_are_drawn_at_one_scale() {
        use super::super::headless::{rigged_camera, rigged_stage};
        let framing_at = |height: i64| {
            let mut value = document(&person(3, Vec::new()));
            value["body"]["archetype"]["height"] = json!(height);
            build_rigged(&parse(&value).expect("reads"))
                .expect("builds")
                .studio_framing()
        };
        let (short, tall) = (framing_at(1_300), framing_at(2_100));
        assert!(
            tall.body.1.y - short.body.1.y > 0.6,
            "{:?} {:?}",
            short.body,
            tall.body
        );
        let in_view = |camera: &Transform, point: Vec3| {
            let local = camera.compute_affine().inverse().transform_point3(point);
            let reach = -local.z * (super::super::FOV * 0.5).tan();
            local.z < 0.0 && local.x.abs() <= reach && local.y.abs() <= reach
        };
        let corners = |(min, max): (Vec3, Vec3)| {
            (0..8).map(move |i| {
                Vec3::new(
                    if i & 1 == 0 { min.x } else { max.x },
                    if i & 2 == 0 { min.y } else { max.y },
                    if i & 4 == 0 { min.z } else { max.z },
                )
            })
        };
        let head = short.head.expect("a head");
        let giant = (Vec3::new(-0.35, 0.0, -0.2), Vec3::new(0.35, 3.0, 0.2));
        for tile in 0..super::super::ANGLES.len() {
            let (a, b) = (
                rigged_camera(tile, rigged_stage(short.body), head, None),
                rigged_camera(tile, rigged_stage(tall.body), head, None),
            );
            assert!(
                a.translation.distance(b.translation) < 0.02
                    && a.rotation.angle_between(b.rotation) < 0.01,
                "tile {tile}: the short body's camera {a:?}, the tall one's {b:?}"
            );
            for body in [short.body, tall.body, giant] {
                let stage = rigged_stage(body);
                let camera = rigged_camera(tile, stage, head, None);
                for corner in corners(body).chain(corners(stage)) {
                    assert!(
                        in_view(&camera, corner),
                        "tile {tile}: {corner} of {body:?} (stage {stage:?}) is out of frame"
                    );
                }
            }
            // Not so far off that the body is a figure in the distance: the
            // stage's height fills most of the frame.
            let camera = rigged_camera(tile, rigged_stage(tall.body), head, None);
            let (min, max) = rigged_stage(tall.body);
            let middle = (min + max) * 0.5;
            let span = camera.translation.distance(middle) * (super::super::FOV * 0.5).tan() * 2.0;
            assert!(
                (max.y - min.y) / span > 0.7,
                "tile {tile}: a {:.2} m stage in a {span:.2} m frame",
                max.y - min.y
            );
        }
    }

    /// #1482 (critic minor 5): the front views see the face and the back
    /// view the back - the body stood by [`spawn_standing`], the cameras
    /// placed as the sheet places them.
    #[test]
    fn the_front_views_see_the_face() {
        use super::super::headless::{rigged_camera, rigged_head, rigged_stage};
        use bevy::ecs::system::RunSystemOnce;
        let built = build_rigged(&parse(&document(&person(3, Vec::new()))).expect("reads"))
            .expect("builds");
        let framing = built.studio_framing();
        let rig = built.avatar.rig.clone();
        let mut app = crate::player::visuals::spawn_path_app();
        let mut built = Some(built);
        let root = app
            .world_mut()
            .run_system_once(
                move |mut commands: Commands,
                      mut meshes: ResMut<Assets<Mesh>>,
                      mut materials: ResMut<Assets<StandardMaterial>>,
                      mut images: ResMut<Assets<Image>>,
                      mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                    spawn_standing(
                        &mut commands,
                        built.take().expect("once"),
                        &mut meshes,
                        &mut materials,
                        &mut images,
                        &mut bindposes,
                    )
                },
            )
            .expect("stands");
        let world = app.world();
        let chassis = world.get::<ChildOf>(root).expect("hung").parent();
        let to_world = world
            .get::<Transform>(chassis)
            .expect("a chassis")
            .mul_transform(*world.get::<Transform>(root).expect("a root"));
        let face = to_world.transform_point(Socket::Face.anchor(&rig).expect("a face").position);
        let skull = to_world.transform_point(Socket::Crown.anchor(&rig).expect("a head").position);
        let facing = (face - skull).with_y(0.0).normalize();
        let stage = rigged_stage(framing.body);
        let head = rigged_head(framing.head, framing.body, None);
        let seen_from = |tile: usize| {
            (rigged_camera(tile, stage, head, None).translation - skull)
                .with_y(0.0)
                .normalize()
                .dot(facing)
        };
        assert!(seen_from(0) > 0.95, "the front view: {}", seen_from(0));
        assert!(seen_from(4) > 0.95, "the front close-up: {}", seen_from(4));
        assert!(seen_from(3) < -0.95, "the back view: {}", seen_from(3));
    }
}
