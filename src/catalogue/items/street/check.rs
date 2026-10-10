//! A street building held to the conventions (#1598; see the parent
//! module's docs): what the tests hold every street building in the
//! catalogue to, and what the render tool's `--street-check` holds a draft
//! to before it is built in. Each check answers with one line per kind of
//! fault, naming the first part at fault and how many share it - a grammar
//! gone wrong breaks a rule hundreds of times over, and the first is the
//! one to read.

use std::collections::BTreeMap;

use super::{MAX_REACH_M, StreetFit, StreetKind, StreetSpec};
use crate::pds::GeneratorKind;
use crate::pds::sanitize::limits::MAX_SHAPE_SOURCE_BYTES;
use crate::terrain::FOUNDATION_SINK_M;

/// How far above its walls a street building's crown may stand (m): a
/// cornice, a parapet, a roof, a rooftop's plant.
pub(crate) const MAX_CROWN_M: f32 = 10.0;

/// The material slots [`OPENINGS`](super::OPENINGS) reads, which every
/// street building's palette fills.
pub(crate) const OPENING_SLOTS: [&str; 5] =
    ["Glass", "RoomLit", "RoomDark", "ShopGlass", "ShopLit"];

/// What every street building lists as solid, at least (#1572).
const SOLID_AT_LEAST: [&str; 3] = ["Wall", "Door", "Pane"];

/// Grammar seeds a street building is held to the conventions at besides
/// its own: a Berlin footprint's copy draws with a seed of its own (#1505),
/// so the rules hold for the grammar, not for one drawing of it.
pub(crate) const SEEDS: [u64; 3] = [1, 2, 3];

/// How far a box may stray past a line before it counts (m).
const EPS: f64 = 1e-3;

/// How far under its walls' top a part may stand and still roof the lot
/// (m): a deck sunk behind a parapet.
const ROOF_DROP_M: f64 = 0.5;

/// The share of a lot that may stand open to the sky: a lightwell, a
/// roof's notch.
const OPEN_ROOF_SHARE: f64 = 0.1;

/// How many parts a street building of `kind` may draw at any fit: a
/// Berlin street stands it in rows, copy after copy, so a part on one is a
/// part on hundreds.
pub(crate) fn part_budget(kind: StreetKind) -> usize {
    match kind {
        StreetKind::House => 1200,
        StreetKind::Block => 2600,
        StreetKind::Low => 600,
    }
}

/// How many parts `spec` drawn with `rules` at `fit` draws, its own grammar
/// seed: `None` where it draws nothing.
pub(crate) fn parts(spec: &StreetSpec, rules: &str, fit: StreetFit) -> Option<usize> {
    let kind = spec.grammar_with(fit, rules);
    let model = kind.shape_def().expect("a Shape").derive().ok()?;
    Some(model.terminals.len())
}

/// The fits a street building is held to the conventions at: its kind's
/// smallest and largest - the smallest with a ground floor of homes, the
/// largest trading - and two between.
pub(crate) fn sample_fits(kind: StreetKind) -> Vec<StreetFit> {
    let (f, d, s) = (kind.frontages(), kind.depths(), kind.storeys());
    let (fl, dl, sl) = (f.len() - 1, d.len() - 1, s.len() - 1);
    vec![
        StreetFit::new(f[0], d[0], s[0], false),
        StreetFit::new(f[fl], d[dl], s[sl], true),
        StreetFit::new(f[fl / 2], d[0], s[sl], true),
        StreetFit::new(f[0], d[dl], s[sl / 2], false),
    ]
}

/// The axis-aligned box a terminal fills, `(min, max)`, in its grammar's
/// frame.
pub(crate) fn bounds(t: &symbios_shape::Terminal) -> ([f64; 3], [f64; 3]) {
    let s = &t.scope;
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for corner in 0..8 {
        let local = symbios_shape::Vec3::new(
            if corner & 1 == 0 { 0.0 } else { s.size.x },
            if corner & 2 == 0 { 0.0 } else { s.size.y },
            if corner & 4 == 0 { 0.0 } else { s.size.z },
        );
        let p = s.position + s.rotation * local;
        for (axis, v) in [p.x, p.y, p.z].into_iter().enumerate() {
            lo[axis] = lo[axis].min(v);
            hi[axis] = hi[axis].max(v);
        }
    }
    (lo, hi)
}

