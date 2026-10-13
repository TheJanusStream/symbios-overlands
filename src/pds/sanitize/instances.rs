//! Instance sets' sanitiser (#1643): every set bounded - its anchor, its
//! variants and their grammar values, its copies, and the copies of all the
//! room's sets together - and an absolute placement's grammar values with
//! them. Bounds, never a refusal: a set that holds too much keeps what fits,
//! in the record's order, so every peer keeps the same.

use super::{Sanitize, limits};
use crate::pds::instances::{GrammarParams, InstanceCopy, InstanceSolid, InstanceVariant};
use crate::pds::{Fp, Fp3, Placement};

/// Whether `name` may name a grammar value: a grammar identifier - a
/// letter or `_`, then letters, digits and `_` - of at most
/// [`limits::MAX_GRAMMAR_PARAM_NAME_CHARS`].
pub(crate) fn is_param_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.len() <= limits::MAX_GRAMMAR_PARAM_NAME_CHARS
}

impl Sanitize for GrammarParams {
    /// Names that are no grammar identifier dropped, the first
    /// [`limits::MAX_GRAMMAR_PARAMS`] by name kept, and each value finite
    /// within [`limits::MAX_GRAMMAR_PARAM_VALUE`] either way.
    fn sanitize(&mut self) {
        self.0.retain(|name, _| is_param_name(name));
        while self.0.len() > limits::MAX_GRAMMAR_PARAMS {
            self.0.pop_last();
        }
        let bound = limits::MAX_GRAMMAR_PARAM_VALUE;
        for value in self.0.values_mut() {
            *value = Fp(if value.0.is_finite() {
                value.0.clamp(-bound, bound)
            } else {
                0.0
            });
        }
    }
}

impl Sanitize for InstanceCopy {
    /// Within the anchor's 10 km, turned within a circle, and sized within
    /// a thousandth and a thousand times its generator.
    fn sanitize(&mut self) {
        let offset = limits::MAX_INSTANCE_OFFSET_CM;
        self.x_cm = self.x_cm.clamp(-offset, offset);
        self.z_cm = self.z_cm.clamp(-offset, offset);
        self.y_cm = self.y_cm.clamp(-offset, offset);
        self.yaw_ddeg = self.yaw_ddeg.rem_euclid(3_600);
        self.scale_pm = self.scale_pm.clamp(1, limits::MAX_INSTANCE_SCALE_PM);
    }
}

/// Bound every instance set of `placements`, and the grammar values of
/// every absolute placement (see the module docs).
pub(crate) fn sanitize_placements(placements: &mut [Placement]) {
    let mut room_copies = 0usize;
    for placement in placements.iter_mut() {
        match placement {
            Placement::Absolute { params, .. } => params.sanitize(),
            Placement::Instances {
                transform,
                solid,
                variants,
                copies,
                ..
            } => {
                transform.sanitize();
                // A set's anchor is drawn at its own size, as a grid's is
                // (each copy has its own): a scale the record held would
                // say what is not drawn.
                transform.scale = Fp3([1.0; 3]);
                sanitize_solid(solid);
                sanitize_variants(variants);
                let kept = limits::MAX_INSTANCE_COPIES
                    .min(limits::MAX_ROOM_INSTANCE_COPIES.saturating_sub(room_copies));
                sanitize_copies(copies, variants.len().max(1), kept);
                room_copies += copies.len();
            }
            Placement::Scatter { .. } | Placement::Grid { .. } | Placement::Unknown => {}
        }
    }
}

/// A word this build does not know kept to be written back, unless it is
/// too long to be one.
fn sanitize_solid(solid: &mut InstanceSolid) {
    if let InstanceSolid::Other(word) = solid
        && word.chars().count() > limits::MAX_INSTANCE_SOLID_CHARS
    {
        *solid = InstanceSolid::Shell;
    }
}

/// The first [`limits::MAX_INSTANCE_VARIANTS`], each with its grammar
/// values bounded.
fn sanitize_variants(variants: &mut Vec<InstanceVariant>) {
    variants.truncate(limits::MAX_INSTANCE_VARIANTS);
    for variant in variants.iter_mut() {
        variant.params.sanitize();
    }
}

