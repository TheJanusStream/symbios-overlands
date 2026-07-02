//! Material & splat widgets plus the unified texture-bridge dispatcher.
//! Handles both terrain's four-layer `SovereignMaterialConfig` (rules +
//! layers) and per-slot `SovereignTextureConfig` editing (bark/leaf/twig,
//! brick, plank, shingle, and the rest of the `bevy_symbios_texture` family).

use bevy_egui::egui;

use crate::pds::{
    SovereignAshlarConfig, SovereignAsphaltConfig, SovereignBarkConfig, SovereignBrickConfig,
    SovereignChainLinkConfig, SovereignCobblestoneConfig, SovereignConcreteConfig,
    SovereignCorrugatedConfig, SovereignEncausticConfig, SovereignFabricConfig,
    SovereignFlameConfig, SovereignFlowerConfig, SovereignGroundConfig, SovereignIceConfig,
    SovereignIronGrilleConfig, SovereignLavaConfig, SovereignLeafConfig, SovereignLeafSpriteConfig,
    SovereignLogEndConfig, SovereignMarbleConfig, SovereignMaterialConfig, SovereignMetalConfig,
    SovereignPaversConfig, SovereignPetalConfig, SovereignPlankConfig, SovereignPuffConfig,
    SovereignRingConfig, SovereignRockConfig, SovereignSandConfig, SovereignShardConfig,
    SovereignShingleConfig, SovereignSnowConfig, SovereignSnowflakeConfig, SovereignSoftDiscConfig,
    SovereignSparkConfig, SovereignSplatRule, SovereignStainedGlassConfig, SovereignStuccoConfig,
    SovereignTextureConfig, SovereignThatchConfig, SovereignTwigConfig, SovereignWainscotingConfig,
    SovereignWindowConfig,
};

use super::widgets::{drag_u32, fp_slider};

pub(super) fn draw_material_forge(
    ui: &mut egui::Ui,
    mat: &mut SovereignMaterialConfig,
    dirty: &mut bool,
) {
    drag_u32(ui, "Texture size", &mut mat.texture_size, 16, 4096, dirty);
    fp_slider(ui, "Tile scale", &mut mat.tile_scale, 1.0, 500.0, dirty);

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
                );
            });
    }
}

fn draw_splat_rule(ui: &mut egui::Ui, rule: &mut SovereignSplatRule, dirty: &mut bool) {
    fp_slider(ui, "Height min", &mut rule.height_min, 0.0, 1.0, dirty);
    fp_slider(ui, "Height max", &mut rule.height_max, 0.0, 1.0, dirty);
    fp_slider(ui, "Slope min", &mut rule.slope_min, 0.0, 1.0, dirty);
    fp_slider(ui, "Slope max", &mut rule.slope_max, 0.0, 1.0, dirty);
    fp_slider(ui, "Sharpness", &mut rule.sharpness, 0.05, 8.0, dirty);
}

pub(super) fn draw_texture_bridge(
    ui: &mut egui::Ui,
    texture: &mut SovereignTextureConfig,
    salt: &str,
    dirty: &mut bool,
) {
    draw_texture_bridge_opts(ui, texture, salt, dirty, true);
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
) {
    egui::ComboBox::from_id_salt(format!("{}_tex_ty", salt))
        .selected_text(texture.label())
        .show_ui(ui, |ui| {
            macro_rules! opt {
                ($label:literal, $expr:expr) => {{
                    let selected = texture.label() == $label;
                    if ui.selectable_label(selected, $label).clicked() && !selected {
                        *texture = $expr;
                        *dirty = true;
                    }
                }};
            }
            opt!("None", SovereignTextureConfig::None);
            // Slotted between None and the procedural-generator list so
            // the existing 24-variant order stays contiguous — muscle
            // memory survives the addition.
            if allow_referenced {
                opt!(
                    "Referenced",
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
            // Additional tileable surfaces.
            opt!("Fabric", SovereignTextureConfig::Fabric(Default::default()));
            opt!("Sand", SovereignTextureConfig::Sand(Default::default()));
            opt!("Snow", SovereignTextureConfig::Snow(Default::default()));
            opt!("Ice", SovereignTextureConfig::Ice(Default::default()));
            opt!("Lava", SovereignTextureConfig::Lava(Default::default()));
            // Alpha-masked mesh cards.
            opt!(
                "Chain Link",
                SovereignTextureConfig::ChainLink(Default::default())
            );
            opt!(
                "Log End",
                SovereignTextureConfig::LogEnd(Default::default())
            );
        });

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
        SovereignTextureConfig::None | SovereignTextureConfig::Unknown => {}
        // Referenced has its own sub-source editor (URL / AtprotoBlob /
        // DidPfp) wired up in the asset-reference UI ticket; for now the
        // variant exists on the type so room records can round-trip it.
        SovereignTextureConfig::Referenced { source } => {
            super::widgets::draw_asset_reference_editor(ui, source, salt, dirty);
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
        SovereignTextureConfig::Lava(c) => run!(
            c,
            SovereignLavaConfig,
            bevy_symbios_texture::ui::lava_config_editor
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
