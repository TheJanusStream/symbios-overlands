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

use crate::config::network::MAX_RELIABLE_PAYLOAD_BYTES;
use crate::pds::record_size::{HARD_RECORD_CEILING_BYTES, SOFT_RECORD_BUDGET_BYTES, SizeReadout};
use crate::pds::{AvatarRecord, InventoryRecord, RoomRecord};
use crate::protocol::OverlandsMessage;

/// What the world's live edits weigh on the way to the people in it, and
/// whether they get there (#1500).
///
/// A second ceiling, beside the per-record budget the readouts above weigh:
/// every live edit goes to the people in the world as the WHOLE record in one
/// message, and past [`MAX_RELIABLE_PAYLOAD_BYTES`] the game refuses to send
/// it (#1123) - in its log and a toast in its own window, where an agent
/// looks for neither. So an agent that offered the owner a change "live"
/// in Ashmere (1.3 MiB) was offering one nobody could see. `refused` says so
/// in the answer itself, and names what does reach them: a save, which each
/// of them fetches (#1499).
///
/// `bytes` is the quantity the game decides on - the message as the wire
/// carries it - and is `None` for a record that will not serialise.
pub(super) fn live_sync(record: &RoomRecord) -> Value {
    let bytes = OverlandsMessage::room_state_update(record)
        .and_then(|message| crate::network::chunk::wire_payload_bytes(&message));
    let mut answer = json!({
        "bytes": bytes,
        "ceiling_bytes": MAX_RELIABLE_PAYLOAD_BYTES,
    });
    match bytes {
        Some(bytes) if bytes <= MAX_RELIABLE_PAYLOAD_BYTES => {}
        Some(_) => {
            answer["refused"] = json!(
                "past the ceiling: no live edit reaches anyone in the world. A \
                 save does - everyone in it fetches the saved world - so show a \
                 change by saving it"
            );
        }
        None => {
            answer["refused"] = json!(
                "the record will not serialise, so no live edit is sent: it \
                 holds a part this build cannot write back"
            );
        }
    }
    answer
}

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

    /// #1500: a world past the live ceiling is said to reach nobody live,
    /// with the measure and the ceiling, and what does reach them, a save,
    /// is named. One under it is given its weight and nothing more.
    #[test]
    fn a_world_past_the_live_ceiling_is_said_to_reach_nobody_live() {
        let light = live_sync(&RoomRecord::default_for_did("did:plc:sizes"));
        assert!(light.get("refused").is_none(), "{light}");
        assert!(
            light["bytes"]
                .as_u64()
                .is_some_and(|bytes| bytes as usize <= MAX_RELIABLE_PAYLOAD_BYTES),
            "{light}"
        );

        let heavy = live_sync(&super::super::harness::past_the_live_ceiling(
            "did:plc:sizes",
        ));
        assert!(
            heavy["bytes"]
                .as_u64()
                .is_some_and(|bytes| bytes as usize > MAX_RELIABLE_PAYLOAD_BYTES),
            "{heavy}"
        );
        assert_eq!(heavy["ceiling_bytes"], MAX_RELIABLE_PAYLOAD_BYTES);
        assert!(
            heavy["refused"]
                .as_str()
                .is_some_and(|why| why.contains("no live edit") && why.contains("sav")),
            "{heavy}"
        );
    }
}