/// Every copy bounded, those of a variant the set does not hold dropped,
/// and at most `kept` of the rest kept, in order.
fn sanitize_copies(copies: &mut Vec<InstanceCopy>, variants: usize, kept: usize) {
    copies.retain(|copy| usize::try_from(copy.variant).is_ok_and(|v| v < variants));
    copies.truncate(kept);
    for copy in copies.iter_mut() {
        copy.sanitize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::instances::COPY_SCALE_ONE;
    use crate::pds::{RoomRecord, TransformData};
    use std::collections::BTreeMap;

    fn set(variants: Vec<InstanceVariant>, copies: Vec<InstanceCopy>) -> Placement {
        Placement::Instances {
            generator_ref: "house".into(),
            transform: TransformData::default(),
            snap_to_terrain: true,
            solid: InstanceSolid::Shell,
            variants,
            copies,
        }
    }

    fn copies_of(placement: &Placement) -> &[InstanceCopy] {
        match placement {
            Placement::Instances { copies, .. } => copies,
            _ => panic!("a set"),
        }
    }

    #[test]
    fn a_name_is_a_grammar_identifier() {
        for good in ["Storeys", "_x", "Floor2", "a"] {
            assert!(is_param_name(good), "{good}");
        }
        let long = "x".repeat(limits::MAX_GRAMMAR_PARAM_NAME_CHARS + 1);
        for bad in ["", "2nd", "scope.x", "a-b", "Ünter", long.as_str()] {
            assert!(!is_param_name(bad), "{bad}");
        }
    }

    #[test]
    fn grammar_values_keep_their_good_names_and_bounded_values() {
        let mut values: BTreeMap<String, Fp> = (0..limits::MAX_GRAMMAR_PARAMS + 4)
            .map(|i| (format!("V{i:02}"), Fp(1.0)))
            .collect();
        values.insert("bad name".into(), Fp(1.0));
        values.insert("Big".into(), Fp(1.0e9));
        values.insert("Nan".into(), Fp(f32::NAN));
        let mut params = GrammarParams(values);
        params.sanitize();
        assert_eq!(params.0.len(), limits::MAX_GRAMMAR_PARAMS);
        assert!(!params.0.contains_key("bad name"));
        assert_eq!(params.0["Big"], Fp(limits::MAX_GRAMMAR_PARAM_VALUE));
        assert_eq!(params.0["Nan"], Fp(0.0));
    }

    /// A copy of a variant the set does not hold is dropped, every other
    /// bounded; a set with no variants draws variant 0 alone.
    #[test]
    fn a_copy_stands_within_bounds_on_a_variant_the_set_holds() {
        let wild = InstanceCopy {
            x_cm: i32::MAX,
            z_cm: i32::MIN,
            yaw_ddeg: -900,
            variant: 1,
            scale_pm: 0,
            y_cm: 5,
        };
        let mut placements = vec![
            set(Vec::new(), vec![InstanceCopy::at(1.0, 2.0, 3.0), wild]),
            set(vec![InstanceVariant::default(); 2], vec![wild]),
        ];
        if let Placement::Instances { transform, .. } = &mut placements[0] {
            transform.scale = Fp3([2.0, 3.0, 4.0]);
        }
        sanitize_placements(&mut placements);
        assert!(
            matches!(&placements[0], Placement::Instances { transform, .. } if transform.scale == Fp3([1.0; 3])),
            "an anchor is drawn at its own size"
        );
        assert_eq!(copies_of(&placements[0]), [InstanceCopy::at(1.0, 2.0, 3.0)]);
        assert_eq!(
            copies_of(&placements[1]),
            [InstanceCopy {
                x_cm: limits::MAX_INSTANCE_OFFSET_CM,
                z_cm: -limits::MAX_INSTANCE_OFFSET_CM,
                yaw_ddeg: 2_700,
                variant: 1,
                scale_pm: 1,
                y_cm: 5,
            }]
        );
    }

    /// The room's copies are counted across its sets in order: the set that
    /// passes the room's bound keeps what fits, the sets after it none.
    #[test]
    fn the_rooms_copies_are_bounded_across_its_sets_in_order() {
        let many = |n: usize| {
            set(
                Vec::new(),
                vec![
                    InstanceCopy {
                        scale_pm: COPY_SCALE_ONE,
                        ..InstanceCopy::at(0.0, 0.0, 0.0)
                    };
                    n
                ],
            )
        };
        let mut placements = vec![
            many(limits::MAX_INSTANCE_COPIES + 10),
            many(limits::MAX_INSTANCE_COPIES),
            many(10),
        ];
        sanitize_placements(&mut placements);
        let kept: Vec<usize> = placements.iter().map(|p| copies_of(p).len()).collect();
        assert_eq!(
            kept,
            [
                limits::MAX_INSTANCE_COPIES,
                limits::MAX_ROOM_INSTANCE_COPIES - limits::MAX_INSTANCE_COPIES,
                0
            ]
        );
    }

    #[test]
    fn a_solid_word_too_long_to_be_one_is_a_shell() {
        let mut placements = vec![Placement::Instances {
            generator_ref: "house".into(),
            transform: TransformData::default(),
            snap_to_terrain: true,
            solid: InstanceSolid::Other("w".repeat(500)),
            variants: Vec::new(),
            copies: Vec::new(),
        }];
        sanitize_placements(&mut placements);
        assert!(matches!(
            &placements[0],
            Placement::Instances {
                solid: InstanceSolid::Shell,
                ..
            }
        ));
        let mut kept = vec![Placement::Instances {
            generator_ref: "house".into(),
            transform: TransformData::default(),
            snap_to_terrain: true,
            solid: InstanceSolid::Other("hull".into()),
            variants: Vec::new(),
            copies: Vec::new(),
        }];
        sanitize_placements(&mut kept);
        assert!(matches!(
            &kept[0],
            Placement::Instances { solid: InstanceSolid::Other(word), .. } if word == "hull"
        ));
    }

    /// Through the record's own sanitiser: a set of a terrain is dropped as
    /// a scatter of one is, and a set's copies are bounded.
    #[test]
    fn the_record_sanitises_its_sets() {
        let mut record = RoomRecord::default_for_seed(7, "did:plc:instances");
        let terrain = crate::pds::find_terrain_config(&record).map(|_| {
            let mut names: Vec<&String> = record.generators.keys().collect();
            names.sort();
            names
                .into_iter()
                .find(|name| {
                    matches!(
                        record.generators[*name].kind,
                        crate::pds::GeneratorKind::Terrain(_)
                    )
                })
                .cloned()
                .expect("a terrain")
        });
        let terrain = terrain.expect("a terrain");
        let before = record.placements.len();
        let mut of_terrain = set(Vec::new(), vec![InstanceCopy::at(0.0, 0.0, 0.0)]);
        if let Placement::Instances { generator_ref, .. } = &mut of_terrain {
            *generator_ref = terrain;
        }
        record.placements.push(of_terrain);
        record.placements.push(set(
            Vec::new(),
            vec![InstanceCopy {
                variant: 7,
                ..InstanceCopy::at(0.0, 0.0, 0.0)
            }],
        ));
        record.sanitize();
        assert_eq!(
            record.placements.len(),
            before + 1,
            "the terrain's set goes"
        );
        assert!(copies_of(record.placements.last().expect("the set")).is_empty());
    }
}
