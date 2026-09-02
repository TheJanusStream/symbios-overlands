//! Test-only assertion helpers shared by every theme's guards: sanitiser
//! stability, the owner-panel idiom, glazing-card geometry, tilt and blob
//! connectivity. Reached through [`util`](super) like the constructors.

use crate::pds::PrimCommon;
use crate::pds::{Generator, GeneratorKind, SovereignTextureConfig};

/// Assert that `sanitize_generator` leaves a primitive-built entry
/// geometrically untouched. Rotations are compared with an epsilon
/// because the sanitiser renormalises every quaternion, which can
/// shift the last ulp of an already-normalised rotation; everything
/// else must be bit-identical.
pub(in crate::catalogue::items) fn assert_sanitize_stable(built: &Generator, name: &str) {
    fn tree_eq(a: &Generator, b: &Generator, name: &str) {
        assert_eq!(a.kind, b.kind, "{name}: kind rewritten by sanitiser");
        assert_eq!(
            a.transform.translation, b.transform.translation,
            "{name}: translation rewritten"
        );
        assert_eq!(
            a.transform.scale, b.transform.scale,
            "{name}: scale rewritten"
        );
        for i in 0..4 {
            assert!(
                (a.transform.rotation.0[i] - b.transform.rotation.0[i]).abs() < 1e-5,
                "{name}: rotation rewritten beyond renormalisation: {:?} vs {:?}",
                a.transform.rotation,
                b.transform.rotation
            );
        }
        assert_eq!(a.children.len(), b.children.len(), "{name}: child dropped");
        for (ca, cb) in a.children.iter().zip(b.children.iter()) {
            tree_eq(ca, cb, name);
        }
    }
    let mut sanitized = built.clone();
    crate::pds::sanitize_generator(&mut sanitized);
    tree_eq(built, &sanitized, name);
}

/// Minimum clearance between an owner panel and anything solid behind it.
///
/// Deliberately small — a backing plate is *supposed* to sit millimetres
/// behind the portrait, and only a body whose face comes out **in front of**
/// the panel is a bug. Four millimetres is enough to rule out a coplanar
/// depth tie without outlawing the plate. Ten of the twenty-four monuments
/// shipped with the panel between 5 mm and 40 mm behind its own body (#977).
const PANEL_REVEAL: f32 = 0.004;

