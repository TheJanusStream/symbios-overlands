//! Shared egui widgets and helpers used across every editor tab: fixed-point
//! slider, u32/u64 drag, RGB/RGBA colour pickers, generator-kind combo, the
//! transform editor, unique-key helpers, and the ternary-tree L-system
//! preset factory.

use bevy_egui::egui;

use crate::pds::{
    Fp, Fp3, Fp4, GeneratorKind, PropMeshType, SovereignBarkConfig, SovereignGeneratorKind,
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

/// Default starter preset for a freshly added Shape generator. A 3-storey
/// columnar building shipped verbatim from `symbios-shape`'s
/// `simple_building` example, plus matching brick / glass material slots so
/// the procedural mesh fallback renders something visually distinct out of
/// the box. Used by the per-node kind picker and by the "+ New" menu in
/// the Generators tab via [`super::construct::make_default_for_kind`].
pub(super) fn default_shape_kind() -> GeneratorKind {
    use crate::pds::{
        SovereignBrickConfig, SovereignShingleConfig, SovereignStuccoConfig, SovereignWindowConfig,
    };

    let mut materials = std::collections::HashMap::new();
    materials.insert(
        "Brick".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.78, 0.34, 0.22]),
            roughness: Fp(0.85),
            uv_scale: Fp(2.0),
            texture: SovereignTextureConfig::Brick(SovereignBrickConfig::default()),
            ..Default::default()
        },
    );
    materials.insert(
        "Stucco".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.92, 0.88, 0.78]),
            roughness: Fp(0.9),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Stucco(SovereignStuccoConfig::default()),
            ..Default::default()
        },
    );
    materials.insert(
        "Window".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.6, 0.75, 0.85]),
            roughness: Fp(0.15),
            metallic: Fp(0.1),
            uv_scale: Fp(1.0),
            texture: SovereignTextureConfig::Window(SovereignWindowConfig::default()),
            ..Default::default()
        },
    );
    materials.insert(
        "Roof".to_string(),
        SovereignMaterialSettings {
            base_color: Fp3([0.45, 0.28, 0.22]),
            roughness: Fp(0.95),
            uv_scale: Fp(1.5),
            texture: SovereignTextureConfig::Shingle(SovereignShingleConfig::default()),
            ..Default::default()
        },
    );

    GeneratorKind::Shape {
        grammar_source: "Lot --> Extrude(9) Split(Y) { 3: Ground | ~1: Floor | 2: Cap }\n\
            Ground --> Mat(\"Stucco\") Split(X) { 1.2: Pier | ~1: Bay | 1.2: Pier }\n\
            Floor --> Mat(\"Brick\") Repeat(X, 2.5) { Bay }\n\
            Bay --> Split(X) { ~1: Wall | 1.5: WindowPanel | ~1: Wall }\n\
            WindowPanel --> Mat(\"Window\") I(\"Window\")\n\
            Cap --> Mat(\"Roof\") Taper(0.7) I(\"Roof\")"
            .to_string(),
        root_rule: "Lot".to_string(),
        footprint: Fp3([12.0, 0.0, 8.0]),
        seed: 1,
        materials,
    }
}
