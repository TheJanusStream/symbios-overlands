//! Material & splat widgets plus the unified texture-bridge dispatcher.
//! Handles both terrain's four-layer `SovereignMaterialConfig` (rules +
//! layers) and per-slot `SovereignTextureConfig` editing (bark/leaf/twig,
//! brick, plank, shingle, and the rest of the `bevy_symbios_texture` family).

use bevy_egui::egui;

use crate::pds::{
    Fp, Fp2, SovereignAshlarConfig, SovereignAsphaltConfig, SovereignBarkConfig,
    SovereignBrickConfig, SovereignBroadleafConfig, SovereignCactusSkinConfig,
    SovereignChainLinkConfig, SovereignChitinConfig, SovereignCobblestoneConfig,
    SovereignConcreteConfig, SovereignCorrugatedConfig, SovereignCrackedEarthConfig,
    SovereignEnamelConfig, SovereignEncausticConfig, SovereignFabricConfig, SovereignFlameConfig,
    SovereignFlowerConfig, SovereignForestFloorConfig, SovereignFrondConfig,
    SovereignGrassTuftConfig, SovereignGravelConfig, SovereignGroundConfig, SovereignIceConfig,
    SovereignIronGrilleConfig, SovereignLavaConfig, SovereignLeafConfig, SovereignLeafSpriteConfig,
    SovereignLichenConfig, SovereignLogEndConfig, SovereignMarbleConfig, SovereignMaterialConfig,
    SovereignMaterialSettings, SovereignMetalConfig, SovereignMossConfig, SovereignNeedleConfig,
    SovereignObsidianConfig, SovereignParquetConfig, SovereignPaversConfig, SovereignPetalConfig,
    SovereignPlankConfig, SovereignPuffConfig, SovereignReedConfig, SovereignRingConfig,
    SovereignRockConfig, SovereignSandConfig, SovereignShardConfig, SovereignShingleConfig,
    SovereignSnowConfig, SovereignSnowflakeConfig, SovereignSoftDiscConfig,
    SovereignSolarPanelConfig, SovereignSparkConfig, SovereignSplatRule,
    SovereignStainedGlassConfig, SovereignStuccoConfig, SovereignTextureConfig,
    SovereignThatchConfig, SovereignTruchetConfig, SovereignTwigConfig, SovereignWainscotingConfig,
    SovereignWindowConfig,
};

use super::widgets::{drag_u32, fp_slider};

pub(super) fn draw_material_forge(
    ui: &mut egui::Ui,
    mat: &mut SovereignMaterialConfig,
    dirty: &mut bool,
    assets: &mut super::assets::AssetPanel<'_>,
) {
    // #1268 f66: this whole forge carried no hover text and no prose.
    drag_u32(ui, "Texture size", &mut mat.texture_size, 16, 4096, dirty).on_hover_text(
        "Pixels across one generated texture. Bigger is sharper up close and \
         costs memory on every device that loads this world.",
    );
    fp_slider(ui, "Tile scale", &mut mat.tile_scale, 1.0, 500.0, dirty).on_hover_text(
        "How many times the texture repeats across the terrain. Higher packs the \
         pattern tighter; too high and it reads as noise.",
    );

    // Canonical palette labels for the R/G/B/A splat channels. Users may
    // swap any layer for a different texture generator via the per-layer
    // bridge; the labels stay fixed because the splat rules are indexed
    // by channel, not by content.
    let labels = ["Grass (R)", "Dirt (G)", "Rock (B)", "Snow (A)"];
    for (i, label) in labels.iter().enumerate() {
        egui::CollapsingHeader::new(format!("{} rule", label))
            .default_open(false)
            .show(ui, |ui| {
                draw_splat_rule(ui, &mut mat.rules[i], dirty);
            });
    }

    for (i, label) in labels.iter().enumerate() {
        egui::CollapsingHeader::new(format!("{} texture", label))
            .default_open(false)
            .show(ui, |ui| {
                draw_texture_bridge(
                    ui,
                    &mut mat.layers[i],
                    &format!("terrain_layer_{}", i),
                    dirty,
                    assets,
                );
                // Which of the four layers this is matters to the status
                // line: a Referenced layer that fails renders a convincing
                // procedural ground, so "did MY image load" is a question
                // only the panel can answer (#1246 f347).
                if let SovereignTextureConfig::Referenced { source } = &mat.layers[i] {
                    let status = assets.terrain_layer(i, source);
                    if super::assets::asset_status_row(ui, status, assets.now) {
                        assets.retry(
                            crate::world_builder::asset_failure::AssetRetry::TerrainLayer(i),
                        );
                    }
                }
            });
    }
}