/// Assert that an identity monument (#975) carries the owner's profile
/// picture correctly — the shared guard all 24 themed monuments call.
///
/// One helper rather than 24 copies, because these are invariants of the
/// *idiom*, not of any one monument: get any of them wrong and the panel is
/// still a perfectly valid `Sign` that renders, just a distorted, tiled,
/// night-blind or back-to-front one. None of that is visible in the render
/// tool, which never fetches an image at all.
///
/// What it pins:
///
/// * exactly one panel, pointed at **this room's** DID (a monument showing
///   somebody else's face is the worst failure available here);
/// * square — the aspect a face cannot survive losing;
/// * `uv_scale` 1.0 — Sign images are clamp-to-edge, so anything else crops
///   and smears rather than tiles;
/// * a pure white tint — `base_color` multiplies the fetched image, so any
///   other colour stains the owner's face, and it stains it *only once a
///   picture loads*, which is the state no render here can show (#976);
/// * the standing rotation `quat_x(FRAC_PI_2)` — the panel's wound front is
///   `−Y`, so the `-FRAC_PI_2` that stands up an ordinary quad turns this one
///   away from the viewer *and* upside-down;
/// * `unlit` and single-sided — legible at any hour, never mirrored;
/// * monument scale: a panel big enough and high enough to read from the
///   gateway's landing, on a prop tall enough to be a monument;
/// * something solid **behind** it, and nothing solid **around** it. The
///   panel is single-sided, so without a backing plate the monument is
///   see-through from behind — but the mirror failure is worse and was
///   shipping on ten of the twenty-four (#977): a panel authored a centimetre
///   or two *inside* the body it is mounted on, so the portrait is buried in
///   the slab and z-fights with it. Both checks lean on the family's
///   authoring convention — hero face toward `-Z`, so "behind" is `+Z` —
///   which is the same convention the render tool and the settlement placer
///   already assume.
pub(in crate::catalogue::items) fn assert_owner_panel(
    entry: &dyn crate::catalogue::CatalogueEntry,
    did: &str,
) {
    use crate::pds::{GeneratorKind, SignSource};

    let slug = entry.slug();
    let root = entry.build(did);
    /// What one walk of the tree collects. A struct rather than eight
    /// out-parameters: `boxes` holds axis-aligned cuboids only, as
    /// `(centre, half-extents)`, because the burial check needs a box and a
    /// tilted prim's AABB would over-report — and every monument's *body*, the
    /// only mass that can bury a panel, is axis-aligned by the family's own
    /// rule that a tilted sub-root spins what it carries.
    #[derive(Default)]
    struct Census {
        panels: Vec<([f32; 3], f32)>,
        solids: Vec<[f32; 3]>,
        boxes: Vec<([f32; 3], [f32; 3])>,
        top: f32,
    }
    let mut census = Census {
        top: f32::MIN,
        ..Default::default()
    };

    fn walk(g: &Generator, at: [f32; 3], slug: &str, did: &str, c: &mut Census) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        c.top = c.top.max(here[1]);
        match &g.kind {
            GeneratorKind::Sign {
                source,
                size,
                material,
                double_sided,
                unlit,
                ..
            } => {
                match source {
                    SignSource::DidPfp { did: d } => assert_eq!(
                        d, did,
                        "{slug}: the panel points at {d}, not at the room owner"
                    ),
                    other => panic!("{slug}: panel source is {other:?}, not the owner's pfp"),
                }
                assert_eq!(
                    size.0[0], size.0[1],
                    "{slug}: panel {:?} is not square — a face cannot survive the stretch",
                    size.0
                );
                assert_eq!(
                    material.uv_scale.0, 1.0,
                    "{slug}: Sign images are clamp-to-edge; uv_scale above 1.0 crops and smears"
                );
                assert_eq!(
                    material.base_color.0,
                    [1.0, 1.0, 1.0],
                    "{slug}: base_color multiplies the fetched image, so anything but \
                     white stains the owner's face"
                );
                // The standing rotation, checked on the node itself: the panel
                // must be turned to face `−Z` the right way up, and the two
                // half-angles differ only in sign, so this is a one-character
                // mistake that no render taken here can catch.
                let q = g.transform.rotation.0;
                let want = std::f32::consts::FRAC_PI_4.sin();
                assert!(
                    (q[0] - want).abs() < 1e-4
                        && q[1].abs() < 1e-4
                        && q[2].abs() < 1e-4
                        && (q[3] - want).abs() < 1e-4,
                    "{slug}: panel rotation {q:?} is not quat_x(+FRAC_PI_2) — a \
                     negative half-angle here faces it away and inverts it"
                );
                assert!(*unlit, "{slug}: a lit portrait goes black at dusk");
                assert!(
                    !*double_sided,
                    "{slug}: a double-sided panel shows the owner mirrored from behind"
                );
                c.panels.push((here, size.0[0]));
            }
            GeneratorKind::Cuboid { size, .. } => {
                c.solids.push(here);
                if g.transform.rotation.0 == [0.0, 0.0, 0.0, 1.0] {
                    c.boxes
                        .push((here, [size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5]));
                }
            }
            GeneratorKind::Cylinder { .. }
            | GeneratorKind::Sphere { .. }
            | GeneratorKind::Superellipsoid { .. } => c.solids.push(here),
            _ => {}
        }
        for child in &g.children {
            walk(child, here, slug, did, c);
        }
    }
    walk(&root, [0.0; 3], slug, did, &mut census);
    let Census {
        panels,
        solids,
        boxes,
        top,
    } = census;

    assert_eq!(
        panels.len(),
        1,
        "{slug}: expected exactly one owner panel, found {}",
        panels.len()
    );
    let (at, side) = panels[0];
    assert!(
        side >= 1.2,
        "{slug}: a {side} m panel is too small to read from the gateway's landing"
    );
    assert!(
        at[1] - side * 0.5 > 1.0,
        "{slug}: the panel's bottom edge sits at {}, low enough to be walked in front of",
        at[1] - side * 0.5
    );
    assert!(
        top >= 4.0,
        "{slug}: the monument tops out at {top} m — not monument scale"
    );
    assert!(
        solids.iter().any(|s| {
            (s[0] - at[0]).abs() < side * 0.75
                && (s[1] - at[1]).abs() < side * 0.75
                && s[2] > at[2]
                && s[2] - at[2] < 1.0
        }),
        "{slug}: nothing stands behind the panel — it is single-sided, so the \
         monument is see-through from the back"
    );

    // ...and nothing stands *in* it. A body whose front face is nearer the
    // viewer than the panel swallows the portrait: the panel is either hidden
    // outright or z-fights with the slab it was meant to sit proud of.
    for (c, e) in &boxes {
        let covers_panel = (c[0] - at[0]).abs() < e[0] && (c[1] - at[1]).abs() < e[1];
        if !covers_panel {
            continue;
        }
        let front = c[2] - e[2];
        assert!(
            front > at[2] + PANEL_REVEAL,
            "{slug}: a solid at {c:?} presents its face at z = {front}, in front of \
             a panel at z = {} — the portrait is buried in the body it is mounted \
             on. The panel must stand at least {PANEL_REVEAL} m proud of it.",
            at[2]
        );
    }
}

