//! The avatar's round trip through the Inventory (#1838): Save to
//! Inventory copies the live avatar into an [`AvatarItem`], and Wear (from
//! the Inventory window's row) puts one back on.
//!
//! Both halves act on the LIVE records only. Save to Inventory inserts into
//! `LiveInventoryRecord` and the Inventory window's Save writes it, as the
//! World Editor's "Save to Inventory" does; Wear replaces
//! `LiveAvatarRecord` in one undo step and stays unsaved until the Avatar
//! editor's Save publishes the bundle.

use crate::pds::avatar::EngineAvatarRecord;
use crate::pds::inventory::AvatarItem;
use crate::pds::{AvatarRecord, InventoryRecord};

/// Why the live avatar cannot be saved to the Inventory right now, or
/// `None` when it can: the stash has not loaded, it is full, or the body
/// has nothing to copy or cannot be written back. Asked every frame the
/// Avatar editor draws its footer, so it reads the record and never
/// copies it.
pub(crate) fn save_to_inventory_blocked_reason(
    record: &AvatarRecord,
    inventory: Option<&InventoryRecord>,
) -> Option<String> {
    let Some(inventory) = inventory else {
        return Some(String::from("Your inventory has not loaded yet."));
    };
    if inventory.is_full() {
        let cap = crate::config::state::MAX_INVENTORY_ITEMS;
        return Some(format!(
            "Inventory full ({cap}/{cap}) - remove an item first."
        ));
    }
    AvatarItem::copy_refused(record)
}

/// Copy `record` - the live avatar, unsaved edits included - into
/// `inventory` under a name no item of any kind holds. Returns the name it
/// landed under, or the sentence saying why it did not.
pub(crate) fn save_avatar_to_inventory(
    record: &AvatarRecord,
    inventory: &mut InventoryRecord,
) -> Result<String, String> {
    save_avatar_to_inventory_as(record, None, inventory)
}

/// [`save_avatar_to_inventory`], under a free name built on `name` when one
/// is given - the Catalogue's Copy to Inventory (#1840) names the copy for
/// its entry, where the record's own default would call every person
/// "Wanderer". The item is the same copy either way.
pub(crate) fn save_avatar_to_inventory_as(
    record: &AvatarRecord,
    name: Option<&str>,
    inventory: &mut InventoryRecord,
) -> Result<String, String> {
    if let Some(reason) = save_to_inventory_blocked_reason(record, Some(inventory)) {
        return Err(reason);
    }
    let item = AvatarItem::from_record(record)?;
    let name = inventory.free_name(&name.map_or_else(|| item.default_name(), str::to_owned));
    if !inventory.put_avatar(name.clone(), item) {
        // Unreachable: `free_name` asked every map.
        return Err(format!("\"{name}\" is already taken."));
    }
    Ok(name)
}

/// The wardrobe key a worn item's engine record goes to (#1838).
///
/// The wardrobe keeps every body it is given, so a Wear must not grow it
/// with a copy of a body it already holds - but it may reuse a key only
/// where this client KNOWS the record there is equal: the stored avatar's
/// resolved body, or an entry of a fetched wardrobe listing. Anywhere else
/// the key is minted fresh, because writing the item's body over a key
/// whose record is something else would overwrite a saved self.
pub(crate) fn wardrobe_rkey_for(
    body: &EngineAvatarRecord,
    stored: Option<&AvatarRecord>,
    listing: Option<&[(String, EngineAvatarRecord)]>,
    did: &str,
) -> String {
    let stored_rig = stored.and_then(|stored| stored.body.rigged_ref());
    if let Some(rig) = stored_rig
        && rig
            .resolved
            .as_ref()
            .is_some_and(|resolved| &resolved.body == body)
    {
        return rig.avatar.clone();
    }
    // The stored avatar is the fresher word on ITS key: a listing fetched
    // before the owner sculpted and saved still holds the old body there,
    // and wearing an item equal to that old body at that key would write it
    // over the body they saved since (critic of #1838). A successful save
    // also refreshes the listing's entry
    // ([`super::AvatarEditorState::note_published_body`]).
    if let Some((rkey, _)) = listing
        .unwrap_or_default()
        .iter()
        .find(|(rkey, listed)| listed == body && stored_rig.is_none_or(|rig| rig.avatar != *rkey))
    {
        return rkey.clone();
    }
    crate::pds::tid::tid_now(crate::seeded_defaults::fnv1a_64(did))
}