fn draw_splat_rule(ui: &mut egui::Ui, rule: &mut SovereignSplatRule, dirty: &mut bool) {
    // Bounded against each other (#1238 f90). These two pairs are the
    // WORST of the four: `SovereignSplatRule` has no `Sanitize` impl
    // anywhere, so an inverted band was never corrected and never
    // flagged — it simply matched nothing, for good.
    // The band this layer paints in, in fractions of the terrain's full
    // range — not metres (#1268 f66).
    ui.label(
        egui::RichText::new(
            "Where this layer shows: 0 is the lowest ground in the world and 1 the \
             highest, 0 slope is flat and 1 is a cliff.",
        )
        .small()
        .color(crate::ui::theme::current(ui.ctx()).text_weak),
    );
    super::widgets::fp_range_sliders(
        ui,
        "Height min",
        "Height max",
        &mut rule.height_min,
        &mut rule.height_max,
        0.0,
        1.0,
        dirty,
    );
    super::widgets::fp_range_sliders(
        ui,
        "Slope min",
        "Slope max",
        &mut rule.slope_min,
        &mut rule.slope_max,
        0.0,
        1.0,
        dirty,
    );
    fp_slider(ui, "Sharpness", &mut rule.sharpness, 0.05, 8.0, dirty).on_hover_text(
        "How abruptly this layer gives way at the edges of its band. Low blends \
         into its neighbours; high draws a hard line.",
    );
}

/// The `uv_transform` rows every material editor shares (#957): pattern
/// slide and spin in degrees CCW.
///
/// Factored out because every material editor — the primitive one in
/// `construct::draw_universal_material`, the L-system and Shape slot lists,
/// and the Sign panel (#964) — edits the same
/// [`SovereignMaterialSettings`] and all of them flow to the same
/// `world_builder::material::sovereign_uv_transform`. Both knobs ride the
/// material rather than the mesh, so dragging them re-keys only the
/// `StandardMaterial`; no mesh is rebuilt.
///
/// `offset_unit` names what the slide is measured in: **metres of surface**
/// for a metre-mapped prim, **spans** for a Sign, whose quad is normalised
/// so one unit is one panel width.
pub(super) fn draw_uv_transform_rows(
    ui: &mut egui::Ui,
    m: &mut SovereignMaterialSettings,
    offset_unit: &str,
    dirty: &mut bool,
) {
    let mut offset = m.uv_offset.0;
    ui.horizontal(|ui| {
        ui.label(format!("UV offset ({offset_unit})"));
        for v in offset.iter_mut() {
            if ui
                .add(
                    crate::ui::num::drag(v)
                        .speed(0.05)
                        .range(-1_000.0..=1_000.0),
                )
                .changed()
            {
                *dirty = true;
            }
        }
    });
    m.uv_offset = Fp2(offset);

    let mut rotation = m.uv_rotation.0;
    ui.horizontal(|ui| {
        ui.label("UV rotation (deg)");
        if ui
            .add(
                crate::ui::num::drag(&mut rotation)
                    .speed(1.0)
                    .range(-360.0..=360.0),
            )
            .changed()
        {
            *dirty = true;
        }
    });
    m.uv_rotation = Fp(rotation);
}

