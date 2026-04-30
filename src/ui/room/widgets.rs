//! Shared egui widgets and helpers used across every editor tab: fixed-point
//! slider, u32/u64 drag, RGB/RGBA colour pickers, generator-kind combo, the
//! transform editor, unique-key helpers, and the ternary-tree L-system
//! preset factory.

use bevy_egui::egui;

use crate::pds::{
    Fp, Fp3, Fp4, Fp64, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignGeneratorKind,
    SovereignLeafConfig, SovereignMaterialSettings, SovereignTextureConfig, SovereignTwigConfig,
    TransformData,
};

pub(super) fn draw_transform(ui: &mut egui::Ui, t: &mut TransformData, dirty: &mut bool) {
    ui.label("Translation");
    let mut tr = t.translation.0;
    ui.horizontal(|ui| {
        for v in tr.iter_mut() {
            if ui.add(egui::DragValue::new(v).speed(0.5)).changed() {
                *dirty = true;
            }
        }
    });
    t.translation = Fp3(tr);

    ui.label("Scale");
    let mut sc = t.scale.0;
    ui.horizontal(|ui| {
        for v in sc.iter_mut() {
            if ui
                .add(egui::DragValue::new(v).speed(0.05).range(0.01..=1000.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
    t.scale = Fp3(sc);

    ui.label("Rotation (quaternion xyzw)");
    let mut rot = t.rotation.0;
    ui.horizontal(|ui| {
        for v in rot.iter_mut() {
            if ui.add(egui::DragValue::new(v).speed(0.01)).changed() {
                *dirty = true;
            }
        }
    });
    t.rotation = Fp4(rot);
}

pub(super) fn draw_transform_no_scale(ui: &mut egui::Ui, t: &mut TransformData, dirty: &mut bool) {
    ui.label("Translation");
    let mut tr = t.translation.0;
    ui.horizontal(|ui| {
        if ui
            .add(egui::DragValue::new(&mut tr[0]).speed(0.5))
            .changed()
        {
            *dirty = true;
        }
        if ui
            .add(egui::DragValue::new(&mut tr[1]).speed(0.5))
            .changed()
        {
            *dirty = true;
        }
        if ui
            .add(egui::DragValue::new(&mut tr[2]).speed(0.5))
            .changed()
        {
            *dirty = true;
        }
    });
    t.translation = Fp3(tr);

    ui.label("Rotation (quaternion xyzw)");
    let mut rot = t.rotation.0;
    ui.horizontal(|ui| {
        if ui
            .add(egui::DragValue::new(&mut rot[0]).speed(0.01))
            .changed()
        {
            *dirty = true;
        }
        if ui
            .add(egui::DragValue::new(&mut rot[1]).speed(0.01))
            .changed()
        {
            *dirty = true;
        }
        if ui
            .add(egui::DragValue::new(&mut rot[2]).speed(0.01))
            .changed()
        {
            *dirty = true;
        }
        if ui
            .add(egui::DragValue::new(&mut rot[3]).speed(0.01))
            .changed()
        {
            *dirty = true;
        }
    });
    t.rotation = Fp4(rot);

    ui.label(
        egui::RichText::new(format!(
            "Scale: {:.2} x {:.2} x {:.2} (Configure scale in Generator)",
            t.scale.0[0], t.scale.0[1], t.scale.0[2]
        ))
        .small()
        .color(egui::Color32::GRAY),
    );
}

pub(super) fn fp_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Fp,
    lo: f32,
    hi: f32,
    dirty: &mut bool,
) {
    let mut v = value.0;
    if ui
        .add(egui::Slider::new(&mut v, lo..=hi).text(label))
        .changed()
    {
        *value = Fp(v);
        *dirty = true;
    }
}

pub(super) fn drag_u32(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u32,
    lo: u32,
    hi: u32,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.add(egui::DragValue::new(value).range(lo..=hi)).changed() {
            *dirty = true;
        }
    });
}

pub(super) fn drag_u64(ui: &mut egui::Ui, label: &str, value: &mut u64, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.add(egui::DragValue::new(value)).changed() {
            *dirty = true;
        }
    });
}

