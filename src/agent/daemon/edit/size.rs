//! What a save of each record would write, against the budget (#1455).
//!
//! A world is not saved as one record: it is a manifest (placements,
//! environment, landing) and one record per generator, and the budget is on
//! each (`record_size::SOFT_RECORD_BUDGET_BYTES`, 100 KiB). An agent that
//! measured the whole assembled world against it - as the game itself once
//! did (#1027) - rationed a world that was nowhere near its limit. So the
//! answers name the largest record a save would write and its size, in the
//! words the game's own refusal would use (`room generator "mother_tree"`).

use serde_json::{Value, json};

use crate::pds::record_size::{HARD_RECORD_CEILING_BYTES, SOFT_RECORD_BUDGET_BYTES, SizeReadout};
use crate::pds::{AvatarRecord, InventoryRecord, RoomRecord};

/// The world's largest record, as a save would write it.
pub(super) fn room(record: &RoomRecord) -> Value {
    readout(crate::pds::room::measure_publish(record))
}

/// The avatar's largest record, as a save would write it.
pub(super) fn avatar(record: &AvatarRecord) -> Value {
    readout(crate::pds::avatar::wardrobe::measure_publish(record))
}

/// The inventory's largest record, as a save would write it.
pub(super) fn inventory(record: &InventoryRecord) -> Value {
    readout(crate::pds::inventory::measure_publish(record))
}

fn readout(size: SizeReadout) -> Value {
    let mut answer = json!({
        "largest": size.largest,
        "bytes": size.bytes,
        "budget_bytes": SOFT_RECORD_BUDGET_BYTES,
    });
    if let Some(bytes) = size.bytes {
        if bytes > HARD_RECORD_CEILING_BYTES {
            answer["over"] = json!("the ceiling: a save is refused");
        } else if bytes > SOFT_RECORD_BUDGET_BYTES {
            answer["over"] = json!("the budget: a save still goes through, but cut it down");
        }
    }
    if let Some(reason) = size.unserializable {
        answer["unwritable"] = json!(reason);
    }
    answer
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::Generator;

    /// THE CASE THAT ASKED FOR THIS (#1455): the agent weighed its whole
    /// world, 91 KB, against the 100 KiB budget and started rationing - but a
    /// world saves as a manifest and a record per generator, and its largest
    /// was a third of that. The answer names the largest record and its size.
    #[test]
    fn a_world_is_weighed_by_its_largest_record_not_its_whole() {
        let mut record = RoomRecord::default_for_did("did:plc:sizes");
        let big: Generator = serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.gen.cuboid", "size": [10000, 10000, 10000], "solid": true,
            "children": (0..200).map(|i| serde_json::json!({
                "$type": "network.symbios.gen.cuboid", "size": [1000, 1000, 1000], "solid": false,
                "transform": {"translation": [i * 1000, 0, 0]}
            })).collect::<Vec<_>>()
        }))
        .expect("wire JSON");
        record.generators.insert("wall_of_boxes".into(), big);
        let whole = serde_json::to_vec(&record).expect("serialises").len();

        let size = room(&record);

        assert_eq!(
            size["largest"], "room generator \"wall_of_boxes\"",
            "{size}"
        );
        let bytes = size["bytes"].as_u64().expect("measured") as usize;
        assert!(
            bytes < whole,
            "{bytes} is one record of the {whole}-byte whole"
        );
        assert_eq!(size["budget_bytes"], SOFT_RECORD_BUDGET_BYTES);
        assert!(size.get("over").is_none(), "{size}");
    }

    /// Past the budget the answer says so, and what it means for a save.
    #[test]
    fn a_record_past_the_budget_says_what_a_save_will_do() {
        let over = readout(SizeReadout {
            bytes: Some(SOFT_RECORD_BUDGET_BYTES + 1),
            largest: Some("room manifest".into()),
            unserializable: None,
        });
        assert!(
            over["over"]
                .as_str()
                .is_some_and(|s| s.starts_with("the budget")),
            "{over}"
        );
        let refused = readout(SizeReadout {
            bytes: Some(HARD_RECORD_CEILING_BYTES + 1),
            largest: Some("room manifest".into()),
            unserializable: None,
        });
        assert!(
            refused["over"]
                .as_str()
                .is_some_and(|s| s.contains("refused")),
            "{refused}"
        );
    }
}
