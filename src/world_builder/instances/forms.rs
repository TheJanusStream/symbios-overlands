//! The forms each variant of a set was baked into (#1643), kept by what
//! they were baked from - the generator's content and the variant - so a
//! set respawned by an edit, and two sets of one generator, grow and bake
//! each variant once.
//!
//! Forms are kept for the room they were baked in: a visit to another room
//! drops them all. Within a room the least recently drawn go once more than
//! [`MAX_FORMS`] are held; a build still drawing from one keeps it alive.

use std::collections::HashMap;
use std::sync::Arc;

use avian3d::prelude::Collider;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;

use super::bake::{Baked, NearPart};
use crate::pds::InstanceVariant;

/// The most forms held at once: well past a Kiez's hundred or so variants.
const MAX_FORMS: usize = 512;

/// One variant baked.
pub(super) struct Form {
    /// One mesh per material and sway; `None` where a part is drawn with a
    /// material that is not a standard one, and each copy is spawned whole.
    pub near: Option<Vec<NearPart<Handle<Mesh>>>>,
    /// Its voxel shell; `None` where no part has the positions and normals
    /// one needs.
    pub far: Option<Handle<Mesh>>,
    /// The box its parts fill, in its frame.
    pub bounds: Option<(Vec3, Vec3)>,
    /// Half the width of its base ([`Baked::base`]).
    pub base: Option<f32>,
    /// Its shell as a collider, where it has one.
    pub shell: Option<Collider>,
}

impl Form {
    /// The forms `baked` holds, their meshes added to `meshes` with no CPU
    /// copy: drawn, never read again. A template that could not be baked
    /// draws nothing.
    pub(super) fn from_baked(baked: Option<Baked>, meshes: &mut Assets<Mesh>) -> Self {
        let Some(baked) = baked else {
            return Form {
                near: None,
                far: None,
                bounds: None,
                base: None,
                shell: None,
            };
        };
        let mut add = |mut mesh: Mesh| {
            mesh.asset_usage = RenderAssetUsages::RENDER_WORLD;
            meshes.add(mesh)
        };
        let shell = baked.far.as_ref().and_then(Collider::trimesh_from_mesh);
        Form {
            near: baked.near.map(|near| {
                near.into_iter()
                    .map(|(mesh, material, sway)| (add(mesh), material, sway))
                    .collect()
            }),
            far: baked.far.map(&mut add),
            bounds: baked.bounds,
            base: baked.base,
            shell,
        }
    }

    /// Whether it draws anything at all.
    pub(super) fn draws(&self) -> bool {
        self.bounds.is_some()
    }
}

/// The key the forms of `variant` of the generator written `tree_json` are
/// kept under.
pub(super) fn form_key(tree_json: &str, variant: &InstanceVariant) -> u64 {
    let mut text = String::with_capacity(tree_json.len() + 64);
    text.push_str(tree_json);
    text.push_str("|seed=");
    if let Some(seed) = variant.seed {
        text.push_str(&seed.to_string());
    }
    for (name, value) in variant.params.wire() {
        text.push('|');
        text.push_str(name);
        text.push('=');
        text.push_str(&value.to_string());
    }
    crate::seeded_defaults::fnv1a_64(&text)
}

/// The forms held, each with when it was last asked for.
#[derive(Default)]
pub(super) struct Forms {
    held: HashMap<u64, (Arc<Form>, u64)>,
    /// Counts every ask, for the least recently drawn.
    clock: u64,
    /// The room they were baked in.
    room: Option<String>,
}

impl Forms {
    /// Drop every form when the room is not the one they were baked in.
    pub(super) fn keep_for_room(&mut self, room: Option<&str>) {
        if self.room.as_deref() != room {
            self.held.clear();
            self.room = room.map(str::to_owned);
        }
    }

    /// The forms kept under `key`, if any.
    pub(super) fn get(&mut self, key: u64) -> Option<Arc<Form>> {
        self.clock += 1;
        let clock = self.clock;
        self.held.get_mut(&key).map(|(form, used)| {
            *used = clock;
            Arc::clone(form)
        })
    }

    /// Keep `form` under `key`, in place of any there, letting the least
    /// recently drawn go past [`MAX_FORMS`].
    pub(super) fn insert(&mut self, key: u64, form: Form) -> Arc<Form> {
        self.clock += 1;
        let form = Arc::new(form);
        self.held.insert(key, (Arc::clone(&form), self.clock));
        while self.held.len() > MAX_FORMS {
            let Some(oldest) = self
                .held
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(key, _)| *key)
            else {
                break;
            };
            self.held.remove(&oldest);
        }
        form
    }

    /// How many forms are held.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.held.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Fp, GrammarParams};
    use std::collections::BTreeMap;

    fn nothing() -> Form {
        Form::from_baked(None, &mut Assets::<Mesh>::default())
    }

    #[test]
    fn a_variant_keys_by_its_seed_and_values_and_the_generator() {
        let plain = InstanceVariant::default();
        let seeded = InstanceVariant {
            seed: Some(3),
            params: GrammarParams::default(),
        };
        let valued = InstanceVariant {
            seed: Some(3),
            params: GrammarParams(BTreeMap::from([("Storeys".into(), Fp(4.0))])),
        };
        let keys = [
            form_key("{}", &plain),
            form_key("{}", &seeded),
            form_key("{}", &valued),
            form_key("{\"a\":1}", &valued),
        ];
        for (i, a) in keys.iter().enumerate() {
            for b in &keys[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(form_key("{}", &valued), form_key("{}", &valued.clone()));
    }

    #[test]
    fn the_least_recently_drawn_go_first_and_another_room_drops_all() {
        let mut forms = Forms::default();
        forms.keep_for_room(Some("did:plc:a"));
        for key in 0..MAX_FORMS as u64 {
            forms.insert(key, nothing());
        }
        // Key 0 is drawn again, so key 1 is now the least recent.
        assert!(forms.get(0).is_some());
        forms.insert(9_999, nothing());
        assert_eq!(forms.len(), MAX_FORMS);
        assert!(forms.get(0).is_some());
        assert!(forms.get(1).is_none());
        forms.keep_for_room(Some("did:plc:a"));
        assert_eq!(forms.len(), MAX_FORMS);
        forms.keep_for_room(Some("did:plc:b"));
        assert_eq!(forms.len(), 0);
    }
}