/// Faults by kind: how many parts share each, and the first.
#[derive(Default)]
struct Faults(BTreeMap<String, (usize, String)>);

impl Faults {
    fn add(&mut self, fault: String, part: impl FnOnce() -> String) {
        self.0
            .entry(fault)
            .and_modify(|(n, _)| *n += 1)
            .or_insert_with(|| (1, part()));
    }

    fn lines(self) -> impl Iterator<Item = String> {
        self.0.into_iter().map(|(fault, (n, first))| match n {
            1 => format!("{fault}: {first}"),
            n => format!("{fault}: {first}, and {} more", n - 1),
        })
    }
}

/// What breaks the conventions in `spec` drawn with `rules` at `fit` and
/// grammar seed `seed` (its own where `None`): one line for each kind of
/// fault, none where it keeps them all. Whether its faces fight or its
/// parts float is [`whole`]'s to answer.
pub(crate) fn conventions(
    spec: &StreetSpec,
    rules: &str,
    fit: StreetFit,
    seed: Option<u64>,
) -> Vec<String> {
    let fit = fit.snapped(spec.kind);
    let mut kind = spec.grammar_with(fit, rules);
    let GeneratorKind::Shape {
        grammar_source,
        materials,
        round_meshes,
        solid_meshes,
        seed: own,
        ..
    } = &mut kind
    else {
        unreachable!("a street building is a Shape");
    };
    if let Some(seed) = seed {
        *own = seed;
    }
    let mut problems = Vec::new();
    // What its mesh lists name, its rules must draw: a mesh listed solid or
    // round that no `I` draws misleads whoever reads the list.
    let drawn = grammar_source.as_str();
    for (list, ids) in [("solid", &*solid_meshes), ("round", &*round_meshes)] {
        for id in ids {
            if !drawn.contains(&format!("I(\"{id}\")")) {
                problems.push(format!(
                    "{id} is listed {list}, but no I(\"{id}\") draws it"
                ));
            }
        }
    }
    if grammar_source.len() > MAX_SHAPE_SOURCE_BYTES {
        problems.push(format!(
            "its grammar is {} bytes, past the {MAX_SHAPE_SOURCE_BYTES} a record keeps of one",
            grammar_source.len()
        ));
    }
    for slot in OPENING_SLOTS {
        if !materials.contains_key(slot) {
            problems.push(format!(
                "its palette has no {slot}, which the openings name"
            ));
        }
    }
    for id in SOLID_AT_LEAST {
        if !solid_meshes.iter().any(|s| s == id) {
            problems.push(format!("{id} is not in its solid meshes"));
        }
    }
    let model = match kind.shape_def().expect("a Shape").derive() {
        Ok(model) => model,
        Err(why) => {
            problems.push(format!("it draws nothing: {}", why.message()));
            return problems;
        }
    };
    let GeneratorKind::Shape { materials, .. } = &kind else {
        unreachable!("a street building is a Shape");
    };

    let (w, d) = (f64::from(fit.frontage), f64::from(fit.depth));
    let walls = f64::from(spec.walls_m(fit.storeys));
    let reach = f64::from(MAX_REACH_M);
    let crown = walls + f64::from(MAX_CROWN_M);
    let sink = f64::from(FOUNDATION_SINK_M);
    let mut faults = Faults::default();
    let mut top = 0.0_f64;
    for t in &model.terminals {
        let (lo, hi) = bounds(t);
        let id = t.mesh_id.as_str();
        let part = || {
            format!(
                "I(\"{id}\") at [{:.2}, {:.2}, {:.2}]..[{:.2}, {:.2}, {:.2}]",
                lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]
            )
        };
        if lo[0] < -EPS || hi[0] > w + EPS {
            faults.add("reaches past a side, where a neighbour stands".into(), part);
        }
        if lo[2] < -reach - EPS || hi[2] > d + reach + EPS {
            faults.add(
                format!("stands more than {MAX_REACH_M} m out of its front or back"),
                part,
            );
        }
        if lo[1] < -EPS {
            faults.add("reaches below its foot".into(), part);
        }
        if hi[1] > crown + EPS {
            faults.add(
                format!("stands more than {MAX_CROWN_M} m over its walls' {walls:.2} m"),
                part,
            );
        }
        if matches!(id, "Door" | "Pane") && lo[1] < sink - EPS {
            faults.add(
                format!("starts under the {FOUNDATION_SINK_M} m its placement sinks it"),
                part,
            );
        }
        match &t.material {
            Some(material) if !materials.contains_key(&material.id) => faults.add(
                format!("names Mat(\"{}\"), not in its palette", material.id),
                part,
            ),
            None => faults.add("has no Mat above it, so no material".into(), part),
            Some(_) => {}
        }
        // An opening thinner across the street than along it faces a side:
        // within a metre of one, it looks into the neighbour's wall.
        if matches!(id, "Door" | "Pane")
            && hi[0] - lo[0] < hi[2] - lo[2]
            && (lo[0] < 1.0 || hi[0] > w - 1.0)
        {
            faults.add("a window or a door faces a party wall".into(), part);
        }
        if id == "Wall" {
            top = top.max(hi[1]);
        }
    }
    problems.extend(faults.lines());
    let budget = part_budget(spec.kind);
    if model.terminals.len() > budget {
        problems.push(format!(
            "it draws {} parts, past the {budget} a {} may",
            model.terminals.len(),
            spec.kind.name()
        ));
    }
    if top < walls - EPS {
        problems.push(format!(
            "its walls stop at {top:.2} m, under its storeys' {walls:.2} m"
        ));
    }

    // Its roof: something at its walls' top or over them across the lot,
    // or it stands open to the sky. Each square metre is looked down on
    // through the boxes of what stands that high.
    let high: Vec<_> = model
        .terminals
        .iter()
        .map(bounds)
        .filter(|(_, hi)| hi[1] >= walls - ROOF_DROP_M)
        .collect();
    let cells: Vec<(f64, f64)> = (0..fit.frontage)
        .flat_map(|x| (0..fit.depth).map(move |z| (f64::from(x) + 0.5, f64::from(z) + 0.5)))
        .collect();
    let open = cells
        .iter()
        .filter(|&&(x, z)| {
            !high
                .iter()
                .any(|(lo, hi)| lo[0] <= x && x <= hi[0] && lo[2] <= z && z <= hi[2])
        })
        .count();
    if open as f64 > OPEN_ROOF_SHARE * cells.len() as f64 {
        problems.push(format!(
            "its roof leaves {open} of its {} square metres open to the sky",
            cells.len()
        ));
    }

    // Its front: a door, and a window in every storey - the storeys show as
    // storeys, not as a stretched wall. A storey's window lies within it:
    // a double-height hall's glass is no storey's, and the storeys over it
    // need windows of their own somewhere along the front.
    let front: Vec<_> = model
        .terminals
        .iter()
        .map(|t| (t.mesh_id.as_str(), bounds(t)))
        .filter(|(_, (lo, hi))| lo[2] < 1.0 && hi[0] - lo[0] >= hi[2] - lo[2])
        .collect();
    if !front.iter().any(|(id, _)| *id == "Door") {
        problems.push("no door on its front".to_string());
    }
    for storey in 0..fit.storeys {
        let foot = if storey == 0 {
            0.0
        } else {
            f64::from(spec.walls_m(storey))
        };
        let head = f64::from(spec.walls_m(storey + 1));
        let lit = front
            .iter()
            .any(|(id, (lo, hi))| *id == "Pane" && lo[1] >= foot - EPS && hi[1] <= head + EPS);
        if !lit {
            problems.push(format!("storey {} has no window on its front", storey + 1));
        }
    }
    problems
}