/// Rotate `v` by the quaternion `q` (`[x, y, z, w]`) — the guards' one
/// implementation of the thing that is easiest to get backwards (#972).
///
/// A guard that checks where a *tilted* part's ends actually land has to turn
/// a local half-extent into world space, and hand-rolling that as
/// `(sin θ, cos θ)` has a fifty-fifty chance of picking the wrong handedness.
/// When it picks wrong, it agrees with a part rotated the wrong way and the
/// two errors cancel: the lifeguard tower's boarding ramp pointed downhill
/// toward its own deck, and a guard written the same afternoon confirmed both
/// ends were exactly where they should be.
///
/// So there is one of these, it uses the standard right-handed formula, and
/// no guard writes its own. `Quat::from_rotation_x(θ)` turns `+Y` toward
/// `+Z`, which means a prim's local `+Z` end goes **down** for positive θ —
/// the opposite of what "tilt it up by θ" suggests, and the reason this note
/// is longer than the function.
pub(in crate::catalogue::items) fn rotate_by(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let u = [q[0], q[1], q[2]];
    let uv = cross(u, v);
    let uuv = cross(u, uv);
    [
        v[0] + 2.0 * (q[3] * uv[0] + uuv[0]),
        v[1] + 2.0 * (q[3] * uv[1] + uuv[1]),
        v[2] + 2.0 * (q[3] * uv[2] + uuv[2]),
    ]
}

/// One upright glazing card as the guards see it: its world centre and the
/// `[width, height]` of quad it spans.
pub(in crate::catalogue::items) struct CardRect {
    pub center: [f32; 3],
    pub size: [f32; 2],
}

/// Collect every upright [`window_card`](super::material::window_card) in a built tree, in the prop's own
/// world frame.
///
/// "Upright" means a [`plane`](super::build::plane) stood up by `quat_x(±FRAC_PI_2)`, which maps
/// the quad's local Z extent onto world Y — so a card's `size` reads as
/// `[width, height]` and it occupies a rectangle on a single Z plane. That is
/// the whole family's glazing idiom, and it is what makes the geometric
/// guards below expressible at all.
///
/// Translations accumulate down the tree; rotations are not composed, because
/// every prop in this family keeps its sub-roots axis-aligned by the rule
/// that a tilted parent spins what it carries ([`nest`](super::build::nest)). A card under a
/// tilted parent is therefore reported at the wrong place — which is a bug
/// worth having surface as a failing guard rather than a silent skip.
pub(in crate::catalogue::items) fn window_cards(root: &Generator) -> Vec<CardRect> {
    fn walk(g: &Generator, at: [f32; 3], out: &mut Vec<CardRect>) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        if let GeneratorKind::Plane {
            size,
            common: PrimCommon { material, .. },
            ..
        } = &g.kind
            && matches!(material.texture, SovereignTextureConfig::Window(_))
        {
            let q = g.transform.rotation.0;
            let upright = q[0].abs() > 0.5 && q[1].abs() < 1e-4 && q[2].abs() < 1e-4;
            if upright {
                out.push(CardRect {
                    center: here,
                    size: size.0,
                });
            }
        }
        for c in &g.children {
            walk(c, here, out);
        }
    }
    let mut out = Vec::new();
    walk(root, [0.0; 3], &mut out);
    out
}