pub(super) fn color_picker(ui: &mut egui::Ui, label: &str, value: &mut Fp3, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut rgb = value.0;
        if ui.color_edit_button_rgb(&mut rgb).changed() {
            *value = Fp3(rgb);
            *dirty = true;
        }
    });
}

/// RGBA colour picker — mirrors [`color_picker`] but for [`Fp4`] fields
/// where the alpha channel carries renderer-relevant information (fog
/// opacity, sun-glow strength). Uses the unmultiplied variant so the
/// alpha edits independently of RGB rather than being pre-scaled.
pub(super) fn color_picker_rgba(ui: &mut egui::Ui, label: &str, value: &mut Fp4, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut rgba = value.0;
        if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
            *value = Fp4(rgba);
            *dirty = true;
        }
    });
}

pub(super) fn kind_combo(ui: &mut egui::Ui, kind: &mut SovereignGeneratorKind) -> bool {
    let mut changed = false;
    egui::ComboBox::from_label("Kind")
        .selected_text(match kind {
            SovereignGeneratorKind::FbmNoise => "FBM Noise",
            SovereignGeneratorKind::DiamondSquare => "Diamond Square",
            SovereignGeneratorKind::VoronoiTerracing => "Voronoi Terracing",
        })
        .show_ui(ui, |ui| {
            changed |= ui
                .selectable_value(kind, SovereignGeneratorKind::FbmNoise, "FBM Noise")
                .changed();
            changed |= ui
                .selectable_value(
                    kind,
                    SovereignGeneratorKind::DiamondSquare,
                    "Diamond Square",
                )
                .changed();
            changed |= ui
                .selectable_value(
                    kind,
                    SovereignGeneratorKind::VoronoiTerracing,
                    "Voronoi Terracing",
                )
                .changed();
        });
    changed
}

pub(super) fn generator_combo(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    names: &[String],
    dirty: &mut bool,
) {
    egui::ComboBox::from_label(label)
        .selected_text(value.clone())
        .show_ui(ui, |ui| {
            for n in names {
                if ui.selectable_value(value, n.clone(), n).changed() {
                    *dirty = true;
                }
            }
        });
}

pub(super) fn unique_key<T>(map: &std::collections::HashMap<String, T>, prefix: &str) -> String {
    let mut n = 0;
    loop {
        let key = if n == 0 {
            prefix.to_string()
        } else {
            format!("{prefix}_{n}")
        };
        if !map.contains_key(&key) {
            return key;
        }
        n += 1;
    }
}

/// "Ternary Tree (+Props +Materials +Variations)" preset, ported verbatim
/// from `lsystem-explorer`. Ships with three material slots (bark / twig /
/// leaf) pre-wired to procedural textures, plus a prop-mapping table so the
/// `B` terminals become leaf billboards and `~(0)` props become twig cards.
/// Used by the per-node kind picker that swaps an existing node's variant
/// in place without touching its transform/children, and by the "+ New"
/// menu in the Generators tab via [`super::construct::make_default_for_kind`].
pub(super) fn default_lsystem_kind() -> GeneratorKind {
    let mut materials = std::collections::HashMap::new();

    materials.insert(
        0,
        SovereignMaterialSettings {
            base_color: Fp3([0.35, 0.2, 0.08]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Bark(SovereignBarkConfig::default()),
            ..Default::default()
        },
    );
    materials.insert(
        1,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(1.0),
            texture: SovereignTextureConfig::Twig(SovereignTwigConfig::default()),
            ..Default::default()
        },
    );
    materials.insert(
        2,
        SovereignMaterialSettings {
            base_color: Fp3([1.0, 1.0, 1.0]),
            roughness: Fp(0.6),
            texture: SovereignTextureConfig::Leaf(SovereignLeafConfig::default()),
            ..Default::default()
        },
    );

    let mut prop_mappings = std::collections::HashMap::new();
    prop_mappings.insert(0, PropMeshType::Twig);
    prop_mappings.insert(1, PropMeshType::Leaf);

    GeneratorKind::LSystem {
        source_code: "#define d1 180\n#define th 0.035\n#define d2 252\n#define a 36\n#define lr 1.12\n#define vr 1.532\n#define ps 60.0\n#define s 0.5\n#define ir 10.0\nomega: C(0.0)!(th)F(4*s)/(45)A[B]\np0: A : 0.7 -> !(th*vr)F(s)[&(a)F(s)A[B]]/(d1)[&(a)F(s)A[B]]/(d2)[&(a)F(s)A[B]]\np1: A : 0.3 -> !(th*vr)F(s)A[B]\np2: F(l) : * -> F(l*lr)\np3: !(w) : * -> !(w*vr)\np4: B : * -> \np5: B -> \np6: C(x) : 0.7 -> C(x)\np7: C(x) : 0.3 -> C(x-ir)".to_string(),
        finalization_code: "p8: B : * -> ,(1)~(0,ps)\np9: C(x) : * -> /(x)".to_string(),
        iterations: 6,
        seed: 1,
        angle: Fp(36.0),
        step: Fp(1.0),
        width: Fp(0.1),
        elasticity: Fp(0.05),
        tropism: Some(Fp3([0.0, -1.0, 0.0])),
        materials,
        prop_mappings,
        prop_scale: Fp(0.04),
        mesh_resolution: 8,
    }
}