/// What of `spec`, built with `rules` at `fit` and grammar seed `seed` (its
/// own where `None`), draws two faces in one place (#1440) or floats free
/// of the body (#1571): one line each, none where nothing does. The faces
/// are compared on the platforms the agent builds for.
pub(crate) fn whole(
    spec: &StreetSpec,
    rules: &str,
    fit: StreetFit,
    seed: Option<u64>,
) -> Vec<String> {
    let built = spec.build_with(fit, rules);
    let built = match seed {
        Some(seed) => built.with_shape_seed(seed),
        None => built,
    };
    let mut problems = Vec::new();
    #[cfg(unix)]
    problems.extend(
        crate::agent::coplanar_overlap_lines(&built)
            .into_iter()
            .map(|pair| format!("faces drawn in one place: {pair}")),
    );
    problems.extend(
        crate::render_tool::free_parts(&built)
            .into_iter()
            .map(|(_, part)| format!("floats free: {part}")),
    );
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::modern_city::street_low::SPEC as LOW;

    /// A draft that keeps every convention but the ones under test: a lot
    /// walled in, a door up a step on its front - then one window
    /// stretched up both storeys, a window in its left party wall, and no
    /// roof at all. The critic's probe (#1598), each fault named.
    const STRETCHED: &str = "\
const WallD = 0.34
const PaneSet = 0.14
Lot --> Extrude(GroundH + FloorH * (Storeys - 1)) Mat(\"Lime\") Body
Body --> Comp(Faces) { Front: FrontInset | Back: BackInset | Left: SideWin
    | Right: PartyIn | Top: NIL | Bottom: NIL }
PartyIn --> Translate(0, 0, -WallD) Wall
SideWin --> Translate(0, 0, -WallD) SideRun
SideRun --> Split(X) { ~1: Wall | 1.2: SideCol | ~1: Wall }
SideCol --> Split(Y) { 1.0: Wall | 1.5: Glazing | ~1: Wall }
FrontInset --> Split(X) { WallD: NIL | ~1: FrontIn | WallD: NIL }
FrontIn --> Translate(0, 0, -WallD) FrontRun
FrontRun --> Split(X) { ~1: Wall | 1.1: DoorCol | 0.5: Wall | 1.2: Tall | ~1: Wall }
DoorCol --> Split(Y) { 0.5: Wall | 2.2: Leaf | ~1: Wall }
Leaf --> Extrude(WallD - 0.16) Mat(\"DoorWood\") I(\"Door\")
Tall --> Split(Y) { 0.5: Wall | ~1: Glazing | 0.5: Wall }
BackInset --> Split(X) { WallD: NIL | ~1: BackIn | WallD: NIL }
BackIn --> Translate(0, 0, -WallD) Wall
";

    /// A two-storey low building's fit, homes on its ground floor.
    const FIT: StreetFit = StreetFit::new(12, 9, 2, false);

    fn says(problems: &[String], what: &str) -> bool {
        problems.iter().any(|p| p.contains(what))
    }

    #[test]
    fn a_stretched_window_a_party_wall_window_and_an_open_top_are_caught() {
        let problems = conventions(&LOW, STRETCHED, FIT, None);
        assert!(says(&problems, "storey 1 has no window"), "{problems:#?}");
        assert!(says(&problems, "storey 2 has no window"), "{problems:#?}");
        assert!(says(&problems, "faces a party wall"), "{problems:#?}");
        assert!(says(&problems, "open to the sky"), "{problems:#?}");
        assert!(says(&problems, "no I(\"Trim\") draws it"), "{problems:#?}");
    }

    #[test]
    fn a_part_with_no_material_is_caught() {
        let bare = STRETCHED.replace("Mat(\"Lime\") ", "");
        let problems = conventions(&LOW, &bare, FIT, None);
        assert!(says(&problems, "has no Mat above it"), "{problems:#?}");
        assert!(!says(
            &conventions(&LOW, STRETCHED, FIT, None),
            "has no Mat"
        ));
    }

    /// The modern city's own low building keeps them all, as the fleet's
    /// tests hold every street building to them.
    #[test]
    fn the_reference_keeps_the_conventions() {
        assert_eq!(
            conventions(&LOW, LOW.rules, FIT, None),
            Vec::<String>::new()
        );
    }
}
