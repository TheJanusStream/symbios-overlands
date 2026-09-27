//! `--generator FILE` drawn as its record keeps it (#1486).
//!
//! The file was parsed and drawn as written, but a generator in a room
//! record passes the sanitiser first, and the sanitiser does more than pull
//! numbers into range: a `BlobGroup` keeps its first 16 elements and drops
//! the rest. A 34-element cow drew whole on the turntable and stood in the
//! world with no legs and no head - the sheet was the one place it looked
//! right. The file is now sanitised as its record's generator is before it
//! is drawn ([`generator_as_kept`]): a room piece by default, an avatar
//! body's visuals with `--body` and for a `--lineup` file entry
//! ([`KeptAs`]). Each place it is not drawn as written is named, one line a
//! place: a key this build does not read first, then a list cut short (one
//! line for the whole cut) or reshaped, then a value pulled into range or
//! onto its default.

use std::collections::BTreeMap;

use serde_json::Value;

use super::rigged::{Difference, json_differences, same_number, shown};
use crate::pds::Generator;

/// The most places named; past it, one line says how many more.
const MAX_NOTES: usize = 24;

/// Which record a file is kept as: a room piece or an avatar body's visuals
/// (the session 879 review - a `--lineup` vehicle prototype went through the
/// room sanitiser, whose 100 m parts and unbounded scale a body never gets).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeptAs {
    /// A generator of a room record: `sanitize_generator`.
    Room,
    /// An avatar body's visuals: `sanitize_avatar_visuals` (16 m parts, a
    /// scale product of 4, no terrain, water or portal).
    Body,
}

impl KeptAs {
    /// The record, as the header names it.
    pub(super) fn record(self) -> &'static str {
        match self {
            Self::Room => "a room record",
            Self::Body => "an avatar body",
        }
    }

    /// The agent command that answers for the record.
    pub(super) fn set_command(self) -> &'static str {
        match self {
            Self::Room => "agent room set",
            Self::Body => "agent avatar set",
        }
    }

    fn sanitise(self, generator: &mut Generator) {
        match self {
            Self::Room => crate::pds::sanitize::sanitize_generator(generator),
            Self::Body => crate::pds::sanitize::sanitize_avatar_visuals(generator),
        }
    }
}

/// A generator file as its record keeps it.
pub(super) struct Kept {
    /// What is drawn: the file read and sanitised.
    pub(super) generator: Generator,
    /// Each place the file is not drawn as written, a misspelt key first;
    /// past [`MAX_NOTES`] the last line says how many more.
    pub(super) notes: Vec<String>,
    /// How many places there are, all of them.
    pub(super) places: usize,
}

/// Read the generator at `path` as `kept_as` keeps it, with what the file
/// writes that is not drawn. A file that will not read is fatal, as it
/// always was.
pub(super) fn read_generator(path: &str, kept_as: KeptAs) -> Kept {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read generator {path:?}: {e}"));
    generator_as_kept(&text, kept_as).unwrap_or_else(|e| panic!("parse generator {path:?}: {e}"))
}

/// `text` read as a [`Generator`] and sanitised as `kept_as` is, and the
/// places the result is not what `text` writes.
pub(super) fn generator_as_kept(text: &str, kept_as: KeptAs) -> Result<Kept, String> {
    let written: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let mut generator: Generator =
        serde_json::from_value(written.clone()).map_err(|e| e.to_string())?;
    let read = serde_json::to_value(&generator);
    kept_as.sanitise(&mut generator);
    // An open union reads a value this build does not know (a prop mapping
    // "Needle", a blob shape from a newer engine) as a stand-in that
    // refuses to be written (#1487): the game draws the stand-in, so the
    // sheet does too, and says so - it cannot say where the rest differs.
    let (read, kept) = match (read, serde_json::to_value(&generator)) {
        (Ok(read), Ok(kept)) => (read, kept),
        (Err(e), _) | (_, Err(e)) => {
            let note = format!(
                "the file holds a value this build reads as a stand-in and cannot write back \
                 ({e}) - drawn as the stand-in, as the game draws it; `{}` refuses to save it \
                 (a misspelt kind or $type?)",
                kept_as.set_command()
            );
            return Ok(Kept {
                generator,
                notes: vec![note],
                places: 1,
            });
        }
    };
    // a misspelt key first: past the cap, clamps are what goes unlisted
    let mut notes = unread(&written, &read);
    notes.extend(cut_and_clamped(&read, &kept));
    let places = notes.len();
    if notes.len() > MAX_NOTES {
        let more = notes.len() - (MAX_NOTES - 1);
        notes.truncate(MAX_NOTES - 1);
        notes.push(format!("... and {more} more"));
    }
    Ok(Kept {
        generator,
        notes,
        places,
    })
}

