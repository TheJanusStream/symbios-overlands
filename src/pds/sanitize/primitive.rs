//! Sanitiser for the parametric primitive variants of [`GeneratorKind`]
//! (Cuboid / Sphere / Cylinder / Capsule / Cone / Torus / Plane /
//! Tetrahedron / Tube / Bevel / Wedge / Helix / Superellipsoid / Spine /
//! Lathe / BlobGroup). Mirrors the
//! bounds the World Editor UI exposes so a
//! hand-crafted record can't push mesh/collider builders into NaN / OOM
//! territory.

use std::collections::HashSet;

use super::Sanitize;
use super::common::{clamp_finite, sanitize_torture};
use crate::pds::generator::GeneratorKind;
use crate::pds::types::{Fp, Fp2, Fp3};

pub(super) fn sanitize_primitive(kind: &mut GeneratorKind) {
    sanitize_faces(kind);
    let c_dim = |v: f32| clamp_finite(v, 0.01, 100.0, 1.0);
    match kind {
        GeneratorKind::Cuboid {
            size,
            material,
            torture,
            ..
        } => {
            size.0 = [c_dim(size.0[0]), c_dim(size.0[1]), c_dim(size.0[2])];
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Sphere {
            radius,
            resolution,
            material,
            torture,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            // Ico subdivision count is exponential in triangles (~20·4ⁿ), so
            // cap it low: ico 6 is ~82k tris (already far past any shipped
            // content, which tops out at ico 4 ≈ 5k), while the old cap of 10
            // would be ~20M tris per sphere — a single-record perf cliff.
            *resolution = (*resolution).clamp(0, 6);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Cylinder {
            radius,
            height,
            resolution,
            material,
            torture,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *height = Fp(c_dim(height.0));
            *resolution = (*resolution).clamp(3, 128);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Capsule {
            radius,
            length,
            latitudes,
            longitudes,
            material,
            torture,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *length = Fp(c_dim(length.0));
            *latitudes = (*latitudes).clamp(2, 64);
            *longitudes = (*longitudes).clamp(4, 128);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Cone {
            radius,
            height,
            resolution,
            material,
            torture,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *height = Fp(c_dim(height.0));
            *resolution = (*resolution).clamp(3, 128);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Torus {
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            material,
            torture,
            ..
        } => {
            *minor_radius = Fp(c_dim(minor_radius.0));
            *major_radius = Fp(c_dim(major_radius.0));
            *minor_resolution = (*minor_resolution).clamp(3, 64);
            *major_resolution = (*major_resolution).clamp(3, 128);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Plane {
            size,
            subdivisions,
            material,
            torture,
            ..
        } => {
            *size = Fp2([c_dim(size.0[0]), c_dim(size.0[1])]);
            *subdivisions = (*subdivisions).clamp(0, 32);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Tetrahedron {
            size,
            material,
            torture,
            ..
        } => {
            *size = Fp(c_dim(size.0));
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Tube {
            radius,
            inner_radius,
            height,
            resolution,
            material,
            torture,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            *height = Fp(c_dim(height.0));
            // Bore stays strictly inside the outer wall (0 = a near-solid rod).
            *inner_radius = Fp(clamp_finite(
                inner_radius.0,
                0.0,
                radius.0 * 0.95,
                radius.0 * 0.5,
            ));
            *resolution = (*resolution).clamp(3, 128);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Bevel {
            size,
            bevel,
            bevel_segments,
            material,
            torture,
            ..
        } => {
            size.0 = [c_dim(size.0[0]), c_dim(size.0[1]), c_dim(size.0[2])];
            // The corner radius can't exceed half the smaller footprint axis.
            let max_b = (size.0[0].min(size.0[2]) * 0.5).max(0.0);
            *bevel = Fp(clamp_finite(bevel.0, 0.0, max_b, 0.0));
            *bevel_segments = (*bevel_segments).clamp(1, 16);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Wedge {
            size,
            material,
            torture,
            ..
        } => {
            size.0 = [c_dim(size.0[0]), c_dim(size.0[1]), c_dim(size.0[2])];
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Superellipsoid {
            half_extents,
            exponent_ns,
            exponent_ew,
            latitudes,
            longitudes,
            material,
            torture,
            ..
        } => {
            half_extents.0 = [
                c_dim(half_extents.0[0]),
                c_dim(half_extents.0[1]),
                c_dim(half_extents.0[2]),
            ];
            // The signed-power parametrisation misbehaves outside this band:
            // exponents → 0 spike the analytic normals along the creases, and
            // past ~2.5 the pinched form thins into degenerate spans that the
            // convex-hull collider can't follow anyway.
            for e in [exponent_ns, exponent_ew] {
                *e = Fp(clamp_finite(e.0, 0.2, 2.5, 1.0));
            }
            *latitudes = (*latitudes).clamp(4, 64);
            *longitudes = (*longitudes).clamp(4, 128);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::BlobGroup {
            elements,
            resolution,
            material,
            torture,
            ..
        } => {
            elements.truncate(super::limits::MAX_BLOB_ELEMENTS);
            if elements.is_empty() {
                elements.push(crate::pds::generator::BlobElement::default());
            }
            for e in elements.iter_mut() {
                for c in e.position.0.iter_mut() {
                    *c = clamp_finite(*c, -100.0, 100.0, 0.0);
                }
                for r in e.radii.0.iter_mut() {
                    *r = c_dim(*r);
                }
                e.blend = Fp(clamp_finite(
                    e.blend.0,
                    0.0,
                    super::limits::MAX_BLOB_BLEND,
                    0.1,
                ));
                // Unit quaternion or identity — the mesher inverts it.
                e.rotation =
                    crate::pds::types::Fp4(super::common::sanitize_unit_quat(e.rotation.0));
            }
            *resolution = (*resolution).clamp(8, super::limits::MAX_BLOB_RESOLUTION);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Spine {
            points,
            resolution,
            samples_per_segment,
            material,
            torture,
            ..
        } => {
            points.truncate(super::limits::MAX_SWEEP_POINTS);
            // A spline needs two ends; a starved list becomes a default
            // vertical rod rather than an invisible / panicking prim.
            while points.len() < 2 {
                let i = points.len();
                points.push(crate::pds::generator::SpinePoint {
                    position: Fp3([0.0, i as f32 - 0.5, 0.0]),
                    radius: Fp(0.15),
                });
            }
            for p in points.iter_mut() {
                for c in p.position.0.iter_mut() {
                    *c = clamp_finite(*c, -100.0, 100.0, 0.0);
                }
                p.radius = Fp(c_dim(p.radius.0));
            }
            *resolution = (*resolution).clamp(3, 64);
            *samples_per_segment = (*samples_per_segment).clamp(2, 32);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Lathe {
            points,
            resolution,
            material,
            torture,
            ..
        } => {
            points.truncate(super::limits::MAX_SWEEP_POINTS);
            while points.len() < 2 {
                let i = points.len();
                points.push(crate::pds::generator::LathePoint {
                    radius: Fp(0.2),
                    height: Fp(i as f32 - 0.5),
                });
            }
            for p in points.iter_mut() {
                // Radius may be exactly 0 (a pole pinch); height is a local
                // offset like any position component.
                p.radius = Fp(clamp_finite(p.radius.0, 0.0, 100.0, 0.1));
                p.height = Fp(clamp_finite(p.height.0, -100.0, 100.0, 0.0));
            }
            *resolution = (*resolution).clamp(3, 128);
            material.sanitize();
            sanitize_torture(torture);
        }
        GeneratorKind::Helix {
            radius,
            tube_radius,
            pitch,
            turns,
            resolution,
            material,
            torture,
            ..
        } => {
            *radius = Fp(c_dim(radius.0));
            // Wire stays thinner than the helix radius so the tube can't self-
            // intersect through the axis. `radius` has already been floored at
            // 0.01 by `c_dim`, so that ceiling can sit *below* the 0.01 minimum
            // gauge — 0.0095 at a hairline helix — and `f32::clamp` panics
            // outright when min > max. Ordering the two is what keeps a record
            // naming `radius: 0` from taking down the pass whose entire job is
            // to survive hostile records.
            let hi = radius.0 * 0.95;
            *tube_radius = Fp(clamp_finite(tube_radius.0, hi.min(0.01), hi, hi.min(0.1)));
            *pitch = Fp(clamp_finite(pitch.0, 0.0, 100.0, 0.4));
            *turns = Fp(clamp_finite(turns.0, 0.05, 16.0, 3.0));
            *resolution = (*resolution).clamp(3, 128);
            material.sanitize();
            sanitize_torture(torture);
        }
        _ => {}
    }
}

/// Face-override hygiene (#955), shared by all sixteen primitive arms via
/// the `faces_mut` accessor: duplicate keys collapse to their first entry
/// (the deterministic winner across peers), the list is capped, and every
/// override's material is clamped exactly like the base one. `Unknown`
/// keys — a face name minted by a newer client — are kept: they are
/// dormant, not hostile, and dropping them would strip data on rewrite.
fn sanitize_faces(kind: &mut GeneratorKind) {
    let Some(faces) = kind.faces_mut() else {
        return;
    };
    let mut seen = HashSet::new();
    faces.retain(|f| seen.insert(f.face));
    faces.truncate(super::limits::MAX_FACE_OVERRIDES);
    for f in faces.iter_mut() {
        f.material.sanitize();
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::super::sanitize_kind;
    use super::*;
    use crate::pds::TortureParams;
    use crate::pds::generator::{FaceKey, FaceOverride, primitive_kind_tags};
    use crate::pds::texture::SovereignMaterialSettings;

    /// Widest raw wire number any primitive's *own* geometry field may
    /// still hold once sanitised, in [`crate::pds::types::Fp`] units
    /// (÷10 000 for metres). Every dimensional clamp in this file tops out
    /// at 100 m (`c_dim`) or 100 local units (the point lists), i.e. 1e6
    /// raw, and every count clamp at a few hundred — so 1e8 leaves two
    /// orders of magnitude of headroom while still being far below what an
    /// *unclamped* field carries: the hostile values injected below land at
    /// 4.3e9 (a saturated `u32`) or `i32::MAX` (a saturated `Fp`).
    ///
    /// The `material` and `faces` subtrees are excluded from this envelope
    /// and checked by delegation instead (see
    /// [`every_primitive_delegates_the_shared_blocks`]) — a texture seed is
    /// legitimately a full-range integer, so a magnitude bound says nothing
    /// there.
    const MAX_RAW: i64 = 100_000_000;

    /// Values a hostile record can actually carry into a numeric field.
    /// The wire form of every float is a scaled integer decoded through
    /// `i64` ([`crate::pds::types::Fp`]), so **NaN and infinity are not
    /// wire-reachable at all** — `i64 as f32 / 10_000` is always finite.
    /// What *is* reachable is enormous, negative, and zero, which is what
    /// these are. (`clamp_finite`'s NaN handling is exercised in-memory by
    /// `tests/misc.rs`.)
    ///
    /// The list runs wide on purpose: a `u32` count rejects 9e15 at decode
    /// time while an `Fp` accepts it, and vice-versa for -1, so between
    /// them every numeric field gets at least one value it must clamp.
    const HOSTILE: [i64; 7] = [
        9_000_000_000_000_000,
        4_294_967_295,
        65_535,
        255,
        0,
        -1,
        -9_000_000_000_000_000,
    ];

    /// One step of a path into a `serde_json::Value`.
    #[derive(Clone, Debug)]
    enum Seg {
        Key(String),
        Idx(usize),
    }

    fn show(path: &[Seg]) -> String {
        path.iter()
            .map(|s| match s {
                Seg::Key(k) => format!("/{k}"),
                Seg::Idx(i) => format!("[{i}]"),
            })
            .collect()
    }

    fn collect_numbers(v: &Value, at: &mut Vec<Seg>, out: &mut Vec<Vec<Seg>>) {
        match v {
            Value::Number(_) => out.push(at.clone()),
            Value::Array(a) => {
                for (i, e) in a.iter().enumerate() {
                    at.push(Seg::Idx(i));
                    collect_numbers(e, at, out);
                    at.pop();
                }
            }
            Value::Object(m) => {
                for (k, e) in m {
                    at.push(Seg::Key(k.clone()));
                    collect_numbers(e, at, out);
                    at.pop();
                }
            }
            _ => {}
        }
    }

    fn at<'a>(v: &'a Value, path: &[Seg]) -> &'a Value {
        let mut cur = v;
        for seg in path {
            cur = match seg {
                Seg::Key(k) => cur.get(k).expect("path segment exists"),
                Seg::Idx(i) => cur.get(i).expect("path segment exists"),
            };
        }
        cur
    }

    fn at_mut<'a>(v: &'a mut Value, path: &[Seg]) -> &'a mut Value {
        let mut cur = v;
        for seg in path {
            cur = match seg {
                Seg::Key(k) => cur.get_mut(k).expect("path segment exists"),
                Seg::Idx(i) => cur.get_mut(i).expect("path segment exists"),
            };
        }
        cur
    }

    /// The roster, `kind_tag` and `default_primitive_for_tag` agree — the
    /// three hand-written spellings of the same sixteen names.
    ///
    /// `default_primitive_for_tag` answers `None` for an unknown tag, so a
    /// primitive that joined the roster without a default arm used to be a
    /// silently un-constructible shape rather than a compile error.
    #[test]
    fn every_primitive_has_a_default_and_a_matching_tag() {
        for tag in primitive_kind_tags() {
            let kind = GeneratorKind::default_primitive_for_tag(tag).unwrap_or_else(|| {
                panic!(
                    "{tag} is on the for_each_primitive! roster but \
                     GeneratorKind::default_primitive_for_tag has no arm for it"
                )
            });
            assert_eq!(
                kind.kind_tag(),
                *tag,
                "kind_tag disagrees with the for_each_primitive! roster"
            );
            assert!(kind.is_primitive(), "{tag} does not report as a primitive");
            assert!(
                crate::catalogue::items::measure::is_primitive(&kind),
                "{tag} is not a primitive to the mesher-side is_primitive"
            );
        }
    }

    /// **The clamp-arm gate.** For every primitive on the roster, every
    /// numeric field of its wire form is replaced — one at a time — with a
    /// value a hostile record can carry, and the result must come back
    /// inside the sanitiser's envelope.
    ///
    /// This is the test the roster macro cannot be: `sanitize_primitive`'s
    /// ladder ends in `_ => {}`, so a seventeenth primitive with no clamp
    /// arm compiles, routes (the router is generated from the roster now),
    /// ships, and is simply never clamped. No macro can write a per-variant
    /// bound — every primitive's dimensions mean something different — so
    /// enumeration is the only thing that can hold this line.
    #[test]
    fn every_primitive_clamps_hostile_wire_values() {
        for tag in primitive_kind_tags() {
            let kind = GeneratorKind::default_primitive_for_tag(tag).expect("roster has a default");
            let pristine = serde_json::to_value(&kind).expect("primitive serialises");
            let mut paths = Vec::new();
            collect_numbers(&pristine, &mut Vec::new(), &mut paths);
            assert!(
                !paths.is_empty(),
                "{tag} has no numeric wire fields at all — is its default really a shape?"
            );

            let mut decoded_any = false;
            for path in &paths {
                for &h in &HOSTILE {
                    let mut doc = pristine.clone();
                    *at_mut(&mut doc, path) = Value::from(h);
                    // A value the field's own integer type rejects is not
                    // wire-reachable, so it is not this test's business.
                    let Ok(mut hostile) = serde_json::from_value::<GeneratorKind>(doc) else {
                        continue;
                    };
                    decoded_any = true;
                    sanitize_kind(&mut hostile);

                    let after = serde_json::to_value(&hostile).expect("sanitised serialises");
                    let mut left = Vec::new();
                    collect_numbers(&after, &mut Vec::new(), &mut left);
                    for p in &left {
                        if p.iter()
                            .any(|s| matches!(s, Seg::Key(k) if k == "material" || k == "faces"))
                        {
                            continue;
                        }
                        let n = at(&after, p).as_i64().expect("wire scalars are integers");
                        assert!(
                            n.abs() <= MAX_RAW,
                            "{tag}: injecting {h} at {} left {}{} = {n} outside the \
                             sanitiser's envelope (±{MAX_RAW} raw). Add a clamp arm for \
                             {tag} in pds::sanitize::primitive.",
                            show(path),
                            tag,
                            show(p),
                        );
                    }

                    // A clamp that does not converge is a clamp that can be
                    // pushed further by re-saving the same record.
                    let mut again = hostile.clone();
                    sanitize_kind(&mut again);
                    assert_eq!(
                        again,
                        hostile,
                        "{tag}: sanitising twice differs from sanitising once after \
                         injecting {h} at {}",
                        show(path)
                    );
                }
            }
            assert!(
                decoded_any,
                "{tag}: no hostile value decoded, so nothing was actually tested"
            );
        }
    }

    /// Every primitive's arm must hand its shared blocks to their own
    /// sanitisers: the base material, each per-face override's material,
    /// and the torture block.
    ///
    /// A forgotten `material.sanitize()` is invisible to the envelope check
    /// above — a texture seed is legitimately a full-range integer, so no
    /// magnitude bound can be asserted over that subtree — but shows up
    /// here as a sanitised prim that is not a fixed point of the material
    /// sanitiser.
    ///
    /// The out-of-range values are named fields rather than something
    /// derived from the wire form, because both blocks serialise
    /// default-eliding: a default material encodes as `{}`, so there is
    /// nothing generic to corrupt. Naming them is safe in the way this
    /// issue cares about — if `roughness` is renamed this test stops
    /// compiling, which is the loud failure, not the silent one.
    #[test]
    fn every_primitive_delegates_the_shared_blocks() {
        let bad_material = SovereignMaterialSettings {
            roughness: Fp(9.0),
            metallic: Fp(-4.0),
            emission_strength: Fp(1.0e6),
            ..Default::default()
        };
        let bad_torture = TortureParams {
            twist: Fp(1.0e6),
            hollow: Fp(10.0),
            ..Default::default()
        };

        for tag in primitive_kind_tags() {
            let mut kind =
                GeneratorKind::default_primitive_for_tag(tag).expect("roster has a default");
            *kind.material_mut().expect("primitive") = bad_material.clone();
            *kind.torture_mut().expect("primitive") = bad_torture;
            // Two overrides on the same face, both hostile: the duplicate
            // must collapse and the survivor's material must be clamped
            // exactly like the base one.
            let over = FaceOverride {
                face: FaceKey::PathCutStart,
                material: bad_material.clone(),
                uv_mapping: None,
            };
            *kind.faces_mut().expect("primitive") = vec![over.clone(), over];

            sanitize_kind(&mut kind);

            let mat = kind.material().expect("primitive");
            let mut settled = mat.clone();
            settled.sanitize();
            assert_eq!(
                &settled, mat,
                "{tag}: base material is not clamped — its arm in \
                 pds::sanitize::primitive never called material.sanitize()"
            );

            let faces = kind.faces().expect("primitive");
            assert_eq!(faces.len(), 1, "{tag}: duplicate face override survived");
            let mut settled = faces[0].material.clone();
            settled.sanitize();
            assert_eq!(
                &settled, &faces[0].material,
                "{tag}: face-override material is not clamped"
            );

            let tort = kind.torture().expect("primitive");
            let mut settled = *tort;
            sanitize_torture(&mut settled);
            assert_eq!(
                &settled, tort,
                "{tag}: torture is not clamped — its arm in \
                 pds::sanitize::primitive never called sanitize_torture()"
            );
        }
    }
}
