//! Record-size budget (Stage 0 of the single-record-boundary plan): measure
//! every record's serialized wire payload (for split-format rooms, the
//! largest single record the publish writes), warn past a soft budget, and
//! refuse to publish past a hard ceiling.
//!
//! ATProto records are single DAG-CBOR blocks in the owner's repo; the
//! relay/sync layer rejects blocks around 1 MiB, and PDS implementations
//! additionally impose their own (often lower) XRPC JSON body caps on
//! `putRecord` / `applyWrites`. The soft budget is the design target every
//! record should stay under so we never get near either limit; the hard
//! ceiling refuses a publish outright *before any network I/O*. The
//! pre-flight refusal matters beyond UX: the 5xx recovery path in
//! [`super::publish_avatar_record`] deletes the stored record and re-puts
//! it — an oversized record that failed the re-put would leave the owner
//! with *no* record at all. Refusing up front makes that data-loss sequence
//! unreachable.
//!
//! Sizes are measured over the serialized `record` field alone; the
//! surrounding `{repo, collection, rkey}` envelope adds ~100 bytes, which the
//! margin between the hard ceiling and the 1 MiB block limit absorbs.

use serde::Serialize;

/// Design target every record should stay under (100 KiB). Crossing it only
/// warns — the publish still proceeds — but it is the signal to start the
/// later stages of the split plan (default-elision, record sharding).
pub const SOFT_RECORD_BUDGET_BYTES: usize = 100 * 1024;

/// Absolute pre-flight ceiling (900 KiB): past this the publish is refused
/// without touching the network. Leaves margin under the ~1 MiB ATProto
/// block limit for the `putRecord` envelope and PDS-side JSON overhead.
pub const HARD_RECORD_CEILING_BYTES: usize = 900 * 1024;

/// Where a measured record size falls against the two budgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeClass {
    /// At or under [`SOFT_RECORD_BUDGET_BYTES`] — nothing to report.
    WithinBudget,
    /// Over the soft budget but still publishable — warn.
    OverSoftBudget,
    /// Over [`HARD_RECORD_CEILING_BYTES`] — publish is refused.
    OverHardCeiling,
}

/// Classify a measured byte count against the soft/hard budgets.
pub fn classify(bytes: usize) -> SizeClass {
    if bytes > HARD_RECORD_CEILING_BYTES {
        SizeClass::OverHardCeiling
    } else if bytes > SOFT_RECORD_BUDGET_BYTES {
        SizeClass::OverSoftBudget
    } else {
        SizeClass::WithinBudget
    }
}

/// Serialized JSON byte length of `record` — the `record` field of the
/// `putRecord` body. `None` when serialization fails, which no record type
/// can practically hit (plain data structs), but the UI readout must render
/// a dash rather than panic if it ever does.
pub fn serialized_record_bytes<T: Serialize>(record: &T) -> Option<usize> {
    serde_json::to_vec(record).ok().map(|v| v.len())
}

/// Pre-flight guard every record publish path calls before any network I/O.
/// Returns the measured size, or an error when the record is past the hard
/// ceiling (or unserializable). `label` names the record kind in the error
/// so the shared status line stays self-explanatory.
pub fn preflight<T: Serialize>(record: &T, label: &str) -> Result<usize, String> {
    let bytes = serde_json::to_vec(record)
        .map_err(|e| format!("serialize ({label}): {e}"))?
        .len();
    if bytes > HARD_RECORD_CEILING_BYTES {
        return Err(format!(
            "{label} record is {} — past the {} publish ceiling; refusing to send \
             (the PDS would reject it, and the delete-then-put recovery path could \
             delete the stored record without replacing it). Remove content and retry.",
            human_bytes(bytes),
            human_bytes(HARD_RECORD_CEILING_BYTES),
        ));
    }
    Ok(bytes)
}