/// Assert that no two glazing cards sharing a Z plane overlap (#972).
///
/// Two `Window` cards on the same plane, overlapping, is a depth tie the
/// rasteriser breaks arbitrarily — and because both are alpha-masked frames,
/// the result is a band of interleaved mullion that reads as neither. It is
/// the same failure the coplanar rule describes, arrived at between two cards
/// rather than between a card and its reveal, and it is invisible in a
/// straight-on render: the office block shipped with its shopfront band
/// running 0.4 m up into the bottom of its own curtain wall, and the sheet
/// simply showed a slightly muddled row.
///
/// Cards on *different* planes are none of this guard's business — a door
/// leaf proud of the glazing behind it is the idiom working.
pub(in crate::catalogue::items) fn assert_cards_do_not_overlap(root: &Generator, slug: &str) {
    let cards = window_cards(root);
    for (i, a) in cards.iter().enumerate() {
        for b in &cards[i + 1..] {
            if (a.center[2] - b.center[2]).abs() > 1e-3 {
                continue;
            }
            let overlaps = |axis: usize| {
                (a.center[axis] - b.center[axis]).abs() < (a.size[axis] + b.size[axis]) * 0.5 - 1e-4
            };
            assert!(
                !(overlaps(0) && overlaps(1)),
                "{slug}: glazing cards at {:?} ({:?}) and {:?} ({:?}) share a Z plane \
                 and overlap — two alpha-masked frames tie for depth over the overlap",
                a.center,
                a.size,
                b.center,
                b.size
            );
        }
    }
}

/// Assert that no **solid** primitive in a tree wears a `Window` texture
/// (#972 lesson 1, stated as a prohibition rather than as a census).
///
/// The card guards elsewhere count cards and check the ones they find. This
/// checks the other side of the same rule, and it is the stronger half: a
/// `Window` texture on a cuboid or a cylinder is *always* wrong, whatever the
/// count says, because the generator masks its panes away and upstream renders
/// every card at `AlphaMode::Mask(0.5)`. So the slab becomes a frame with real
/// holes in it, showing whatever solid it was stuck to — and on a cuboid it
/// grows windows on its sides, top and bottom into the bargain.
///
/// Worth naming separately because the failure has a *sociable* form: the
/// grand hotel acquired it by reaching for another kit's
/// [`curtain_wall`](crate::catalogue::items::modern_city::curtain_wall)
/// helper, which is a lit glass box behind proud fins — correct on the tower
/// it was written for, and unable to be a window anywhere else. A count-based
/// guard passes that happily; this one does not.
pub(in crate::catalogue::items) fn assert_no_glazing_on_solids(root: &Generator, slug: &str) {
    fn walk(g: &Generator, at: [f32; 3], slug: &str) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        let material = match &g.kind {
            GeneratorKind::Cuboid {
                common: PrimCommon { material, .. },
                ..
            }
            | GeneratorKind::Cylinder {
                common: PrimCommon { material, .. },
                ..
            }
            | GeneratorKind::Sphere {
                common: PrimCommon { material, .. },
                ..
            }
            | GeneratorKind::Cone {
                common: PrimCommon { material, .. },
                ..
            }
            | GeneratorKind::Capsule {
                common: PrimCommon { material, .. },
                ..
            }
            | GeneratorKind::Torus {
                common: PrimCommon { material, .. },
                ..
            }
            | GeneratorKind::Superellipsoid {
                common: PrimCommon { material, .. },
                ..
            } => Some(material),
            _ => None,
        };
        if let Some(m) = material {
            assert!(
                !matches!(m.texture, SovereignTextureConfig::Window(_)),
                "{slug}: a {} at {here:?} wears a Window texture — its panes are \
                 masked away, so it is a frame with holes onto whatever stands \
                 behind it. Cards belong on a flat Plane over a real opening.",
                g.kind.kind_tag()
            );
        }
        for c in &g.children {
            walk(c, here, slug);
        }
    }
    walk(root, [0.0; 3], slug);
}