/// A list the sanitiser cut or reshaped, found by walking `read` and `kept`
/// together: every item it kept is one the file wrote. `tail` when it kept
/// the file's first items in order (a cap cut the rest); otherwise it
/// dropped from the middle or reordered (duplicate faces, sorted meshes).
struct Reshaped {
    list: String,
    had: usize,
    kept: usize,
    tail: bool,
}

fn reshaped_lists(read: &Value, kept: &Value, at: &str, out: &mut Vec<Reshaped>) {
    match (read, kept) {
        (Value::Object(a), Value::Object(b)) => {
            for (key, x) in a {
                if let Some(y) = b.get(key) {
                    let here = format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"));
                    reshaped_lists(x, y, &here, out);
                }
            }
        }
        (Value::Array(a), Value::Array(b)) if a != b => {
            if b.len() <= a.len() && b.iter().all(|item| a.contains(item)) {
                out.push(Reshaped {
                    list: at.to_owned(),
                    had: a.len(),
                    kept: b.len(),
                    tail: a[..b.len()] == b[..],
                });
            } else {
                for (i, (x, y)) in a.iter().zip(b).enumerate() {
                    reshaped_lists(x, y, &format!("{at}/{i}"), out);
                }
            }
        }
        _ => {}
    }
}

/// What the sanitiser changed, `read` against `kept`: a list cut short is
/// one line, whatever it cut; a list reshaped otherwise is one line too,
/// with no item-by-item notes that would name the wrong items.
fn cut_and_clamped(read: &Value, kept: &Value) -> Vec<String> {
    let mut reshaped = Vec::new();
    reshaped_lists(read, kept, "", &mut reshaped);
    let within = |here: &str| {
        reshaped.iter().any(|r| {
            here == r.list
                || here
                    .strip_prefix(r.list.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        })
    };
    let mut differences = Vec::new();
    json_differences(read, kept, "", &mut differences);
    let mut cut: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut notes = Vec::new();
    for difference in &differences {
        let here = difference.pointer();
        if within(here) {
            continue;
        }
        let (before, after) = (read.pointer(here), kept.pointer(here));
        match difference {
            Difference::Dropped(_) => match list_item(here, read) {
                Some((list, index)) => cut.entry(list.to_owned()).or_default().push(index),
                // a record leaves a default out, so a value clamped onto its
                // default reads as gone: it is drawn at that default
                None => notes.push(format!(
                    "{here}: {} is not kept - drawn at its default, which a record leaves out",
                    before.map_or_else(String::new, shown)
                )),
            },
            Difference::Changed(_) => {
                if let (Some(before), Some(after)) = (before, after)
                    && !same_number(before, after)
                {
                    notes.push(format!(
                        "{here}: {} is out of range - drawn as {}, as a record keeps it",
                        shown(before),
                        shown(after)
                    ));
                }
            }
            Difference::Added(_) => notes.push(format!(
                "{here}: not in the file - drawn as {}, as a record sets it",
                after.map_or_else(String::new, shown)
            )),
        }
    }
    let mut lines: Vec<String> = reshaped
        .iter()
        .map(|r| {
            if r.tail {
                format!(
                    "{}: {} items, a record keeps {} - {}/{}..{} are dropped and not drawn",
                    r.list,
                    r.had,
                    r.kept,
                    r.list,
                    r.kept,
                    r.had - 1
                )
            } else {
                format!(
                    "{}: {} items, a record keeps {} - duplicates dropped or the list put in \
                     order, drawn as a record keeps it",
                    r.list, r.had, r.kept
                )
            }
        })
        .collect();
    // a list that shrank while its kept items were also clamped: the tail it lost
    lines.extend(cut.into_iter().map(|(list, gone)| {
        let had = read
            .pointer(&list)
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let (first, last) = (gone.iter().min(), gone.iter().max());
        format!(
            "{list}: {had} items, a record keeps {} - {list}/{}..{} are dropped and not drawn",
            had - gone.len(),
            first.copied().unwrap_or(0),
            last.copied().unwrap_or(0)
        )
    }));
    lines.extend(notes);
    lines
}

/// `(list, index)` when `pointer` is an item of a list in `document`.
fn list_item<'a>(pointer: &'a str, document: &Value) -> Option<(&'a str, usize)> {
    let (list, index) = pointer.rsplit_once('/')?;
    let index = index.parse().ok()?;
    document.pointer(list)?.as_array()?;
    Some((list, index))
}