/// The live avatar record that wears `item` (#1838), or `None` for an item
/// this build cannot read. A rigged body is resolved locally - nothing is
/// fetched - and every prop gets a fresh attachment key, so the next avatar
/// save publishes the bundle and, by the derived delete rule, retires the
/// outfit being taken off.
pub(crate) fn wear_avatar_item(
    item: &AvatarItem,
    stored: Option<&AvatarRecord>,
    listing: Option<&[(String, EngineAvatarRecord)]>,
    did: &str,
) -> Option<AvatarRecord> {
    let entropy = crate::seeded_defaults::fnv1a_64(did);
    item.wear(
        |body| wardrobe_rkey_for(body, stored, listing, did),
        || crate::pds::tid::tid_now(entropy),
    )
}

/// Put `item` on the live avatar in ONE write - one change tick, one undo
/// step labelled `wear {name}` - unsaved until the Avatar editor's Save
/// (#1838). The Inventory's avatar row and the Catalogue's avatar entries
/// (#1840) both wear through here, so the refusals, the already-worn check
/// and the editor reset have one home.
///
/// `live` is a `Mut`, not a `&mut` (critic of #1838): every refusal reads it
/// through `Deref`, so only a wear that happens stamps the change tick.
/// Each refusal says why in a toast (#1141), though the callers' Wear
/// buttons are disabled for the ones they can foresee - and returns the
/// sentence as its `Err`, the toast's reason, for a caller with no screen
/// to show it on (the agent client, #1843).
#[allow(clippy::too_many_arguments)]
pub(crate) fn wear_avatar_on_live(
    item: &AvatarItem,
    name: &str,
    mut live: bevy::prelude::Mut<'_, crate::state::LiveAvatarRecord>,
    stored: Option<&AvatarRecord>,
    avatar_editor: &mut super::AvatarEditorState,
    did: &str,
    undo_labels: &mut crate::ui::undo::PendingUndoLabels,
    toasts: &mut crate::notify::Toasts,
    now: f64,
) -> Result<(), String> {
    if let Some(reason) = wear_avatar_blocked_reason(Some(&live.0), item) {
        toasts.warn(format!("Could not wear \"{name}\" - {reason}"), now);
        return Err(reason.to_owned());
    }
    // Asked of the copy, on the click only: wearing what is already on
    // would mint fresh keys for an identical outfit.
    if AvatarItem::from_record(&live.0).as_ref() == Ok(item) {
        let already = format!("You are already wearing \"{name}\".");
        toasts.info(already.clone(), now);
        return Err(already);
    }
    let Some(record) = wear_avatar_item(item, stored, avatar_editor.wardrobe_entries(), did) else {
        const UNREADABLE: &str = "this build cannot read it.";
        toasts.warn(format!("Could not wear \"{name}\" - {UNREADABLE}"), now);
        return Err(UNREADABLE.to_owned());
    };
    // The whole body is replaced: nothing the gizmo or the parts editor
    // was aimed at survives it.
    avatar_editor.clear_gizmo_selections();
    avatar_editor.close_parts_editor();
    // ONE write, so ONE change tick and one undo step.
    live.0 = record;
    undo_labels.set_avatar(format!("wear {name}"));
    toasts.success(
        format!("Wearing \"{name}\" - Save in the Avatar window to keep it."),
        now,
    );
    Ok(())
}