/// Assert that a rotated node carries children **only at its own origin**
/// (#972).
///
/// The oldest gotcha in this family's list and, until now, the only one with
/// no test: rotation propagates down a tree, so a tilted parent spins
/// everything it holds. A ramp board built as a sub-root with its cleats
/// nested under it turns the cleats twice and swings their offsets out of the
/// surface — and because [`nest`](super::build::nest) rebases only *translations*, the authored
/// world position and the rendered one part company with nothing in the
/// record looking wrong.
///
/// It is also how a guard gets fooled twice over. A footprint check that
/// accumulates translations down the tree — which is what every guard in this
/// family does, because composing rotations for a family that has almost none
/// would be noise — reports a tilted parent's children where they were
/// *authored*, not where they render. The tilt hides the fault from the render
/// and from the guard at once. Forbidding the shape is what keeps every
/// translation-only walk sound by construction.
///
/// # Why "at its own origin" and not "never"
///
/// A tilted parent is perfectly safe when its children sit at its own origin,
/// because then the rotation moves nothing and only *orientation* propagates —
/// which is the whole point of authoring a rig as one turned assembly. The
/// kit's [`valve_wheel`](crate::catalogue::items::industrial_park::valve_wheel)
/// is exactly that: a rim turned to face a pipe, with its hub and spokes at
/// `[0, 0, 0]` so they ride the turn. `nest`'s own note calls the same thing
/// the point on a leaning mast.
///
/// So the rule is about **offset children under a turn**, which is the only
/// form that displaces anything. The fix when it fires is always the same:
/// demote the tilted piece to a child and give the sub-assembly a flat root —
/// the thing it stands on (the ramp's foot kerb, the awning's poles).
pub(in crate::catalogue::items) fn assert_no_tilted_parents(root: &Generator, slug: &str) {
    fn walk(g: &Generator, at: [f32; 3], slug: &str) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        let q = g.transform.rotation.0;
        let upright = q[0].abs() < 1e-4 && q[1].abs() < 1e-4 && q[2].abs() < 1e-4;
        if !upright {
            for c in &g.children {
                let o = c.transform.translation.0;
                assert!(
                    o[0].abs() < 1e-4 && o[1].abs() < 1e-4 && o[2].abs() < 1e-4,
                    "{slug}: a rotated {} at {here:?} carries a child offset by \
                     {o:?} — the turn spins that offset, so the child renders \
                     somewhere the record does not say, and every guard here \
                     walks translations only and will agree with the record. \
                     Demote the tilted piece to a child and give the \
                     sub-assembly a flat root. (A turned rig whose children sit \
                     at its own origin is fine — that is what the turn is for.)",
                    g.kind.kind_tag()
                );
            }
        }
        for c in &g.children {
            walk(c, here, slug);
        }
    }
    walk(root, [0.0; 3], slug);
}

/// How many disconnected components a [`blob_group`](super::build::blob_group) polygonises into.
///
/// One is almost always the intended answer: a group's whole reason to exist
/// is that its elements blend into a single skin, and a second component
/// means two of them drifted out of blend range — or, more often, that the
/// mesh is thinner than the sample grid can resolve and has broken up (see
/// [`blob_cell_size`](super::build::blob_cell_size)).
///
/// Union-find over the welded triangle graph. Coincident vertices are welded
/// first because the Box UV projection duplicates a vertex per projection
/// region, and those seam splits are texture topology rather than geometry.
/// Lifted from the avatar suite's
/// `humanoid_blob_masses_are_single_connected_skins`, which is where the
/// technique was worked out, so the catalogue does not roll a second copy.
pub(in crate::catalogue::items) fn blob_components(kind: &GeneratorKind) -> usize {
    use bevy::mesh::VertexAttributeValues;

    fn find(parent: &mut [usize], mut a: usize) -> usize {
        while parent[a] != a {
            parent[a] = parent[parent[a]];
            a = parent[a];
        }
        a
    }
    let mesh = crate::world_builder::build_primitive_mesh(kind).mesh;
    let pos = match mesh.attribute(bevy::prelude::Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
        _ => return 0,
    };
    let Some(indices) = mesh.indices() else {
        return 0;
    };
    let mut weld: std::collections::HashMap<[u32; 3], usize> =
        std::collections::HashMap::with_capacity(pos.len());
    let rep: Vec<usize> = pos
        .iter()
        .enumerate()
        .map(|(i, p)| {
            *weld
                .entry([p[0].to_bits(), p[1].to_bits(), p[2].to_bits()])
                .or_insert(i)
        })
        .collect();
    let n = pos.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut touched = vec![false; n];
    let idx: Vec<usize> = indices.iter().map(|i| rep[i]).collect();
    for tri in idx.chunks(3) {
        for &(a, b) in &[(tri[0], tri[1]), (tri[0], tri[2])] {
            touched[a] = true;
            touched[b] = true;
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            parent[ra] = rb;
        }
    }
    (0..n)
        .filter(|&v| touched[v] && find(&mut parent, v) == v)
        .count()
}

