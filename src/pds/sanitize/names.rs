//! Generator-name hygiene shared by [`crate::pds::room::RoomRecord`] and
//! [`crate::pds::inventory::InventoryRecord`] (#1205).
//!
//! A generator name is the key that placements, traits and wear metadata
//! use to find their generator, and the label every tree row, combo and
//! toast paints. Two things break that quietly: an over-long key (which
//! `InventoryRecord::sanitize` used to answer by *deleting the item* after
//! the owner had watched it save), and an invisible character — a
//! zero-width space, a bidi override, a control byte — that makes two
//! distinct keys paint identically or a row paint as nothing at all.
//!
//! [`sanitize_keys`] is the one place both records clean their maps, and
//! it reports every rename so the caller can move the key's dependants
//! (placements, traits, wear) with it. The rename dialog's validator
//! ([`crate::ui::confirm::validate_new_key`]) refuses the same set at the
//! point of typing, so a record this client wrote never needs the repair.

use std::collections::HashMap;

/// True for a character that paints as nothing and so must not appear in
/// a name: every ASCII / Latin-1 control (`char::is_control`, category Cc)
/// plus the format characters (category Cf) epaint renders zero-width —
/// U+200B–U+200F (zero-width space / non-joiner / joiner, LRM, RLM),
/// U+202A–U+202E (bidi embeddings and overrides), U+2060–U+2064 (word
/// joiner and invisible operators), U+2066–U+2069 (bidi isolates), U+FEFF
/// (BOM), U+00AD (soft hyphen), U+061C (Arabic letter mark) and U+180E
/// (Mongolian vowel separator).
///
/// The joiner is the deliberate exception at the call sites: an emoji
/// family is `👨\u{200D}👩` and is perfectly visible. [`clean_name`] keeps
/// U+200D only when it sits between two characters that are themselves
/// visible and outside ASCII, which is what a joined emoji looks like and
/// what a `"Tree\u{200D}"` twin never does.
pub fn is_invisible(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}'
                | '\u{00AD}'
                | '\u{061C}'
                | '\u{180E}'
        )
}

/// Whether the joiner at `chars[i]` is doing visible work: both
/// neighbours exist and are visible non-ASCII characters (an emoji, a
/// Devanagari conjunct), so stripping it would change what is painted.
fn joiner_is_load_bearing(chars: &[char], i: usize) -> bool {
    let visible_non_ascii = |c: &char| !c.is_ascii() && !c.is_whitespace() && !is_invisible(*c);
    i > 0
        && chars.get(i - 1).is_some_and(visible_non_ascii)
        && chars.get(i + 1).is_some_and(visible_non_ascii)
}

/// Does `name` carry any character [`clean_name`] would strip?
pub fn has_invisible(name: &str) -> bool {
    let chars: Vec<char> = name.chars().collect();
    chars.iter().enumerate().any(|(i, &c)| {
        if c == '\u{200D}' {
            !joiner_is_load_bearing(&chars, i)
        } else {
            is_invisible(c)
        }
    })
}

/// Strip invisible characters, trim, and cut to `max_chars` characters.
/// `None` when nothing visible is left — such a key names no row and is
/// dropped by [`sanitize_keys`].
///
/// The cut is in `chars`, not bytes, so it can never split a scalar; it
/// is applied after the strip so a name padded with a thousand zero-width
/// spaces does not spend its whole budget on them.
pub fn clean_name(raw: &str, max_chars: usize) -> Option<String> {
    let chars: Vec<char> = raw.chars().collect();
    let kept: String = chars
        .iter()
        .enumerate()
        .filter(|&(i, &c)| {
            if c == '\u{200D}' {
                joiner_is_load_bearing(&chars, i)
            } else {
                !is_invisible(c)
            }
        })
        .map(|(_, &c)| c)
        .collect();
    let trimmed: String = kept.trim().chars().take(max_chars).collect();
    // A trailing space left by the cut is trimmed again so the result is
    // exactly what `validate_new_key` would have accepted.
    let trimmed = trimmed.trim_end().to_owned();
    (!trimmed.is_empty()).then_some(trimmed)
}