/// Default starter preset for a freshly added Shape generator. A detailed
/// modern villa adapted from `bevy_symbios_shape`'s `detailed_villa` example —
/// a two-storey brick / stucco main house with a gable shingle roof, attached
/// metal-roofed garage, paver driveway, and wood deck. The full material
/// palette (brick / stucco / concrete / shingle / metal / glass / wood /
/// pavers / grass) is wired up so the fallback render shows something
/// architecturally legible out of the box. Used by the per-node kind picker
/// and by the "+ New" menu in the Generators tab via
/// [`super::construct::make_default_for_kind`].
pub(super) fn default_shape_kind() -> GeneratorKind {
    use crate::pds::{
        SovereignBrickConfig, SovereignConcreteConfig, SovereignGroundConfig, SovereignMetalConfig,
        SovereignPaversConfig, SovereignPlankConfig, SovereignShingleConfig, SovereignStuccoConfig,
        SovereignWindowConfig,
    };

    let mut materials = std::collections::HashMap::new();

    materials.insert(
        "Brick".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.5, 0.25, 0.15]),
            roughness: Fp(0.9),
            uv_scale: Fp(2.0),
            texture: SovereignTextureConfig::Brick(SovereignBrickConfig {
                aspect_ratio: Fp64(3.0),
                color_brick: Fp3([0.45, 0.22, 0.15]),
                scale: Fp64(8.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    materials.insert(
        "Stucco".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.88, 0.84, 0.78]),
            roughness: Fp(0.8),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Stucco(SovereignStuccoConfig {
                color_base: Fp3([0.87, 0.83, 0.77]),
                roughness: Fp64(0.35),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    materials.insert(
        "Concrete".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.6, 0.6, 0.6]),
            roughness: Fp(0.85),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Concrete(SovereignConcreteConfig {
                formwork_lines: Fp64(3.0),
                formwork_depth: Fp64(0.1),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    materials.insert(
        "Shingle".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.2, 0.2, 0.25]),
            roughness: Fp(0.8),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Shingle(SovereignShingleConfig::default()),
            ..Default::default()
        },
    );

    materials.insert(
        "Metal".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.18, 0.18, 0.2]),
            roughness: Fp(0.3),
            metallic: Fp(0.85),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Metal(SovereignMetalConfig {
                style: bevy_symbios_texture::metal::MetalStyle::StandingSeam,
                seam_count: Fp64(6.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    materials.insert(
        "Glass".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.1, 0.2, 0.3]),
            roughness: Fp(0.05),
            metallic: Fp(0.9),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Window(SovereignWindowConfig {
                panes_x: 2,
                panes_y: 2,
                frame_width: Fp64(0.1),
                glass_opacity: Fp64(0.3),
                mullion_thickness: Fp64(0.12),
                corner_radius: Fp64(0.18),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    materials.insert(
        "Wood".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.38, 0.22, 0.12]),
            roughness: Fp(0.6),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Plank(SovereignPlankConfig {
                color_wood_light: Fp3([0.4, 0.24, 0.14]),
                color_wood_dark: Fp3([0.22, 0.12, 0.06]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    materials.insert(
        "Pavers".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.5, 0.48, 0.45]),
            roughness: Fp(0.85),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Pavers(SovereignPaversConfig::default()),
            ..Default::default()
        },
    );

    materials.insert(
        "Grass".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.2, 0.35, 0.15]),
            roughness: Fp(0.9),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Ground(SovereignGroundConfig {
                color_dry: Fp3([0.3, 0.4, 0.2]),
                color_moist: Fp3([0.15, 0.25, 0.1]),
                macro_scale: Fp64(4.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    // Grammar adapted from `bevy_symbios_shape/examples/detailed_villa.rs`.
    // Footprint is 20 × 16 (Lot splits 14:HouseMass | 6:GarageMass on X;
    // each mass splits 3+13 / 4+12 on Z).
    let grammar_source = [
        // ── 1. Massing ──
        "Lot --> Split(X) { 14: HouseMass | 6: GarageMass }",
        "HouseMass --> Split(Z) { 3: DeckArea | 13: MainHouse }",
        "GarageMass --> Split(Z) { 4: Driveway | 12: GarageStruct }",
        // ── 2. Platforms & ground ──
        "DeckArea --> Extrude(0.3) Mat(\"Wood\") I(\"Deck\")",
        "Driveway --> Extrude(0.1) Mat(\"Pavers\") I(\"Drive\")",
        // ── 3. Main house volume ──
        "MainHouse --> Extrude(9.5) Split(Y) { 3.5: GroundFloor | 0.3: BeltCourse | 3.2: UpperFloor | 0.3: RoofFascia | 2.2: MainRoof }",
        // ── 4. Garage volume ──
        "GarageStruct --> Extrude(4.0) Split(Y) { 3.5: GarageBody | 0.5: GarageRoof }",
        // ── 5. Roofs ──
        "MainRoof --> Roof(Gable, 30) { Slope: ShingleSlope | GableEnd: GableWall }",
        "ShingleSlope --> Mat(\"Shingle\") I(\"RoofTile\")",
        "GableWall --> Mat(\"Stucco\") I(\"Wall\")",
        "GarageRoof --> Comp(Faces) { Top: FlatRoof | Side: GarageFascia }",
        "GarageFascia --> Extrude(0.1) Mat(\"Metal\") I(\"Fascia\")",
        "FlatRoof --> Mat(\"Metal\") I(\"GarageRoofTile\")",
        "BeltCourse --> Comp(Faces) { Side: BeltFace }",
        "BeltFace --> Extrude(0.25) Mat(\"Concrete\") I(\"Trim\")",
        "RoofFascia --> Comp(Faces) { Side: FasciaFace }",
        "FasciaFace --> Extrude(0.05) Mat(\"Metal\") I(\"Fascia\")",
        // ── 6. Facades ──
        "GroundFloor --> Comp(Faces) { Front: FrontEntryFacade | Back: SideFacade | Left: SideFacade | Right: SideFacade }",
        "FrontEntryFacade --> Split(X) { 1.5: BrickWall | 2.5: EntryDoor | 1.0: BrickWall | 4.0: PictureWindow | ~1: BrickWall }",
        "SideFacade --> Repeat(X, 4.0) { SideBay }",
        "SideBay --> Split(X) { ~1: BrickWall | 2.0: StandardWindowBrick | ~1: BrickWall }",
        "UpperFloor --> Comp(Faces) { Side: UpperFacade }",
        "UpperFacade --> Repeat(X, 3.5) { UpperBay }",
        "UpperBay --> Split(X) { ~1: StuccoWall | 1.5: StandardWindowStucco | ~1: StuccoWall }",
        "GarageBody --> Comp(Faces) { Front: GarageFront | Back: BrickWall | Left: BrickWall | Right: BrickWall }",
        "GarageFront --> Split(X) { ~1: BrickWall | 5.0: GarageDoor | ~1: BrickWall }",
        // ── 7. Windows & walls ──
        "StandardWindowBrick --> Split(Y) { 0.9: BrickWall | 1.6: WinAssembly | ~1: BrickWall }",
        "StandardWindowStucco --> Split(Y) { 0.9: StuccoWall | 1.6: WinAssembly | ~1: StuccoWall }",
        "PictureWindow --> Split(Y) { 0.8: BrickWall | 2.2: WinAssembly | ~1: BrickWall }",
        "WinAssembly --> Split(X) { 0.15: ConcreteFrame | ~1: WinCenter | 0.15: ConcreteFrame }",
        "WinCenter --> Split(Y) { 0.15: ConcreteFrame | ~1: GlassPane | 0.15: ConcreteFrame }",
        "ConcreteFrame --> Extrude(0.25) Mat(\"Concrete\") I(\"Frame\")",
        "GlassPane --> Extrude(0.05) Mat(\"Glass\") I(\"Pane\")",
        "EntryDoor --> Split(Y) { 2.4: DoorAssembly | ~1: BrickWall }",
        "DoorAssembly --> Split(X) { 0.15: ConcreteFrame | ~1: DoorPanel | 0.15: ConcreteFrame }",
        "DoorPanel --> Split(Y) { ~1: WoodPanel | 0.15: ConcreteFrame }",
        "WoodPanel --> Extrude(0.1) Mat(\"Wood\") I(\"Door\")",
        "GarageDoor --> Split(Y) { 2.5: GaragePanel | ~1: BrickWall }",
        "GaragePanel --> Extrude(0.1) Mat(\"Metal\") I(\"GDoor\")",
        "BrickWall --> Extrude(0.2) Mat(\"Brick\") I(\"Wall\")",
        "StuccoWall --> Extrude(0.2) Mat(\"Stucco\") I(\"Wall\")",
    ]
    .join("\n");

    GeneratorKind::Shape {
        grammar_source,
        root_rule: "Lot".to_string(),
        footprint: Fp3([20.0, 0.0, 16.0]),
        seed: 99,
        materials,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Walk the default villa grammar through the same `parse_rule` /
    /// `add_weighted_rules` path the runtime uses, then derive against the
    /// default footprint. Catches typos and ensures every `Mat("...")` slot
    /// referenced in the grammar has a matching entry in the materials map
    /// — without this, a hand-edit that drops a slot or breaks a rule would
    /// only surface as a runtime warning the first time someone places the
    /// generator in a room.
    #[test]
    fn default_shape_kind_grammar_parses_and_derives() {
        use std::collections::HashSet;
        use symbios_shape::grammar::parse_rule;
        use symbios_shape::{Interpreter, Quat as SQuat, Scope, Vec3 as SVec3};

        let GeneratorKind::Shape {
            grammar_source,
            root_rule,
            footprint,
            seed,
            materials,
        } = default_shape_kind()
        else {
            panic!("default_shape_kind must return GeneratorKind::Shape");
        };

        let mut interp = Interpreter::new();
        interp.seed = seed;
        let mut referenced_mats: HashSet<String> = HashSet::new();

        for (i, raw) in grammar_source.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let rule = parse_rule(line)
                .unwrap_or_else(|e| panic!("villa rule line {} failed to parse: {}", i + 1, e));
            // Collect every material name the rule mentions so we can
            // assert the default materials map covers all of them.
            for mat in line
                .split("Mat(\"")
                .skip(1)
                .filter_map(|chunk| chunk.split('"').next())
            {
                referenced_mats.insert(mat.to_string());
            }
            interp
                .add_weighted_rules(&rule.name, rule.variants)
                .unwrap_or_else(|e| panic!("villa rule {} rejected: {}", rule.name, e));
        }

        assert!(
            interp.has_rule(&root_rule),
            "root rule `{}` missing from villa grammar",
            root_rule
        );
        for name in &referenced_mats {
            assert!(
                materials.contains_key(name),
                "villa grammar references Mat(\"{}\") but no material slot is defined",
                name
            );
        }

        let scope = Scope::new(
            SVec3::ZERO,
            SQuat::IDENTITY,
            SVec3::new(
                footprint.0[0] as f64,
                footprint.0[1] as f64,
                footprint.0[2] as f64,
            ),
        );
        let model = interp
            .derive(scope, &root_rule)
            .expect("villa grammar must derive against its default footprint");
        assert!(
            !model.terminals.is_empty(),
            "villa derivation produced zero terminals — default footprint is starving the splits"
        );
    }
}