/// Why the selected avatar item cannot be worn, or `None` when it can.
pub(crate) fn wear_avatar_blocked_reason(
    live: Option<&AvatarRecord>,
    item: &AvatarItem,
) -> Option<&'static str> {
    if live.is_none() {
        return Some("No avatar loaded yet.");
    }
    if !item.is_readable() {
        return Some("This avatar was made by a newer version of Overlands.");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::Generator;
    use crate::pds::inventory::avatar_item_tests::{generator_record, rigged_record};

    const DID: &str = "did:plc:stash";

    /// Save to Inventory copies the avatar the owner SEES - the live
    /// record with its unsaved sculpt - not the stored one, and names it
    /// for what it is, free across every kind of item.
    #[test]
    fn save_to_inventory_copies_the_live_avatar_with_its_unsaved_edit() {
        let stored = rigged_record();
        let mut live = stored.clone();
        if let Some(resolved) = live.body.rigged_mut().and_then(|rig| rig.resolved.as_mut()) {
            resolved.body.composites.femininity = 0.5;
            resolved.body.name = String::from("Sunday best");
        }
        live.sanitize();
        let mut inventory = InventoryRecord::default();
        inventory.put_item(
            String::from("Sunday best"),
            Generator::default_cuboid(),
            None,
        );

        let name = save_avatar_to_inventory(&live, &mut inventory).expect("saves");
        assert_eq!(name, "Sunday best_1", "free across kinds");
        let item = &inventory.avatars[&name];
        let live_body = &live
            .body
            .rigged_ref()
            .unwrap()
            .resolved
            .as_ref()
            .unwrap()
            .body;
        let stored_body = &stored
            .body
            .rigged_ref()
            .unwrap()
            .resolved
            .as_ref()
            .unwrap()
            .body;
        match &item.body {
            crate::pds::inventory::AvatarItemBody::Rigged(rigged) => {
                assert_eq!(&rigged.record, live_body, "the live body");
                assert_ne!(&rigged.record, stored_body, "not the saved one");
                assert_eq!(rigged.wearables.len(), 1);
            }
            other => panic!("a rigged item, got {other:?}"),
        }
        assert_eq!(item.locomotion, live.locomotion);

        // A vehicle is named for its kind.
        let name = save_avatar_to_inventory(&generator_record(), &mut inventory).expect("saves");
        assert_eq!(name, inventory.avatars[&name].kind().default_name());
    }

    /// Every reason Save to Inventory is disabled is one this answers.
    #[test]
    fn save_to_inventory_says_why_it_cannot() {
        let live = rigged_record();
        assert!(
            save_to_inventory_blocked_reason(&live, None)
                .is_some_and(|why| why.contains("not loaded"))
        );
        assert_eq!(
            save_to_inventory_blocked_reason(&live, Some(&InventoryRecord::default())),
            None
        );
        let mut unresolved = live.clone();
        unresolved.body.rigged_mut().unwrap().resolved = None;
        assert_eq!(
            save_to_inventory_blocked_reason(&unresolved, Some(&InventoryRecord::default())),
            Some(crate::pds::inventory::avatar::UNRESOLVED_BODY.to_string())
        );
        let mut future = live.clone();
        future.body = crate::pds::avatar::AvatarBody::Unknown;
        assert!(
            save_to_inventory_blocked_reason(&future, Some(&InventoryRecord::default()))
                .is_some_and(|why| why.contains("newer client"))
        );
    }

    /// The cap counts every kind (#1838): a stash full of a MIX of avatars
    /// and generators refuses an avatar and a worn prop alike, and one
    /// slot freed - of either kind - lets either in.
    #[test]
    fn a_full_stash_refuses_an_avatar_and_a_generator_alike() {
        let cap = crate::config::state::MAX_INVENTORY_ITEMS;
        let vehicle = AvatarItem::from_record(&generator_record()).expect("copies");
        let mut inventory = InventoryRecord::default();
        for i in 0..cap {
            if i % 2 == 0 {
                inventory.put_avatar(format!("a{i}"), vehicle.clone());
            } else {
                inventory.put_item(format!("g{i}"), Generator::default_cuboid(), None);
            }
        }
        let prop = crate::pds::avatar::AttachmentRecord::new(
            Generator::default_cuboid(),
            symbios_avatar::Socket::Crown,
        );
        let full = save_avatar_to_inventory(&rigged_record(), &mut inventory)
            .expect_err("a full stash refuses an avatar");
        assert!(full.contains("full"), "{full}");
        let full = crate::ui::avatar::save_worn_to_inventory(&prop, &mut inventory)
            .expect_err("and a generator");
        assert!(full.contains("full"), "{full}");
        assert_eq!(inventory.item_count(), cap);

        inventory.remove_item("a0");
        assert!(crate::ui::avatar::save_worn_to_inventory(&prop, &mut inventory).is_ok());
        inventory.remove_item("g1");
        assert!(save_avatar_to_inventory(&rigged_record(), &mut inventory).is_ok());
    }

    /// Wear puts the item on in one undo step, resolved locally so the
    /// body builds at once; the next save publishes the item's body and
    /// props and retires the old outfit's attachment records.
    #[test]
    fn wearing_a_rigged_item_resolves_at_once_in_one_undo_step_and_saves_the_bundle() {
        use bevy::prelude::*;
        let mut item = AvatarItem::from_record(&rigged_record()).expect("copies");
        // Worn over an old outfit: the stored record is a rigged body
        // wearing a different prop.
        let mut old = rigged_record();
        if let Some(rig) = old.body.rigged_mut() {
            rig.avatar = String::from("3jzfcijpj2z2x");
            rig.attachments = vec![String::from("3jzfcijpj2zzz")];
            rig.resolved.as_mut().unwrap().attachments[0].rkey = String::from("3jzfcijpj2zzz");
        }
        if let crate::pds::inventory::AvatarItemBody::Rigged(rigged) = &mut item.body {
            rigged.record.composites.femininity = 0.9;
            rigged.record.sanitize();
        }

        let worn = wear_avatar_item(&item, Some(&old), None, DID).expect("readable");
        let rig = worn.body.rigged_ref().expect("rigged");
        let resolved = rig.resolved.as_ref().expect("resolved without a fetch");
        assert_ne!(
            rig.avatar, "3jzfcijpj2z2x",
            "a different body mints its own key"
        );
        assert_eq!(rig.attachments.len(), 1);
        assert_ne!(
            rig.attachments[0], "3jzfcijpj2zzz",
            "a fresh attachment key"
        );
        assert_eq!(resolved.attachments[0].rkey, rig.attachments[0]);
        assert_eq!(resolved.attachments[0].record.source, None);

        // One undo step, as the app captures it.
        let mut app = App::new();
        app.insert_resource(crate::state::LiveAvatarRecord(old.clone()));
        app.init_resource::<crate::ui::avatar::AvatarEditorState>();
        app.init_resource::<crate::ui::undo::AvatarUndoHistory>();
        app.init_resource::<crate::ui::undo::PendingUndoLabels>();
        app.add_systems(Update, crate::ui::undo::capture_avatar_history);
        app.update();
        let before = app
            .world()
            .resource::<crate::ui::undo::AvatarUndoHistory>()
            .len();
        {
            let world = app.world_mut();
            world
                .resource_mut::<crate::ui::undo::PendingUndoLabels>()
                .set_avatar("wear me");
            world.resource_mut::<crate::state::LiveAvatarRecord>().0 = worn.clone();
        }
        app.update();
        app.update();
        let mut history = app
            .world_mut()
            .resource_mut::<crate::ui::undo::AvatarUndoHistory>();
        assert_eq!(history.len(), before + 1, "one undo step");
        let (entry, _) = history.undo().expect("undoable");
        assert_eq!(entry.record, old, "and one undo takes it off again");

        // The save that follows.
        let plan = crate::pds::avatar::wardrobe::plan_avatar_publish(
            &worn,
            &crate::pds::avatar::wardrobe::attachment_rkeys(&old),
            "2026-11-04T00:00:00Z",
        );
        let (rkey, body) = plan.wardrobe.expect("the body is written");
        assert_eq!(rkey, rig.avatar);
        assert_eq!(body.composites, resolved.body.composites);
        assert_eq!(
            plan.attachments
                .iter()
                .map(|(rkey, _)| rkey.clone())
                .collect::<Vec<_>>(),
            rig.attachments
        );
        assert_eq!(
            plan.attachment_deletes,
            vec![String::from("3jzfcijpj2zzz")],
            "the old outfit's record retires"
        );
    }

    /// The wardrobe does not grow needlessly: an item whose body equals
    /// the stored avatar's, or a fetched listing entry, reuses that key;
    /// anything else mints one.
    #[test]
    fn wearing_over_an_equal_known_body_mints_no_wardrobe_key() {
        let stored = rigged_record();
        let item = AvatarItem::from_record(&stored).expect("copies");
        let worn = wear_avatar_item(&item, Some(&stored), None, DID).expect("readable");
        assert_eq!(
            worn.body.rigged_ref().unwrap().avatar,
            stored.body.rigged_ref().unwrap().avatar,
            "the stored body's key"
        );

        let body = stored
            .body
            .rigged_ref()
            .unwrap()
            .resolved
            .as_ref()
            .unwrap()
            .body
            .clone();
        let listing = vec![(String::from("3jzfcijpj2zl1"), body)];
        let worn = wear_avatar_item(&item, None, Some(&listing), DID).expect("readable");
        assert_eq!(worn.body.rigged_ref().unwrap().avatar, "3jzfcijpj2zl1");

        let worn = wear_avatar_item(&item, Some(&generator_record()), None, DID).expect("readable");
        let minted = &worn.body.rigged_ref().unwrap().avatar;
        assert_ne!(minted, &stored.body.rigged_ref().unwrap().avatar);
        assert_eq!(minted.len(), 13, "a TID");
    }

    /// A listing fetched before the owner sculpted and saved still holds the
    /// old body at the stored key. An item equal to that OLD body must not be
    /// worn at that key - its Save would write the old body over the saved
    /// one (critic of #1838) - and a save keeps a landed listing true.
    #[test]
    fn a_stale_listing_never_puts_an_old_body_back_over_the_saved_one() {
        let before = rigged_record();
        let item = AvatarItem::from_record(&before).expect("copies");
        let rig = before.body.rigged_ref().unwrap();
        let key = rig.avatar.clone();
        let old_body = rig.resolved.as_ref().unwrap().body.clone();
        let listing = vec![(key.clone(), old_body.clone())];

        let mut saved = before.clone();
        let resolved = saved.body.rigged_mut().unwrap().resolved.as_mut().unwrap();
        resolved.body.name = format!("{} sculpted", resolved.body.name);
        let new_body = resolved.body.clone();
        assert_ne!(new_body, old_body);

        let worn = wear_avatar_item(&item, Some(&saved), Some(&listing), DID).expect("readable");
        assert_ne!(
            worn.body.rigged_ref().unwrap().avatar,
            key,
            "the old body would be written over the saved one"
        );

        let mut editor = super::super::AvatarEditorState::default();
        editor.note_published_body(&key, &new_body);
        assert!(
            editor.wardrobe_entries().is_none(),
            "a listing never fetched stays unfetched"
        );
        editor.wardrobe.entries = Some(listing);
        editor.note_published_body(&key, &new_body);
        editor.note_published_body("3jzfcijpj2zl9", &old_body);
        assert_eq!(
            editor.wardrobe_entries().unwrap(),
            &[(key, new_body), (String::from("3jzfcijpj2zl9"), old_body)][..],
            "the saved key holds the saved body; a new key joins as the newest"
        );
    }

    /// A tree whose root is a kind of a newer build makes the whole avatar
    /// unreadable, as it makes a generator item unreadable (critic of
    /// #1838): worn, it would dress the live avatar in what its Save cannot
    /// write.
    #[test]
    fn a_newer_kind_at_a_wearable_or_visuals_root_is_unreadable() {
        let mut person = AvatarItem::from_record(&rigged_record()).expect("copies");
        assert!(person.is_readable());
        if let crate::pds::inventory::avatar::AvatarItemBody::Rigged(rigged) = &mut person.body {
            rigged.wearables[0].generator.kind = crate::pds::GeneratorKind::Unknown;
        }
        assert!(!person.is_readable());
        assert!(wear_avatar_blocked_reason(Some(&rigged_record()), &person).is_some());

        let mut vehicle = AvatarItem::from_record(&generator_record()).expect("copies");
        assert!(vehicle.is_readable());
        if let crate::pds::inventory::avatar::AvatarItemBody::Generator(body) = &mut vehicle.body {
            body.visuals.kind = crate::pds::GeneratorKind::Unknown;
        }
        assert!(!vehicle.is_readable());
    }

    /// A vehicle item wears a generator body, and its locomotion with it.
    #[test]
    fn wearing_a_vehicle_item_wears_its_tree() {
        let vehicle = generator_record();
        let item = AvatarItem::from_record(&vehicle).expect("copies");
        let worn = wear_avatar_item(&item, Some(&rigged_record()), None, DID).expect("readable");
        assert_eq!(worn.body.visuals(), item.visuals());
        assert_eq!(worn.locomotion, vehicle.locomotion);
        assert_eq!(worn.gait, vehicle.gait);
    }
}