/// Walk a built tree and report whether any primitive is strongly emissive
/// (emission strength > 1.0) — the shared "did the kit's firelit hero keep
/// its glow?" check the per-theme kits assert on (forge fire, saloon lamps,
/// brazier coals, …), so escalation's broken-emissive ruin pass has something
/// to snuff.
pub(in crate::catalogue::items) fn has_emissive(g: &crate::pds::Generator) -> bool {
    use crate::pds::GeneratorKind::*;
    let own = match &g.kind {
        Cuboid { common, .. }
        | Cylinder { common, .. }
        | Sphere { common, .. }
        | Cone { common, .. }
        | Torus { common, .. }
        | Capsule { common, .. } => common.material.emission_strength.0 > 1.0,
        _ => false,
    };
    own || g.children.iter().any(has_emissive)
}

/// One axis-aligned face of a built primitive as [`assert_no_coplanar_faces`]
/// sees it: the world axis it faces along, which way, the plane it lies on,
/// and the rectangle it covers on the other two axes (ascending order).
struct AaFace {
    axis: usize,
    sign: f32,
    plane: f32,
    lo: [f32; 2],
    hi: [f32; 2],
    tag: &'static str,
    at: [f32; 3],
}

/// The world half-extents of a prim's local half-extents under a rotation
/// that is a whole number of quarter turns — or `None` if the turn is
/// oblique, in which case the prim has no axis-aligned faces to speak of.
fn quarter_turned(q: [f32; 4], half: [f32; 3]) -> Option<[f32; 3]> {
    let mut ext = [0.0_f32; 3];
    for (axis, h) in half.iter().enumerate() {
        let mut v = [0.0; 3];
        v[axis] = 1.0;
        let w = rotate_by(q, v);
        let big = w.iter().map(|c| c.abs()).fold(0.0_f32, f32::max);
        if big < 0.9999 {
            return None;
        }
        for (e, c) in ext.iter_mut().zip(w) {
            *e += c.abs() * h;
        }
    }
    Some(ext)
}

fn aa_faces(g: &Generator, at: [f32; 3]) -> Vec<AaFace> {
    let q = g.transform.rotation.0;
    let tag = g.kind.kind_tag();
    let mut out = Vec::new();
    let rect = |axis: usize, ext: [f32; 3], shrink: f32| -> ([f32; 2], [f32; 2]) {
        let others: Vec<usize> = (0..3).filter(|a| *a != axis).collect();
        let (b, c) = (others[0], others[1]);
        (
            [at[b] - ext[b] * shrink, at[c] - ext[c] * shrink],
            [at[b] + ext[b] * shrink, at[c] + ext[c] * shrink],
        )
    };
    let push =
        |out: &mut Vec<AaFace>, axis: usize, sign: f32, plane: f32, r: ([f32; 2], [f32; 2])| {
            out.push(AaFace {
                axis,
                sign,
                plane,
                lo: r.0,
                hi: r.1,
                tag,
                at,
            });
        };
    match &g.kind {
        GeneratorKind::Cuboid {
            size,
            common: PrimCommon { torture, .. },
            ..
        } => {
            let half = [size.0[0] * 0.5, size.0[1] * 0.5, size.0[2] * 0.5];
            let Some(ext) = quarter_turned(q, half) else {
                return out;
            };
            let taper = torture.taper.0[0].max(torture.taper.0[1]);
            let up = rotate_by(q, [0.0, 1.0, 0.0]);
            let (up_axis, up_sign) = (0..3)
                .map(|a| (a, up[a]))
                .max_by(|x, y| x.1.abs().partial_cmp(&y.1.abs()).unwrap())
                .unwrap();
            for axis in 0..3 {
                for sign in [-1.0_f32, 1.0] {
                    let is_top = axis == up_axis && sign == up_sign.signum();
                    let is_bottom = axis == up_axis && sign == -up_sign.signum();
                    if taper > 1e-6 && !is_top && !is_bottom {
                        continue; // slanted, not a plane
                    }
                    let shrink = if is_top { 1.0 - taper } else { 1.0 };
                    if shrink < 1e-3 {
                        continue;
                    }
                    let plane = at[axis] + sign * ext[axis];
                    push(&mut out, axis, sign, plane, rect(axis, ext, shrink));
                }
            }
        }
        GeneratorKind::Cylinder {
            radius,
            height,
            common: PrimCommon { torture, .. },
            ..
        }
        | GeneratorKind::Tube {
            radius,
            height,
            common: PrimCommon { torture, .. },
            ..
        }
        | GeneratorKind::Cone {
            radius,
            height,
            common: PrimCommon { torture, .. },
            ..
        } => {
            let cut = torture.path_cut.0 != [0.0, 1.0] || torture.profile_cut.0 != [0.0, 1.0];
            if cut {
                return out; // partial caps: not a whole face
            }
            let r = radius.0;
            let Some(ext) = quarter_turned(q, [r, height.0 * 0.5, r]) else {
                return out;
            };
            let up = rotate_by(q, [0.0, 1.0, 0.0]);
            let (axis, s) = (0..3)
                .map(|a| (a, up[a]))
                .max_by(|x, y| x.1.abs().partial_cmp(&y.1.abs()).unwrap())
                .unwrap();
            let s = s.signum();
            // Bottom cap, full radius, facing -local Y.
            push(
                &mut out,
                axis,
                -s,
                at[axis] - s * ext[axis],
                rect(axis, ext, 1.0),
            );
            // Top cap, tapered radius (a cone's is a point: no face).
            let top = match &g.kind {
                GeneratorKind::Cone { .. } => 0.0,
                _ => 1.0 - torture.taper.0[0].max(torture.taper.0[1]),
            };
            if top > 1e-3 {
                push(
                    &mut out,
                    axis,
                    s,
                    at[axis] + s * ext[axis],
                    rect(axis, ext, top),
                );
            }
        }
        _ => {}
    }
    out
}