/// What of `written` did not read as written, against `read`: a key the
/// reader skips (misspelt, or for a newer build), a value read otherwise.
/// A value the file writes at its default and the wire leaves out is not
/// named - it draws as written.
fn unread(written: &Value, read: &Value) -> Vec<String> {
    let mut differences = Vec::new();
    json_differences(written, read, "", &mut differences);
    let mut notes = Vec::new();
    for difference in &differences {
        let here = difference.pointer();
        match (difference, written.pointer(here), read.pointer(here)) {
            (Difference::Changed(_), Some(was), Some(now)) if !same_number(was, now) => {
                notes.push(format!(
                    "{here}: {} does not read as written - drawn as {}",
                    shown(was),
                    shown(now)
                ));
            }
            (Difference::Dropped(_), Some(was), None) if skipped(written, here) => {
                notes.push(format!(
                    "{here}: not a key this build reads (misspelt?) - {} is ignored",
                    shown(was)
                ));
            }
            _ => {}
        }
    }
    notes
}

/// Whether the key at `pointer` is one the reader skips, asked of the
/// reader: the key's value replaced by one no field could take, and the
/// file read again. A field refuses it or reads otherwise; a skipped key
/// changes nothing.
fn skipped(written: &Value, pointer: &str) -> bool {
    let reread = |value: &Value| {
        serde_json::from_value::<Generator>(value.clone())
            .ok()
            .and_then(|g| serde_json::to_value(g).ok())
    };
    let mut probe = written.clone();
    let Some(slot) = probe.pointer_mut(pointer) else {
        return false;
    };
    *slot = Value::String("\u{1}not a value any field takes".to_owned());
    matches!((reread(written), reread(&probe)), (Some(a), Some(b)) if a == b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::GeneratorKind;
    use serde_json::json;

    fn sphere(x: i64) -> Value {
        json!({
            "shape": { "$type": "network.symbios.blob.sphere" },
            "position": [x, 0, 0],
            "rotation": [0, 0, 0, 10000],
            "radii": [1000, 1000, 1000],
            "subtract": false,
            "blend": 500
        })
    }

    fn blob_group(elements: usize) -> Value {
        json!({
            "$type": "network.symbios.gen.blob_group",
            "resolution": 16,
            "solid": false,
            "elements": (0..elements as i64).map(|i| sphere(i * 1000)).collect::<Vec<_>>()
        })
    }

    fn cuboid() -> Value {
        json!({
            "$type": "network.symbios.gen.cuboid",
            "size": [10000, 10000, 10000],
            "solid": false
        })
    }

    fn kept(file: &Value) -> Kept {
        generator_as_kept(&file.to_string(), KeptAs::Room).expect("the file reads")
    }

    fn elements(generator: &Generator) -> usize {
        serde_json::to_value(generator).expect("writes")["elements"]
            .as_array()
            .map_or(0, Vec::len)
    }

    /// THE CASE (#1486): a 34-element cow drew whole on the sheet while the
    /// world kept 16 of its elements - legs and head gone, and the sheet
    /// the one place it looked right. The sheet now draws the 16, and says
    /// so in one line rather than eighteen.
    #[test]
    fn a_blob_group_past_the_cap_is_drawn_as_a_record_keeps_it() {
        let kept = kept(&blob_group(20));
        assert_eq!(elements(&kept.generator), 16, "drawn as the world keeps it");
        assert_eq!(kept.notes.len(), 1, "{:?}", kept.notes);
        assert!(
            kept.notes[0].starts_with("/elements: 20 items, a record keeps 16 - /elements/16..19"),
            "{:?}",
            kept.notes
        );
    }

    /// Non-vacuity: a group the record keeps whole is drawn whole and
    /// nothing is said about it.
    #[test]
    fn a_blob_group_within_the_cap_is_drawn_whole_and_says_nothing() {
        let kept = kept(&blob_group(16));
        assert_eq!(elements(&kept.generator), 16);
        assert!(kept.notes.is_empty(), "{:?}", kept.notes);
    }

    /// The session 879 review: every key the fixtures above write is one
    /// the wire keeps, so nothing tried the other half of the probe - a
    /// default the file writes and the record leaves out (an identity
    /// transform, empty faces and children) is drawn as written and must not
    /// be called a misspelt key.
    #[test]
    fn a_default_the_record_leaves_out_is_not_named() {
        let mut file = cuboid();
        file["transform"] = json!({ "translation": [0, 0, 0], "rotation": [0, 0, 0, 10000] });
        file["faces"] = json!([]);
        file["children"] = json!([]);
        let kept = kept(&file);
        assert!(kept.notes.is_empty(), "{:?}", kept.notes);
    }

    /// #1487: a value an open union reads as its stand-in cannot be written
    /// back, and the comparison that names the cut wrote it - so a file the
    /// game draws made the tool panic. It is drawn, as the game draws it,
    /// and the stand-in is named.
    #[test]
    fn a_value_read_as_a_stand_in_is_drawn_and_named() {
        let mut file = blob_group(3);
        file["elements"][1]["shape"] = json!({ "$type": "network.symbios.blob.hyperboloid" });
        let kept = kept(&file);
        let GeneratorKind::BlobGroup { elements, .. } = &kept.generator.kind else {
            panic!("still a blob group: {:?}", kept.generator);
        };
        assert_eq!(elements.len(), 3, "the stand-in keeps its place");
        assert_eq!(kept.notes.len(), 1, "{:?}", kept.notes);
        assert!(kept.notes[0].contains("stand-in"), "{:?}", kept.notes);
        assert!(kept.notes[0].contains("agent room set"), "{:?}", kept.notes);
    }

    /// A misspelt key is named, and a value pulled into range is named
    /// with what is drawn instead.
    #[test]
    fn a_misspelt_key_and_a_clamped_value_are_named() {
        let mut file = blob_group(2);
        file["resolutoin"] = json!(20);
        file["resolution"] = json!(90);
        let kept = kept(&file);
        assert!(
            kept.notes
                .iter()
                .any(|n| n.starts_with("/resolutoin: not a key")),
            "{:?}",
            kept.notes
        );
        assert!(
            kept.notes
                .iter()
                .any(|n| n.starts_with("/resolution: 90 is out of range - drawn as 48")),
            "{:?}",
            kept.notes
        );
    }

    /// The session 879 tools review: a Bark `furrow_shape` of 5 is held to
    /// 2, its default, and a record leaves a default out - so the note said
    /// the value was dropped while the sheet drew it at 2.
    #[test]
    fn a_value_clamped_onto_its_default_is_named_as_drawn_at_it() {
        let mut file = cuboid();
        file["material"] = json!({
            "base_color": [10000, 10000, 10000],
            "texture": {"$type": "Bark", "furrow_shape": 50000}
        });
        let kept = kept(&file);
        assert_eq!(
            kept.notes,
            [
                "/material/texture/furrow_shape: 50000 is not kept - drawn at its default, \
              which a record leaves out"
            ],
        );
    }

    /// The session 879 review: past the cap the misspelt key - the note
    /// the probe exists for - was the first cut, and the count said 24
    /// whatever the real number. It comes first, and the count is all.
    #[test]
    fn a_misspelt_key_is_named_first_and_every_place_is_counted() {
        let mut file = blob_group(16);
        for i in 0..16 {
            file["elements"][i]["blend"] = json!(900_000);
            file["elements"][i]["position"] = json!([2_000_000, 0, 0]); // 200 m: held to 100
        }
        file["resolutoin"] = json!(20);
        let kept = kept(&file);
        assert!(
            kept.notes[0].starts_with("/resolutoin: not a key"),
            "{:?}",
            kept.notes
        );
        assert_eq!(kept.places, 33, "{:?}", kept.notes);
        assert_eq!(kept.notes.len(), MAX_NOTES);
        assert!(
            kept.notes[MAX_NOTES - 1].starts_with("... and 10 more"),
            "{:?}",
            kept.notes
        );
    }

    /// The session 879 review: a list the sanitiser shortens from the
    /// middle (a duplicate face override: the first of a face wins) was
    /// named as its tail cut, the kept items after the gap as clamped -
    /// "/faces/2 is dropped" of the face that is drawn. It is one line
    /// for the list, and no item is named wrongly.
    #[test]
    fn a_list_shortened_from_the_middle_is_not_named_as_its_tail() {
        let face = |key: &str, c: i64| {
            json!({
                "face": { "$type": format!("network.symbios.face.{key}") },
                "material": { "base_color": [c, c, c] }
            })
        };
        let mut file = cuboid();
        file["faces"] = json!([face("top", 9000), face("top", 2000), face("bottom", 5000)]);
        let kept = kept(&file);
        assert_eq!(kept.notes.len(), 1, "{:?}", kept.notes);
        assert!(
            kept.notes[0].starts_with("/faces: 3 items, a record keeps 2 - duplicates dropped"),
            "{:?}",
            kept.notes
        );
    }

    /// The session 879 review: a body file went through the room sanitiser,
    /// whose scale has no bound; a body's visuals hold a scale product of 4.
    /// Kept as a body, the root scaled 5 is drawn at 4 and named.
    #[test]
    fn a_body_file_is_kept_as_a_body() {
        let mut file = cuboid();
        file["transform"] = json!({ "scale": [50000, 50000, 50000] });
        let room = generator_as_kept(&file.to_string(), KeptAs::Room).expect("reads");
        assert!(
            room.notes.is_empty(),
            "a room piece may be that big: {:?}",
            room.notes
        );

        let body = generator_as_kept(&file.to_string(), KeptAs::Body).expect("reads");
        assert!(
            body.notes
                .iter()
                .any(|n| n.starts_with("/transform/scale/")),
            "{:?}",
            body.notes
        );
        assert!(body.generator.transform.scale.0[0] <= 4.0 + 1e-3);
    }
}
