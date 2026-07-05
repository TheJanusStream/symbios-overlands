//! Record-size budget (Stage 0 of the single-record-boundary plan): measure
//! every record's serialized `putRecord` payload, warn past a soft budget,
//! and refuse to publish past a hard ceiling.
//!
//! ATProto records are single DAG-CBOR blocks in the owner's repo; the
//! relay/sync layer rejects blocks around 1 MiB, and PDS implementations
//! additionally impose their own (often lower) XRPC JSON body caps on
//! `putRecord` / `applyWrites`. The soft budget is the design target every
//! record should stay under so we never get near either limit; the hard
//! ceiling refuses a publish outright *before any network I/O*. The
//! pre-flight refusal matters beyond UX: the 5xx recovery path in
//! [`super::publish_room_record`] / [`super::publish_avatar_record`] deletes
//! the stored record and re-puts it — an oversized record that failed the
//! re-put would leave the owner with *no* record at all. Refusing up front
//! makes that data-loss sequence unreachable.
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