/// Assert that no two axis-aligned faces in a tree lie on one plane, face
/// the **same** way, and overlap (#972).
///
/// This is the coplanar z-fight stated as a guard. Two faces on one plane
/// with the same normal tie for depth wherever they overlap, and the
/// rasteriser breaks the tie per pixel per frame — the speckle the standing
/// gotcha describes. The classic shapes: a lid cylinder exactly as long as
/// the box it caps, so its end discs sit on the box's end faces; a spoke bar
/// whose outer face lands flush with the hub's; a trim slab sized to meet
/// its host exactly.
///
/// Abutting faces — coincident but facing *opposite* ways, a slat's bottom
/// on a rail's top — are how solids sit on each other and are not flagged.
/// Only whole planar faces are read: rotated prims by a whole number of
/// quarter turns, the caps of uncut revolved prims, a tapered box's top and
/// bottom (its sides slant). Oblique, cut or organic prims are left alone,
/// which is the conservative direction for a guard whose false positive
/// would be a fault nobody can see.
pub(in crate::catalogue::items) fn assert_no_coplanar_faces(root: &Generator, slug: &str) {
    fn walk(g: &Generator, at: [f32; 3], out: &mut Vec<AaFace>) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        out.extend(aa_faces(g, here));
        for c in &g.children {
            walk(c, here, out);
        }
    }
    let mut faces = Vec::new();
    walk(root, [0.0; 3], &mut faces);
    // Every tie, not the first: a fresh guard on a new build usually finds
    // several, and one round trip per tie is the expensive way to learn them.
    let mut ties = Vec::new();
    for (i, a) in faces.iter().enumerate() {
        for b in &faces[i + 1..] {
            if a.axis != b.axis || a.sign != b.sign || (a.plane - b.plane).abs() > 5e-4 {
                continue;
            }
            let overlap = (0..2).all(|k| a.lo[k].max(b.lo[k]) < a.hi[k].min(b.hi[k]) - 1e-4);
            if overlap {
                ties.push(format!(
                    "a {} at {:?} and a {} at {:?} on the plane axis {} = {:.4} facing {}",
                    a.tag,
                    a.at,
                    b.tag,
                    b.at,
                    a.axis,
                    a.plane,
                    if a.sign > 0.0 { "+" } else { "-" }
                ));
            }
        }
    }
    assert!(
        ties.is_empty(),
        "{slug}: {} pair(s) of faces share a plane, face the same way and overlap — a depth \
         tie the rasteriser breaks per pixel (z-fight). Sink one into the other or stand it \
         proud:\n  {}",
        ties.len(),
        ties.join("\n  ")
    );
}