pub(super) fn draw_texture_bridge(
    ui: &mut egui::Ui,
    texture: &mut SovereignTextureConfig,
    salt: &str,
    dirty: &mut bool,
    assets: &mut super::assets::AssetPanel<'_>,
) {
    draw_texture_bridge_opts(ui, texture, salt, dirty, true, assets);
}

/// Body of [`draw_texture_bridge`] with an `allow_referenced` switch.
/// Particle emitters pass `false`: their procedural slot sits alongside a
/// dedicated legacy fetched-source picker, so the `Referenced` asset
/// pointer would be a confusing duplicate (and is inert on the particle
/// bake path anyway).
pub(super) fn draw_texture_bridge_opts(
    ui: &mut egui::Ui,
    texture: &mut SovereignTextureConfig,
    salt: &str,
    dirty: &mut bool,
    allow_referenced: bool,
    assets: &mut super::assets::AssetPanel<'_>,
) {
    // The list is 59 entries in a box egui scrolls at ~200 px, so about ten
    // are visible at a time and the only way to find "Truchet" was to know
    // where it was (#1250 f93). Two things fix that without moving anything:
    // a filter, and the headings the source comments have carried all along.
    let filter_id = ui.id().with((salt, "tex_filter"));
    let mut filter = ui
        .data_mut(|d| d.get_temp::<String>(filter_id))
        .unwrap_or_default();
    egui::ComboBox::from_id_salt(format!("{}_tex_ty", salt))
        .selected_text(texture.label())
        .show_ui(ui, |ui| {
            crate::ui::affordances::text_edit(
                ui,
                egui::TextEdit::singleline(&mut filter)
                    .hint_text("Filter…")
                    .desired_width(160.0),
            );
            let needle = filter.trim().to_ascii_lowercase();
            let filtering = !needle.is_empty();
            macro_rules! opt {
                ($label:literal, $expr:expr) => {{
                    let shown = !filtering || $label.to_ascii_lowercase().contains(&needle);
                    if shown {
                        let selected = texture.label() == $label;
                        if ui.selectable_label(selected, $label).clicked() && !selected {
                            *texture = $expr;
                            *dirty = true;
                        }
                    }
                }};
            }
            // A heading only earns its line when the whole list is showing;
            // under a filter it would be six labels over three results.
            macro_rules! group {
                ($label:literal) => {{
                    if !filtering {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new($label)
                                .small()
                                .strong()
                                .color(crate::ui::theme::current(ui.ctx()).text_weak),
                        );
                    }
                }};
            }
            opt!("None", SovereignTextureConfig::None);
            // Slotted between None and the procedural-generator list so
            // the existing 24-variant order stays contiguous — muscle
            // memory survives the addition.
            if allow_referenced {
                opt!(
                    "External image",
                    SovereignTextureConfig::Referenced {
                        source: Default::default()
                    }
                );
            }
            opt!("Leaf", SovereignTextureConfig::Leaf(Default::default()));
            opt!("Twig", SovereignTextureConfig::Twig(Default::default()));
            opt!("Bark", SovereignTextureConfig::Bark(Default::default()));
            opt!("Window", SovereignTextureConfig::Window(Default::default()));
            opt!(
                "Stained Glass",
                SovereignTextureConfig::StainedGlass(Default::default())
            );
            opt!(
                "Iron Grille",
                SovereignTextureConfig::IronGrille(Default::default())
            );
            opt!("Ground", SovereignTextureConfig::Ground(Default::default()));
            opt!("Rock", SovereignTextureConfig::Rock(Default::default()));
            opt!("Brick", SovereignTextureConfig::Brick(Default::default()));
            opt!("Plank", SovereignTextureConfig::Plank(Default::default()));
            opt!(
                "Shingle",
                SovereignTextureConfig::Shingle(Default::default())
            );
            opt!("Stucco", SovereignTextureConfig::Stucco(Default::default()));
            opt!(
                "Concrete",
                SovereignTextureConfig::Concrete(Default::default())
            );
            opt!("Metal", SovereignTextureConfig::Metal(Default::default()));
            opt!("Pavers", SovereignTextureConfig::Pavers(Default::default()));
            opt!("Ashlar", SovereignTextureConfig::Ashlar(Default::default()));
            opt!(
                "Cobblestone",
                SovereignTextureConfig::Cobblestone(Default::default())
            );
            opt!("Thatch", SovereignTextureConfig::Thatch(Default::default()));
            opt!("Marble", SovereignTextureConfig::Marble(Default::default()));
            opt!(
                "Corrugated",
                SovereignTextureConfig::Corrugated(Default::default())
            );
            opt!(
                "Asphalt",
                SovereignTextureConfig::Asphalt(Default::default())
            );
            opt!(
                "Wainscoting",
                SovereignTextureConfig::Wainscoting(Default::default())
            );
            opt!(
                "Encaustic",
                SovereignTextureConfig::Encaustic(Default::default())
            );
            // Particle sprite cards.
            group!("Particle sprite cards");
            opt!(
                "Soft Disc",
                SovereignTextureConfig::SoftDisc(Default::default())
            );
            opt!("Spark", SovereignTextureConfig::Spark(Default::default()));
            opt!(
                "Snowflake",
                SovereignTextureConfig::Snowflake(Default::default())
            );
            opt!("Puff", SovereignTextureConfig::Puff(Default::default()));
            opt!("Ring", SovereignTextureConfig::Ring(Default::default()));
            opt!("Petal", SovereignTextureConfig::Petal(Default::default()));
            opt!("Shard", SovereignTextureConfig::Shard(Default::default()));
            opt!(
                "Leaf Sprite",
                SovereignTextureConfig::LeafSprite(Default::default())
            );
            opt!("Flame", SovereignTextureConfig::Flame(Default::default()));
            opt!("Flower", SovereignTextureConfig::Flower(Default::default()));
            // Vegetation ground-cover / understory billboard cards.
            group!("Vegetation cards");
            opt!(
                "Grass Tuft",
                SovereignTextureConfig::GrassTuft(Default::default())
            );
            opt!("Frond", SovereignTextureConfig::Frond(Default::default()));
            opt!("Reed", SovereignTextureConfig::Reed(Default::default()));
            opt!("Needle", SovereignTextureConfig::Needle(Default::default()));
            opt!(
                "Broadleaf",
                SovereignTextureConfig::Broadleaf(Default::default())
            );
            opt!("Moss", SovereignTextureConfig::Moss(Default::default()));
            opt!("Lichen", SovereignTextureConfig::Lichen(Default::default()));
            // Additional tileable surfaces.
            group!("More surfaces");
            opt!("Fabric", SovereignTextureConfig::Fabric(Default::default()));
            opt!("Sand", SovereignTextureConfig::Sand(Default::default()));
            opt!("Snow", SovereignTextureConfig::Snow(Default::default()));
            opt!("Ice", SovereignTextureConfig::Ice(Default::default()));
            opt!("Lava", SovereignTextureConfig::Lava(Default::default()));
            opt!(
                "Cactus Skin",
                SovereignTextureConfig::CactusSkin(Default::default())
            );
            // Terrain surfaces added in bevy_symbios_texture 0.8.
            group!("Terrain surfaces");
            opt!(
                "Cracked Earth",
                SovereignTextureConfig::CrackedEarth(Default::default())
            );
            opt!("Gravel", SovereignTextureConfig::Gravel(Default::default()));
            opt!(
                "Forest Floor",
                SovereignTextureConfig::ForestFloor(Default::default())
            );
            // Catalogue surfaces added in bevy_symbios_texture 0.8.
            group!("Catalogue surfaces");
            opt!("Enamel", SovereignTextureConfig::Enamel(Default::default()));
            opt!(
                "Obsidian",
                SovereignTextureConfig::Obsidian(Default::default())
            );
            opt!("Chitin", SovereignTextureConfig::Chitin(Default::default()));
            opt!(
                "Solar Panel",
                SovereignTextureConfig::SolarPanel(Default::default())
            );
            opt!(
                "Parquet",
                SovereignTextureConfig::Parquet(Default::default())
            );
            opt!(
                "Truchet",
                SovereignTextureConfig::Truchet(Default::default())
            );
            // Alpha-masked mesh cards.
            group!("Alpha-masked cards");
            opt!(
                "Chain Link",
                SovereignTextureConfig::ChainLink(Default::default())
            );
            opt!(
                "Log End",
                SovereignTextureConfig::LogEnd(Default::default())
            );
        });

    ui.data_mut(|d| d.insert_temp(filter_id, filter));

    let id = egui::Id::new(salt);
    macro_rules! run {
        ($c:expr, $sov:ty, $editor:path) => {{
            let mut native = $c.to_native();
            let (wb, _regen) = $editor(ui, &mut native, id);
            if wb {
                *$c = <$sov>::from_native(&native);
                *dirty = true;
            }
        }};
    }

    match texture {
        SovereignTextureConfig::None => {}
        // #1251 f87: an empty arm under the bare word "Unknown" left the
        // owner with an unexplained blank panel and, reasonably, a click —
        // permanently replacing content a newer client could still have
        // rendered.
        SovereignTextureConfig::Unknown => {
            super::widgets::unrecognised_value_line(
                ui,
                "texture",
                Some("the surface shows its flat colour instead"),
            );
        }
        // The one entry in the list whose result arrives over the network
        // and can silently never arrive (#1251 f354). The caption states
        // what the flat colour means, so it reads as a state rather than a
        // mystery; the status row under the field (#1246) says which state.
        SovereignTextureConfig::Referenced { source } => {
            ui.label(
                egui::RichText::new(
                    "The surface shows its flat colour until the image \
                     loads, and keeps showing it if the image cannot be \
                     fetched.",
                )
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
            super::widgets::draw_asset_reference_editor(
                ui,
                source,
                salt,
                dirty,
                super::widgets::ReferenceClass::Texture,
                assets,
            );
        }
        SovereignTextureConfig::Leaf(c) => run!(
            c,
            SovereignLeafConfig,
            bevy_symbios_texture::ui::leaf_config_editor
        ),
        SovereignTextureConfig::Twig(c) => run!(
            c,
            SovereignTwigConfig,
            bevy_symbios_texture::ui::twig_config_editor
        ),
        SovereignTextureConfig::Bark(c) => run!(
            c,
            SovereignBarkConfig,
            bevy_symbios_texture::ui::bark_config_editor
        ),
        SovereignTextureConfig::Window(c) => run!(
            c,
            SovereignWindowConfig,
            bevy_symbios_texture::ui::window_config_editor
        ),
        SovereignTextureConfig::StainedGlass(c) => run!(
            c,
            SovereignStainedGlassConfig,
            bevy_symbios_texture::ui::stained_glass_config_editor
        ),
        SovereignTextureConfig::IronGrille(c) => run!(
            c,
            SovereignIronGrilleConfig,
            bevy_symbios_texture::ui::iron_grille_config_editor
        ),
        SovereignTextureConfig::Ground(c) => run!(
            c,
            SovereignGroundConfig,
            bevy_symbios_texture::ui::ground_config_editor
        ),
        SovereignTextureConfig::Rock(c) => run!(
            c,
            SovereignRockConfig,
            bevy_symbios_texture::ui::rock_config_editor
        ),
        SovereignTextureConfig::Brick(c) => run!(
            c,
            SovereignBrickConfig,
            bevy_symbios_texture::ui::brick_config_editor
        ),
        SovereignTextureConfig::Plank(c) => run!(
            c,
            SovereignPlankConfig,
            bevy_symbios_texture::ui::plank_config_editor
        ),
        SovereignTextureConfig::Shingle(c) => run!(
            c,
            SovereignShingleConfig,
            bevy_symbios_texture::ui::shingle_config_editor
        ),
        SovereignTextureConfig::Stucco(c) => run!(
            c,
            SovereignStuccoConfig,
            bevy_symbios_texture::ui::stucco_config_editor
        ),
        SovereignTextureConfig::Concrete(c) => run!(
            c,
            SovereignConcreteConfig,
            bevy_symbios_texture::ui::concrete_config_editor
        ),
        SovereignTextureConfig::Metal(c) => run!(
            c,
            SovereignMetalConfig,
            bevy_symbios_texture::ui::metal_config_editor
        ),
        SovereignTextureConfig::Pavers(c) => run!(
            c,
            SovereignPaversConfig,
            bevy_symbios_texture::ui::pavers_config_editor
        ),
        SovereignTextureConfig::Ashlar(c) => run!(
            c,
            SovereignAshlarConfig,
            bevy_symbios_texture::ui::ashlar_config_editor
        ),
        SovereignTextureConfig::Cobblestone(c) => run!(
            c,
            SovereignCobblestoneConfig,
            bevy_symbios_texture::ui::cobblestone_config_editor
        ),
        SovereignTextureConfig::Thatch(c) => run!(
            c,
            SovereignThatchConfig,
            bevy_symbios_texture::ui::thatch_config_editor
        ),
        SovereignTextureConfig::Marble(c) => run!(
            c,
            SovereignMarbleConfig,
            bevy_symbios_texture::ui::marble_config_editor
        ),
        SovereignTextureConfig::Corrugated(c) => run!(
            c,
            SovereignCorrugatedConfig,
            bevy_symbios_texture::ui::corrugated_config_editor
        ),
        SovereignTextureConfig::Asphalt(c) => run!(
            c,
            SovereignAsphaltConfig,
            bevy_symbios_texture::ui::asphalt_config_editor
        ),
        SovereignTextureConfig::Wainscoting(c) => run!(
            c,
            SovereignWainscotingConfig,
            bevy_symbios_texture::ui::wainscoting_config_editor
        ),
        SovereignTextureConfig::Encaustic(c) => run!(
            c,
            SovereignEncausticConfig,
            bevy_symbios_texture::ui::encaustic_config_editor
        ),
        SovereignTextureConfig::SoftDisc(c) => run!(
            c,
            SovereignSoftDiscConfig,
            bevy_symbios_texture::ui::soft_disc_config_editor
        ),
        SovereignTextureConfig::Spark(c) => run!(
            c,
            SovereignSparkConfig,
            bevy_symbios_texture::ui::spark_config_editor
        ),
        SovereignTextureConfig::Snowflake(c) => run!(
            c,
            SovereignSnowflakeConfig,
            bevy_symbios_texture::ui::snowflake_config_editor
        ),
        SovereignTextureConfig::Puff(c) => run!(
            c,
            SovereignPuffConfig,
            bevy_symbios_texture::ui::puff_config_editor
        ),
        SovereignTextureConfig::Ring(c) => run!(
            c,
            SovereignRingConfig,
            bevy_symbios_texture::ui::ring_config_editor
        ),
        SovereignTextureConfig::Petal(c) => run!(
            c,
            SovereignPetalConfig,
            bevy_symbios_texture::ui::petal_config_editor
        ),
        SovereignTextureConfig::Shard(c) => run!(
            c,
            SovereignShardConfig,
            bevy_symbios_texture::ui::shard_config_editor
        ),
        SovereignTextureConfig::LeafSprite(c) => run!(
            c,
            SovereignLeafSpriteConfig,
            bevy_symbios_texture::ui::leaf_sprite_config_editor
        ),
        SovereignTextureConfig::Flame(c) => run!(
            c,
            SovereignFlameConfig,
            bevy_symbios_texture::ui::flame_config_editor
        ),
        SovereignTextureConfig::Flower(c) => run!(
            c,
            SovereignFlowerConfig,
            bevy_symbios_texture::ui::flower_config_editor
        ),
        SovereignTextureConfig::GrassTuft(c) => run!(
            c,
            SovereignGrassTuftConfig,
            bevy_symbios_texture::ui::grass_config_editor
        ),
        SovereignTextureConfig::Frond(c) => run!(
            c,
            SovereignFrondConfig,
            bevy_symbios_texture::ui::frond_config_editor
        ),
        SovereignTextureConfig::Reed(c) => run!(
            c,
            SovereignReedConfig,
            bevy_symbios_texture::ui::reed_config_editor
        ),
        SovereignTextureConfig::Needle(c) => run!(
            c,
            SovereignNeedleConfig,
            bevy_symbios_texture::ui::needle_config_editor
        ),
        SovereignTextureConfig::Broadleaf(c) => run!(
            c,
            SovereignBroadleafConfig,
            bevy_symbios_texture::ui::broadleaf_config_editor
        ),
        SovereignTextureConfig::Moss(c) => run!(
            c,
            SovereignMossConfig,
            bevy_symbios_texture::ui::moss_config_editor
        ),
        SovereignTextureConfig::Lichen(c) => run!(
            c,
            SovereignLichenConfig,
            bevy_symbios_texture::ui::lichen_config_editor
        ),
        SovereignTextureConfig::Fabric(c) => run!(
            c,
            SovereignFabricConfig,
            bevy_symbios_texture::ui::fabric_config_editor
        ),
        SovereignTextureConfig::Sand(c) => run!(
            c,
            SovereignSandConfig,
            bevy_symbios_texture::ui::sand_config_editor
        ),
        SovereignTextureConfig::Snow(c) => run!(
            c,
            SovereignSnowConfig,
            bevy_symbios_texture::ui::snow_config_editor
        ),
        SovereignTextureConfig::Ice(c) => run!(
            c,
            SovereignIceConfig,
            bevy_symbios_texture::ui::ice_config_editor
        ),
        SovereignTextureConfig::CactusSkin(c) => run!(
            c,
            SovereignCactusSkinConfig,
            bevy_symbios_texture::ui::cactus_config_editor
        ),
        SovereignTextureConfig::Lava(c) => run!(
            c,
            SovereignLavaConfig,
            bevy_symbios_texture::ui::lava_config_editor
        ),
        SovereignTextureConfig::CrackedEarth(c) => run!(
            c,
            SovereignCrackedEarthConfig,
            bevy_symbios_texture::ui::cracked_earth_config_editor
        ),
        SovereignTextureConfig::Gravel(c) => run!(
            c,
            SovereignGravelConfig,
            bevy_symbios_texture::ui::gravel_config_editor
        ),
        SovereignTextureConfig::ForestFloor(c) => run!(
            c,
            SovereignForestFloorConfig,
            bevy_symbios_texture::ui::forest_floor_config_editor
        ),
        SovereignTextureConfig::Enamel(c) => run!(
            c,
            SovereignEnamelConfig,
            bevy_symbios_texture::ui::enamel_config_editor
        ),
        SovereignTextureConfig::Obsidian(c) => run!(
            c,
            SovereignObsidianConfig,
            bevy_symbios_texture::ui::obsidian_config_editor
        ),
        SovereignTextureConfig::Chitin(c) => run!(
            c,
            SovereignChitinConfig,
            bevy_symbios_texture::ui::chitin_config_editor
        ),
        SovereignTextureConfig::SolarPanel(c) => run!(
            c,
            SovereignSolarPanelConfig,
            bevy_symbios_texture::ui::solar_panel_config_editor
        ),
        SovereignTextureConfig::Parquet(c) => run!(
            c,
            SovereignParquetConfig,
            bevy_symbios_texture::ui::parquet_config_editor
        ),
        SovereignTextureConfig::Truchet(c) => run!(
            c,
            SovereignTruchetConfig,
            bevy_symbios_texture::ui::truchet_config_editor
        ),
        SovereignTextureConfig::ChainLink(c) => run!(
            c,
            SovereignChainLinkConfig,
            bevy_symbios_texture::ui::chain_link_config_editor
        ),
        SovereignTextureConfig::LogEnd(c) => run!(
            c,
            SovereignLogEndConfig,
            bevy_symbios_texture::ui::log_end_config_editor
        ),
    }
}