/// Rewrite every key of `map` through [`clean_name`] in lexicographic key
/// order, returning the `(old, new)` pairs that changed so the caller can
/// move whatever else is keyed by the old name.
///
/// Deterministic on purpose — this runs on every peer that loads the
/// record, and two peers keeping different survivors would fracture the
/// shared world. A key that cleans to nothing is removed. A key whose
/// clean form collides with a key that already exists (or with an earlier
/// rename's target) is removed rather than overwriting: the survivor is
/// always the lexicographically first claimant, never the last writer.
pub fn sanitize_keys<V>(map: &mut HashMap<String, V>, max_chars: usize) -> Vec<(String, String)> {
    let mut keys: Vec<String> = map.keys().cloned().collect();
    keys.sort();
    // Keys that already satisfy the rules keep their slot unconditionally,
    // so a rename can never displace an untouched neighbour.
    let mut claimed: std::collections::HashSet<String> = keys
        .iter()
        .filter(|k| clean_name(k, max_chars).as_deref() == Some(k.as_str()))
        .cloned()
        .collect();
    let mut renames = Vec::new();
    for old in keys {
        let cleaned = clean_name(&old, max_chars);
        if cleaned.as_deref() == Some(old.as_str()) {
            continue;
        }
        let Some(value) = map.remove(&old) else {
            continue;
        };
        let Some(new) = cleaned else {
            continue;
        };
        if claimed.insert(new.clone()) {
            map.insert(new.clone(), value);
            renames.push((old, new));
        }
    }
    renames
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invisible_set_covers_zero_width_bidi_and_controls() {
        for c in [
            '\u{200B}', '\u{200E}', '\u{202E}', '\u{2066}', '\u{FEFF}', '\u{00AD}', '\u{7}',
        ] {
            assert!(is_invisible(c), "{c:?} must count as invisible");
        }
        for c in ['a', 'あ', '🌳', ' ', '-', '\u{FE0F}'] {
            assert!(!is_invisible(c), "{c:?} must stay visible");
        }
    }

    #[test]
    fn clean_name_strips_invisibles_but_keeps_an_emoji_joiner() {
        assert_eq!(clean_name("Tree\u{200B}", 256).as_deref(), Some("Tree"));
        assert_eq!(clean_name("\u{202E}eerT", 256).as_deref(), Some("eerT"));
        assert_eq!(clean_name("\u{200B}", 256), None);
        assert_eq!(clean_name("  \u{7}  ", 256), None);
        // A family emoji is joined with U+200D and stays intact …
        let family = "👨\u{200D}👩\u{200D}👧";
        assert_eq!(clean_name(family, 256).as_deref(), Some(family));
        assert!(!has_invisible(family));
        // … while a trailing or ASCII-flanked joiner is an invisible twin.
        assert_eq!(clean_name("Tree\u{200D}", 256).as_deref(), Some("Tree"));
        assert_eq!(clean_name("a\u{200D}b", 256).as_deref(), Some("ab"));
        assert!(has_invisible("Tree\u{200D}"));
        assert!(has_invisible("Tree\u{200B}"));
        assert!(!has_invisible("Tree"));
    }

    #[test]
    fn clean_name_cuts_in_chars_after_stripping() {
        let padded = format!("{}{}", "\u{200B}".repeat(300), "あ".repeat(10));
        assert_eq!(
            clean_name(&padded, 256).as_deref(),
            Some("あ".repeat(10).as_str())
        );
        let long = "あ".repeat(300);
        let cut = clean_name(&long, 256).unwrap();
        assert_eq!(cut.chars().count(), 256);
        assert!(long.starts_with(&cut));
        // A cut that lands on a space does not leave it dangling.
        assert_eq!(clean_name("ab cd", 3).as_deref(), Some("ab"));
    }

    #[test]
    fn sanitize_keys_renames_drops_and_never_overwrites() {
        let mut map: HashMap<String, u8> = HashMap::new();
        map.insert("Tree".into(), 1);
        map.insert("Tree\u{200B}".into(), 2);
        map.insert("Rock\u{200B}".into(), 3);
        map.insert("\u{200B}".into(), 4);
        map.insert("b".repeat(300), 5);
        map.insert("a".repeat(300), 6);
        map.insert("a".repeat(256), 7);
        let renames = sanitize_keys(&mut map, 256);
        assert_eq!(map.get("Tree"), Some(&1), "an untouched key keeps its slot");
        assert_eq!(map.get("Rock"), Some(&3));
        assert_eq!(map.get(&"b".repeat(256)), Some(&5));
        assert_eq!(
            map.get(&"a".repeat(256)),
            Some(&7),
            "the first claimant of a collided key wins, never the last writer"
        );
        assert_eq!(map.len(), 4);
        assert_eq!(
            renames,
            vec![
                ("Rock\u{200B}".to_owned(), "Rock".to_owned()),
                ("b".repeat(300), "b".repeat(256)),
            ]
        );
    }
}