/// Human-readable byte count for the UI readout and error messages
/// (`842 B`, `12.3 KiB`, `1.1 MiB`).
pub fn human_bytes(bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= MIB {
        format!("{:.1} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.1} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_boundaries() {
        // At the boundary is still inside; one past crosses.
        assert_eq!(classify(0), SizeClass::WithinBudget);
        assert_eq!(classify(SOFT_RECORD_BUDGET_BYTES), SizeClass::WithinBudget);
        assert_eq!(
            classify(SOFT_RECORD_BUDGET_BYTES + 1),
            SizeClass::OverSoftBudget
        );
        assert_eq!(
            classify(HARD_RECORD_CEILING_BYTES),
            SizeClass::OverSoftBudget
        );
        assert_eq!(
            classify(HARD_RECORD_CEILING_BYTES + 1),
            SizeClass::OverHardCeiling
        );
    }

    #[test]
    fn human_bytes_units() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(842), "842 B");
        assert_eq!(human_bytes(1024), "1.0 KiB");
        assert_eq!(human_bytes(100 * 1024), "100.0 KiB");
        assert_eq!(human_bytes(1024 * 1024 + 100 * 1024), "1.1 MiB");
    }

    #[test]
    fn preflight_refuses_past_hard_ceiling() {
        // A record whose JSON body is guaranteed past the ceiling: one long
        // string (JSON adds 2 quote bytes).
        let oversized = "x".repeat(HARD_RECORD_CEILING_BYTES);
        let err = preflight(&oversized, "room").unwrap_err();
        assert!(err.contains("room record is"), "err: {err}");
        assert!(err.contains("publish ceiling"), "err: {err}");

        let fine = "x".repeat(10);
        assert_eq!(preflight(&fine, "room").unwrap(), 12);
    }

    #[test]
    fn measured_size_matches_put_record_payload_field() {
        let record = crate::pds::InventoryRecord::default();
        let bytes = serialized_record_bytes(&record).unwrap();
        assert_eq!(bytes, serde_json::to_vec(&record).unwrap().len());
        assert!(bytes > 0);
    }

    /// Diagnostic breakdown of where a seeded default room record's bytes
    /// live — top-level sections plus per-generator weights. Run with
    /// `cargo test record_size -- --nocapture` when planning size work
    /// (Stage 1+ of the single-record-boundary plan). Asserts nothing
    /// beyond serializability so it never goes stale.
    #[test]
    fn default_room_record_section_breakdown() {
        let room = crate::pds::RoomRecord::default_for_did("did:plc:sizebudgetcanary");
        let value = serde_json::to_value(&room).unwrap();
        let obj = value.as_object().unwrap();
        eprintln!(
            "default room total: {}",
            human_bytes(serde_json::to_vec(&room).unwrap().len())
        );
        // Since #697 the room publishes as manifest + children; this is the
        // largest single record a save would actually write.
        if let Some(max) = crate::pds::room::max_publish_record_bytes(&room) {
            eprintln!(
                "  largest published record (manifest/child): {}",
                human_bytes(max)
            );
        }
        for (key, section) in obj {
            let bytes = serde_json::to_vec(section).unwrap().len();
            eprintln!("  {key}: {}", human_bytes(bytes));
            if key == "generators"
                && let Some(map) = section.as_object()
            {
                for (name, generator) in map {
                    let g = serde_json::to_vec(generator).unwrap().len();
                    eprintln!("    {name}: {}", human_bytes(g));
                }
            }
            if key == "environment"
                && let Some(audio) = section.get("ambient_audio")
            {
                let a = serde_json::to_vec(audio).unwrap().len();
                eprintln!("    ambient_audio: {}", human_bytes(a));
            }
        }
    }

    /// Print the paths where two JSON trees differ — failure diagnostics
    /// for the round-trip test below, where a bare `assert_eq!` would dump
    /// two multi-kilobyte documents.
    fn assert_json_eq(label: &str, actual: &serde_json::Value, expected: &serde_json::Value) {
        fn diff(path: &str, a: &serde_json::Value, b: &serde_json::Value) {
            use serde_json::Value;
            match (a, b) {
                (Value::Object(x), Value::Object(y)) => {
                    for k in x.keys().chain(y.keys()) {
                        let (xa, yb) = (x.get(k), y.get(k));
                        if xa != yb {
                            match (xa, yb) {
                                (Some(va), Some(vb)) => diff(&format!("{path}.{k}"), va, vb),
                                _ => eprintln!("DIFF {path}.{k}: {xa:?} vs {yb:?}"),
                            }
                        }
                    }
                }
                (Value::Array(x), Value::Array(y)) => {
                    for (i, (va, vb)) in x.iter().zip(y).enumerate() {
                        if va != vb {
                            diff(&format!("{path}[{i}]"), va, vb);
                        }
                    }
                    if x.len() != y.len() {
                        eprintln!("DIFF {path}: len {} vs {}", x.len(), y.len());
                    }
                }
                _ => eprintln!("DIFF {path}: {a} vs {b}"),
            }
        }
        if actual != expected {
            diff(label, actual, expected);
            panic!("{label}: JSON trees differ (see DIFF lines above)");
        }
    }

    /// Round-trip exactness of the default-eliding wire format (#695): for a
    /// spread of seeds, serialize → deserialize → serialize again must be
    /// byte-identical. A mismatch means some struct's skip predicate compares
    /// against a different default than its deserializer fills in — exactly
    /// the bug class elision can introduce (it caught the
    /// `procedural_texture` legacy-default divergence during development).
    ///
    /// The sanitize leg asserts *fixpoint* stability rather than strict
    /// neutrality: `sanitize()` re-normalizes fixed-point quaternions, so a
    /// first pass may nudge a rotation's last digit (pre-existing
    /// quantization behaviour, unrelated to elision) — but sanitizing the
    /// already-sanitized wire form must change nothing, or every fetch →
    /// republish cycle would keep drifting the record.
    #[test]
    fn eliding_serialization_round_trips_seeded_records() {
        for seed in [0u64, 1, 42, 0xDEAD_BEEF, u64::MAX] {
            let did = format!("did:plc:roundtrip{seed}");
            let room = crate::pds::RoomRecord::default_for_seed(seed, &did);
            let wire = serde_json::to_value(&room).unwrap();
            let mut decoded: crate::pds::RoomRecord = serde_json::from_value(wire.clone()).unwrap();
            assert_json_eq(
                &format!("room[seed {seed}] reserialized"),
                &serde_json::to_value(&decoded).unwrap(),
                &wire,
            );
            decoded.sanitize();
            let once = serde_json::to_value(&decoded).unwrap();
            let mut again: crate::pds::RoomRecord = serde_json::from_value(once.clone()).unwrap();
            again.sanitize();
            assert_json_eq(
                &format!("room[seed {seed}] sanitize fixpoint"),
                &serde_json::to_value(&again).unwrap(),
                &once,
            );

            let avatar = crate::pds::AvatarRecord::default_for_seed(seed);
            let wire = serde_json::to_value(&avatar).unwrap();
            let mut decoded: crate::pds::AvatarRecord =
                serde_json::from_value(wire.clone()).unwrap();
            assert_json_eq(
                &format!("avatar[seed {seed}] reserialized"),
                &serde_json::to_value(&decoded).unwrap(),
                &wire,
            );
            decoded.sanitize();
            let once = serde_json::to_value(&decoded).unwrap();
            let mut again: crate::pds::AvatarRecord = serde_json::from_value(once.clone()).unwrap();
            again.sanitize();
            assert_json_eq(
                &format!("avatar[seed {seed}] sanitize fixpoint"),
                &serde_json::to_value(&again).unwrap(),
                &once,
            );
        }
    }

    /// Legacy compatibility: a fully-explicit (pre-elision) record must
    /// decode to the same value an elided one does. Serializes the default
    /// room via the OLD all-fields shape (reconstructed by merging the
    /// elided output over each struct's serialized defaults is impractical
    /// here, so this exercises the core primitive instead: an explicit
    /// default-valued field decodes identically to an absent one).
    #[test]
    fn explicit_default_fields_decode_like_absent_ones() {
        use crate::pds::TortureParams;
        let absent: TortureParams = serde_json::from_str("{}").unwrap();
        let explicit: TortureParams = serde_json::from_str(
            r#"{"twist":0,"taper":[0,0],"taper_bottom":[0,0],"bend":[0,0,0],
                "s_bend":[0,0],"shear":[0,0],"bulge":[0,0],
                "path_cut":[0,10000],"profile_cut":[0,10000],"hollow":0}"#,
        )
        .unwrap();
        assert_eq!(absent, explicit);
        assert!(absent.is_default());
        // And the elided output of a default really is empty.
        assert_eq!(serde_json::to_string(&absent).unwrap(), "{}");
    }

    /// Canary: the DID-seeded default records must sit comfortably under the
    /// soft budget. If a seeded-defaults change trips this, the budget is
    /// being spent before the owner has authored anything — revisit either
    /// the default build or the budget before shipping.
    #[test]
    fn seeded_default_records_fit_the_soft_budget() {
        let did = "did:plc:sizebudgetcanary";
        let room = crate::pds::RoomRecord::default_for_did(did);
        let avatar = crate::pds::AvatarRecord::default_for_did(did);
        let inventory = crate::pds::InventoryRecord::default();
        for (label, bytes) in [
            ("room", serialized_record_bytes(&room).unwrap()),
            ("avatar", serialized_record_bytes(&avatar).unwrap()),
            ("inventory", serialized_record_bytes(&inventory).unwrap()),
        ] {
            // Baseline visibility under `--nocapture`.
            eprintln!("seeded default {label} record: {}", human_bytes(bytes));
            assert_eq!(
                classify(bytes),
                SizeClass::WithinBudget,
                "seeded default {label} record is {} — over the {} soft budget",
                human_bytes(bytes),
                human_bytes(SOFT_RECORD_BUDGET_BYTES),
            );
        }
    }
}
