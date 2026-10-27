//! A world's banner, drawn (#1783): the record's [`WorldBanner`] as an image,
//! one the world's signs share, and the World Editor's and the arrival's
//! pictures of it.
//!
//! The cloth is a hanging banner, two wide by three high, with a swallowtail
//! cut up into its foot and three soft folds down it. Its field is divided
//! between its two colours ([`BannerDivision`]), and its mark
//! ([`BannerMark`]) is drawn in the other colour of wherever it lies - on a
//! plain field the second colour, on a divided one counterchanged across
//! the line, as a herald draws it, so a mark reads on any division. A world
//! with no banner of its own flies undyed linen.
//!
//! Every sign whose source is [`SignSource::WorldBanner`] paints the one
//! image here ([`WorldBannerArt`]), handed to them through the image cache
//! ([`BlobImageCache::set_world_banner`]); [`draw_world_banner`] redraws it
//! when the world's banner changes, so an owner's edit recolours every
//! banner in the world and rebuilds nothing. The image carries its whole
//! mip chain (#1796), so a banner far off reads as its colours rather than
//! shimmering: each redraw is a new texture, which every sign's material is
//! told to bind ([`rebind`]).
//!
//! [`SignSource::WorldBanner`]: crate::pds::SignSource::WorldBanner
//! [`BlobImageCache::set_world_banner`]: crate::world_builder::image_cache::BlobImageCache::set_world_banner

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

use crate::pds::{BannerDivision, BannerMark, WorldBanner};

/// The shared image's size (px): two wide by three high.
pub const WIDTH: u32 = 96;
/// See [`WIDTH`].
pub const HEIGHT: u32 = 144;

/// Undyed linen: the cloth of a world with no banner of its own.
const LINEN: [f32; 3] = [0.82, 0.78, 0.68];

/// Where the swallowtail's cut begins, down the cloth (0 at its head, 1 at
/// its foot), and its half width at the foot, across (0 to 1).
const NOTCH_TOP: f32 = 0.8;
const NOTCH_HALF: f32 = 0.3;

/// The cloth in its own units: two wide, three high. The divisions' lines
/// and the mark are drawn in these, so a diagonal is a true diagonal.
const CLOTH_W: f32 = 2.0;
const CLOTH_H: f32 = 3.0;
/// The field's middle, where its lines cross and its mark is centred (a
/// little above the cloth's middle, as the swallowtail takes its foot).
const MIDDLE: [f32; 2] = [1.0, 1.3];
/// The mark's half size, in cloth units.
const MARK_HALF: f32 = 0.62;

/// Samples a pixel's side is cut into, for edges that do not stair.
const SUPERSAMPLE: u32 = 3;

/// `banner` drawn `width` by `height`, as straight-alpha sRGB bytes, rows
/// from the head of the cloth down - undyed linen for `None`.
pub fn raster(banner: Option<&WorldBanner>, width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((width * height * 4) as usize);
    let samples = (SUPERSAMPLE * SUPERSAMPLE) as f32;
    // Three soft folds down the cloth: a shade of the column alone, worked
    // out once a column rather than once a sample.
    let across =
        |px: u32, sx: u32| (px as f32 + (sx as f32 + 0.5) / SUPERSAMPLE as f32) / width as f32;
    let folds: Vec<f32> = (0..width)
        .flat_map(|px| (0..SUPERSAMPLE).map(move |sx| (px, sx)))
        .map(|(px, sx)| {
            0.94 + 0.06 * libm::cosf(across(px, sx) * CLOTH_W * std::f32::consts::PI * 3.0)
        })
        .collect();
    for py in 0..height {
        for px in 0..width {
            let mut sum = [0.0f32; 3];
            let mut covered = 0.0f32;
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let x = across(px, sx);
                    let y = (py as f32 + (sy as f32 + 0.5) / SUPERSAMPLE as f32) / height as f32;
                    let fold = folds[(px * SUPERSAMPLE + sx) as usize];
                    if let Some(colour) = colour_at(banner, x, y, fold) {
                        for (s, c) in sum.iter_mut().zip(colour) {
                            *s += c;
                        }
                        covered += 1.0;
                    }
                }
            }
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            if covered > 0.0 {
                bytes.extend(sum.map(|s| byte(s / covered)));
            } else {
                bytes.extend([0, 0, 0]);
            }
            bytes.push(byte(covered / samples));
        }
    }
    bytes
}

/// The cloth's colour at `(x, y)` (0 to 1 across and down), shaded by the
/// `fold` it lies in, or `None` where the swallowtail is cut away.
fn colour_at(banner: Option<&WorldBanner>, x: f32, y: f32, fold: f32) -> Option<[f32; 3]> {
    if y > NOTCH_TOP && (x - 0.5).abs() < NOTCH_HALF * (y - NOTCH_TOP) / (1.0 - NOTCH_TOP) {
        return None;
    }
    let (u, v) = (x * CLOTH_W, y * CLOTH_H);
    let base = match banner {
        None => LINEN,
        Some(banner) => {
            let second = in_second(banner.division, u, v);
            let mu = (u - MIDDLE[0]) / MARK_HALF;
            let mv = (v - MIDDLE[1]) / MARK_HALF;
            // The mark takes the other colour of wherever it lies.
            if in_mark(banner.mark, mu, mv) != second {
                banner.second.0
            } else {
                banner.field.0
            }
        }
    };
    // Its head darker where it is sewn over the bar it hangs from.
    let hem = if y < 0.035 { 0.8 } else { 1.0 };
    Some(base.map(|c| c * fold * hem))
}

/// Whether the cloth at `(u, v)` (cloth units) is the division's second
/// part.
fn in_second(division: BannerDivision, u: f32, v: f32) -> bool {
    let (du, dv) = (u - MIDDLE[0], v - MIDDLE[1]);
    match division {
        BannerDivision::Plain | BannerDivision::Unknown => false,
        BannerDivision::PerPale => du > 0.0,
        BannerDivision::PerFess => dv > 0.0,
        BannerDivision::PerBend => dv > du,
        BannerDivision::Quarterly => (du > 0.0) != (dv > 0.0),
        // Under a chevron whose point stands a third of the field's middle
        // above it, its arms falling at 45 degrees.
        BannerDivision::PerChevron => dv > du.abs() - 0.4,
    }
}

/// Whether `(mu, mv)` - the mark's own square, -1 to 1 each way, down
/// positive - lies on `mark`.
fn in_mark(mark: BannerMark, mu: f32, mv: f32) -> bool {
    use std::f32::consts::{PI, TAU};
    let r = (mu * mu + mv * mv).sqrt();
    // The angle from straight up, turning clockwise as seen - asked only by
    // the marks with points round them.
    let theta = || libm::atan2f(mu, -mv);
    match mark {
        BannerMark::None => false,
        BannerMark::Star | BannerMark::Unknown => {
            // Folded into one of its five arms, either side of the tip.
            let sector = TAU / 5.0;
            let phi = (theta().rem_euclid(sector) - sector * 0.5).abs();
            let (outer, inner) = (0.95, 0.4);
            // The tip at (outer, 0), the arm's inner corner at half a
            // sector round, inner out; inside is the origin's side.
            let (tx, ty) = (outer, 0.0);
            let (ix, iy) = (
                inner * libm::cosf(sector * 0.5),
                inner * libm::sinf(sector * 0.5),
            );
            let (px, py) = (
                r * libm::cosf(sector * 0.5 - phi),
                r * libm::sinf(sector * 0.5 - phi),
            );
            let cross = |ax: f32, ay: f32, bx: f32, by: f32| ax * by - ay * bx;
            let edge = (ix - tx, iy - ty);
            let point = cross(edge.0, edge.1, px - tx, py - ty);
            let origin = cross(edge.0, edge.1, -tx, -ty);
            point * origin >= 0.0
        }
        BannerMark::Crescent => r < 0.8 && (mu * mu + (mv + 0.32) * (mv + 0.32)).sqrt() > 0.64,
        BannerMark::Sun => {
            if r < 0.42 {
                return true;
            }
            if !(0.5..0.95).contains(&r) {
                return false;
            }
            let sector = TAU / 12.0;
            let phi = (theta().rem_euclid(sector) - sector * 0.5).abs();
            // Each ray narrows to its point.
            phi < sector * 0.32 * (1.0 - (r - 0.5) / 0.45)
        }
        BannerMark::Roundel => r < 0.72,
        BannerMark::Lozenge => mu.abs() / 0.62 + mv.abs() / 0.95 < 1.0,
        BannerMark::Cross => {
            (mu.abs() < 0.2 && mv.abs() < 0.85) || (mv.abs() < 0.2 && mu.abs() < 0.85)
        }
        BannerMark::Saltire => {
            let a = (mu + mv) * std::f32::consts::FRAC_1_SQRT_2;
            let b = (mu - mv) * std::f32::consts::FRAC_1_SQRT_2;
            mu.abs() < 0.8
                && mv.abs() < 0.8
                && ((a.abs() < 0.17 && b.abs() < 1.2) || (b.abs() < 0.17 && a.abs() < 1.2))
        }
        BannerMark::Tower => {
            // Its door and a window are the field's, seen through it.
            let door = mu.abs() < 0.13 && mv > 0.42;
            let window = mu.abs() < 0.06 && (-0.15..0.1).contains(&mv);
            let body = mu.abs() < 0.42 && (-0.35..0.85).contains(&mv);
            // Three merlons over it, in the five slots across its head.
            let slot = ((mu + 0.5) / 0.2).floor();
            let merlon = (-0.62..-0.35).contains(&mv) && mu.abs() < 0.5 && slot as i32 % 2 == 0;
            (body || merlon) && !door && !window
        }
        BannerMark::Tree => {
            let trunk = mu.abs() < 0.09 && (0.45..0.92).contains(&mv);
            // Three tiers, each a triangle from its apex down to its base.
            let tier = |apex: f32, base: f32, half: f32| {
                (apex..base).contains(&mv) && mu.abs() < half * (mv - apex) / (base - apex)
            };
            trunk || tier(-0.95, -0.3, 0.38) || tier(-0.62, 0.12, 0.56) || tier(-0.28, 0.55, 0.74)
        }
        BannerMark::Peaks => {
            let peak = |apex_u: f32, apex_v: f32, half: f32| {
                (apex_v..0.8).contains(&mv)
                    && (mu - apex_u).abs() < half * (mv - apex_v) / (0.8 - apex_v)
            };
            mu.abs() < 0.95 && (peak(0.15, -0.75, 0.85) || peak(-0.45, -0.25, 0.55))
        }
        BannerMark::Waves => {
            mu.abs() < 0.85
                && (0..3).any(|k| {
                    let centre = -0.5 + k as f32 * 0.5;
                    (mv - centre - 0.13 * libm::sinf(mu * PI * 2.2)).abs() < 0.11
                })
        }
    }
}

/// [`raster`] at the shared size, drawn once for each banner however many ask:
/// the world's image and the interface's pictures of it ask in one frame
/// while an owner drags a colour.
pub fn shared_raster(banner: Option<&WorldBanner>) -> std::sync::Arc<Vec<u8>> {
    type Drawn = Option<(Option<WorldBanner>, std::sync::Arc<Vec<u8>>)>;
    static LAST: std::sync::Mutex<Drawn> = std::sync::Mutex::new(None);
    let want = banner.cloned();
    let mut last = LAST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((drawn, bytes)) = last.as_ref()
        && *drawn == want
    {
        return bytes.clone();
    }
    let bytes = std::sync::Arc::new(raster(want.as_ref(), WIDTH, HEIGHT));
    *last = Some((want, bytes.clone()));
    bytes
}

/// The image every sign showing this world's banner shares, and the banner
/// it was last drawn for - `Some(None)` drawn as undyed linen, `None` not
/// drawn yet.
#[derive(Resource)]
pub struct WorldBannerArt {
    pub image: Handle<Image>,
    drawn: Option<Option<WorldBanner>>,
}

impl FromWorld for WorldBannerArt {
    fn from_world(world: &mut World) -> Self {
        let image = world
            .resource_mut::<Assets<Image>>()
            .add(banner_image(None));
        Self { image, drawn: None }
    }
}

/// `banner` as an image asset of the shared size, its whole mip chain
/// after it ([`mipped`]), sampled through the chain.
///
/// Bound for sampling only, no `COPY_DST`, as `sky_light::cube_image` is:
/// Bevy re-uploads an image whose descriptor did not change by writing its
/// first level alone into the texture it has (`GpuImage::prepare_asset`,
/// bevy_render 0.19), which would leave a redrawn banner's old colours on
/// its far levels. Without the usage it makes a new texture from the whole
/// chain each time.
fn banner_image(banner: Option<&WorldBanner>) -> Image {
    let (data, levels) = mipped(&shared_raster(banner), WIDTH, HEIGHT);
    let mut image = Image::default();
    image.texture_descriptor.size = Extent3d {
        width: WIDTH,
        height: HEIGHT,
        depth_or_array_layers: 1,
    };
    image.texture_descriptor.dimension = TextureDimension::D2;
    image.texture_descriptor.format = TextureFormat::Rgba8UnormSrgb;
    image.texture_descriptor.mip_level_count = levels;
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING;
    image.sampler = ImageSampler::linear();
    // Kept on the CPU too: a redraw replaces it whole, and it is small.
    image.asset_usage = RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD;
    image.data = Some(data);
    image
}

/// `rgba`, straight-alpha sRGB bytes `width` by `height`, followed by every
/// level of its mip chain, each half the last's size a side and at least a
/// texel - and how many levels that is, its own included. Each texel of a
/// level is the texels of the last it covers - two a side, one where a side
/// is one texel, three at the end of an odd side, so none is left out -
/// averaged in linear light and weighted by how much each covers, so a
/// cut-away texel neither darkens nor tints the cloth beside it.
fn mipped(rgba: &[u8], width: u32, height: u32) -> (Vec<u8>, u32) {
    let mut data = rgba.to_vec();
    // Each texel premultiplied, in linear light.
    let mut level: Vec<[f32; 4]> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&[r, g, b, a]| {
            let linear = LinearRgba::from(Srgba::rgba_u8(r, g, b, a));
            let a = linear.alpha;
            [linear.red * a, linear.green * a, linear.blue * a, a]
        })
        .collect();
    let (mut w, mut h, mut levels) = (width.max(1), height.max(1), 1);
    while w > 1 || h > 1 {
        let (next_w, next_h) = ((w / 2).max(1), (h / 2).max(1));
        // The texels along a side of `side` that texel `i` of `next` covers.
        let span =
            |i: u32, side: u32, next: u32| 2 * i..if i + 1 == next { side } else { 2 * i + 2 };
        let next: Vec<[f32; 4]> = (0..next_h)
            .flat_map(|y| (0..next_w).map(move |x| (x, y)))
            .map(|(x, y)| {
                let (xs, ys) = (span(x, w, next_w), span(y, h, next_h));
                let covered = (xs.len() * ys.len()) as f32;
                let mut sum = [0.0_f32; 4];
                for ty in ys {
                    for tx in xs.clone() {
                        for (s, t) in sum.iter_mut().zip(level[(ty * w + tx) as usize]) {
                            *s += t;
                        }
                    }
                }
                sum.map(|s| s / covered)
            })
            .collect();
        for &[r, g, b, a] in &next {
            let straight = if a > 0.0 {
                LinearRgba::new(r / a, g / a, b / a, a)
            } else {
                LinearRgba::new(0.0, 0.0, 0.0, 0.0)
            };
            data.extend(Srgba::from(straight).to_u8_array());
        }
        (level, w, h) = (next, next_w, next_h);
        levels += 1;
    }
    (data, levels)
}

/// Mark every material whose texture is `image` changed, so each is
/// prepared anew and binds the texture `image` is drawn to now (#1796): a
/// redrawn banner is a new texture. How many there were.
fn rebind(
    image: &Handle<Image>,
    standard: &mut Assets<StandardMaterial>,
    weathered: &mut Assets<crate::weathered::WeatheredMaterial>,
) -> usize {
    let painted: Vec<_> = standard
        .iter()
        .filter(|(_, material)| material.base_color_texture.as_ref() == Some(image))
        .map(|(id, _)| id)
        .collect();
    for &id in &painted {
        if let Some(material) = standard.get_mut(id) {
            material.into_inner();
        }
    }
    let weathered_painted: Vec<_> = weathered
        .iter()
        .filter(|(_, material)| material.base.base_color_texture.as_ref() == Some(image))
        .map(|(id, _)| id)
        .collect();
    for &id in &weathered_painted {
        if let Some(material) = weathered.get_mut(id) {
            material.into_inner();
        }
    }
    painted.len() + weathered_painted.len()
}

/// Hand the image to the image cache's signs, and redraw it when the
/// world's banner changes - touching nothing on a frame it has not.
pub fn draw_world_banner(
    record: Option<Res<crate::state::LiveRoomRecord>>,
    mut art: ResMut<WorldBannerArt>,
    mut images: ResMut<Assets<Image>>,
    mut cache: ResMut<crate::world_builder::image_cache::BlobImageCache>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut weathered: ResMut<Assets<crate::weathered::WeatheredMaterial>>,
) {
    if cache.world_banner().is_none() {
        let image = art.image.clone();
        cache.set_world_banner(image, &mut standard, &mut weathered);
    }
    let want = record.and_then(|record| record.0.identity.as_ref().map(|i| i.banner.clone()));
    if art.drawn.as_ref() == Some(&want) {
        return;
    }
    // A new texture, its whole chain (`banner_image`): every sign's
    // material binds it once told.
    if let Some(mut image) = images.get_mut(&art.image) {
        *image = banner_image(want.as_ref());
    }
    rebind(&art.image, &mut standard, &mut weathered);
    art.drawn = Some(want);
}

/// The banner's resource and its system, for the game and the render tool
/// alike.
pub fn register(app: &mut App) {
    app.init_resource::<WorldBannerArt>()
        .add_systems(Update, draw_world_banner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::Fp3;

    fn banner(division: BannerDivision, mark: BannerMark) -> WorldBanner {
        WorldBanner {
            field: Fp3([0.1, 0.26, 0.6]),
            second: Fp3([0.92, 0.91, 0.87]),
            division,
            mark,
        }
    }

    /// The pixel at `(x, y)` (0 to 1) of `bytes`, drawn `WIDTH` by `HEIGHT`.
    fn pixel(bytes: &[u8], x: f32, y: f32) -> [u8; 4] {
        let px = ((x * WIDTH as f32) as u32).min(WIDTH - 1);
        let py = ((y * HEIGHT as f32) as u32).min(HEIGHT - 1);
        let i = ((py * WIDTH + px) * 4) as usize;
        [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]
    }

    fn is_light(p: [u8; 4]) -> bool {
        p[0] > 150 && p[1] > 150
    }

    #[test]
    fn the_cloth_is_cut_into_a_swallowtail() {
        let bytes = raster(
            Some(&banner(BannerDivision::Plain, BannerMark::None)),
            WIDTH,
            HEIGHT,
        );
        assert_eq!(bytes.len(), (WIDTH * HEIGHT * 4) as usize);
        assert_eq!(
            pixel(&bytes, 0.5, 0.97)[3],
            0,
            "the foot's middle is cut away"
        );
        assert_eq!(pixel(&bytes, 0.05, 0.97)[3], 255, "its tails hang on");
        assert_eq!(pixel(&bytes, 0.5, 0.5)[3], 255);
    }

    /// The mark is the second colour on a plain field, and counterchanged on
    /// a divided one: the other colour of wherever it lies.
    #[test]
    fn a_mark_takes_the_other_colour_of_where_it_lies() {
        let plain = raster(
            Some(&banner(BannerDivision::Plain, BannerMark::Roundel)),
            WIDTH,
            HEIGHT,
        );
        // The mark's middle, and the field beside it at the cloth's edge.
        let (cx, cy) = (MIDDLE[0] / CLOTH_W, MIDDLE[1] / CLOTH_H);
        assert!(is_light(pixel(&plain, cx, cy)));
        assert!(!is_light(pixel(&plain, 0.04, cy)));
        let pale = raster(
            Some(&banner(BannerDivision::PerPale, BannerMark::Roundel)),
            WIDTH,
            HEIGHT,
        );
        // Left of the line the field, the mark light; right of it the
        // second colour, the mark dark.
        assert!(is_light(pixel(&pale, cx - 0.1, cy)));
        assert!(!is_light(pixel(&pale, cx + 0.1, cy)));
        assert!(!is_light(pixel(&pale, 0.04, cy)));
        assert!(is_light(pixel(&pale, 0.96, cy)));
    }

    /// Every mark draws something inside its square and leaves the field
    /// round it, and every division gives its second colour a share.
    #[test]
    fn every_mark_and_division_draws() {
        for mark in BannerMark::ALL {
            let mut on = 0;
            let mut total = 0;
            for i in 0..40 {
                for j in 0..40 {
                    let (mu, mv) = (i as f32 / 20.0 - 1.0, j as f32 / 20.0 - 1.0);
                    total += 1;
                    on += usize::from(in_mark(mark, mu, mv));
                }
            }
            let share = on as f32 / total as f32;
            if mark == BannerMark::None {
                assert_eq!(on, 0);
            } else {
                assert!((0.08..0.75).contains(&share), "{mark:?}: {share}");
            }
            assert!(
                !in_mark(mark, 1.0, 1.0) && !in_mark(mark, -1.0, -1.0),
                "{mark:?}"
            );
        }
        for division in BannerDivision::ALL {
            let mut second = 0;
            for i in 0..20 {
                for j in 0..30 {
                    second += usize::from(in_second(division, i as f32 * 0.1, j as f32 * 0.1));
                }
            }
            let share = second as f32 / 600.0;
            if division == BannerDivision::Plain {
                assert_eq!(second, 0);
            } else {
                assert!((0.25..0.75).contains(&share), "{division:?}: {share}");
            }
        }
    }

    /// The star points up, and its tip and its arms' inner corners fall
    /// where a five-pointed star's do.
    #[test]
    fn the_star_points_up() {
        assert!(in_mark(BannerMark::Star, 0.0, -0.9), "its tip is up");
        assert!(!in_mark(BannerMark::Star, 0.0, 0.9), "and not down");
        assert!(in_mark(BannerMark::Star, 0.0, 0.0));
        // Between two arms, past the inner corner, is the field.
        assert!(!in_mark(BannerMark::Star, 0.0, 0.6));
    }

    #[test]
    fn a_world_with_no_banner_flies_linen() {
        let bytes = raster(None, WIDTH, HEIGHT);
        let p = pixel(&bytes, 0.5, 0.4);
        assert!(
            p[0] > 180 && p[1] > 170 && p[2] > 140 && p[3] == 255,
            "{p:?}"
        );
    }

    /// #1796: the banner carries its whole mip chain after its own bytes,
    /// each level half the last a side down to one texel, and is a texture
    /// made new at every redraw, sampled through the chain.
    #[test]
    fn a_banner_carries_its_mip_chain() {
        let image = banner_image(None);
        assert_eq!(image.texture_descriptor.mip_level_count, 8);
        assert!(
            !image
                .texture_descriptor
                .usage
                .contains(TextureUsages::COPY_DST)
        );
        let texels: u32 = [
            (96, 144),
            (48, 72),
            (24, 36),
            (12, 18),
            (6, 9),
            (3, 4),
            (1, 2),
            (1, 1),
        ]
        .iter()
        .map(|(w, h)| w * h)
        .sum();
        assert_eq!(image.data.as_ref().map(Vec::len), Some(texels as usize * 4));
        assert_eq!(
            &image.data.as_ref().expect("its bytes")[..(WIDTH * HEIGHT * 4) as usize],
            shared_raster(None).as_slice(),
            "its own level first"
        );
    }

    /// #1796: a level averages the texels under it in linear light, each by
    /// how much of it the cloth covers: a cut-away texel beside red cloth
    /// leaves it red, three quarters covered, and white beside black averages
    /// to the light of half white - lighter than the bytes' mean. The last
    /// texel of an odd side is averaged in: red, green and blue make a grey.
    #[test]
    fn a_mip_level_averages_light_by_coverage() {
        let red = [200, 20, 20, 255];
        let (data, levels) = mipped(&[red, red, red, [0, 0, 0, 0]].concat(), 2, 2);
        assert_eq!(levels, 2);
        assert_eq!(data[16..20], [200, 20, 20, 191]);
        let (data, levels) = mipped(&[[255, 255, 255, 255], [0, 0, 0, 255]].concat(), 2, 1);
        assert_eq!(levels, 2);
        assert!((186..=190).contains(&data[8]), "{}", data[8]);
        let three = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]];
        let (data, levels) = mipped(&three.concat(), 3, 1);
        assert_eq!(levels, 2);
        let grey = &data[12..16];
        assert!(
            grey[0] == grey[1] && grey[1] == grey[2] && grey[3] == 255,
            "{grey:?}"
        );
    }

    /// #1796: a redraw tells every material that shows the banner - a plain
    /// one or a weathered one - and no other, to bind it anew.
    #[test]
    fn a_redraw_rebinds_every_material_that_shows_the_banner() {
        let mut images = Assets::<Image>::default();
        let banner = images.add(banner_image(None));
        let other = images.add(Image::default());
        let mut standard = Assets::<StandardMaterial>::default();
        for texture in [&banner, &banner, &other] {
            standard.add(StandardMaterial {
                base_color_texture: Some(texture.clone()),
                ..default()
            });
        }
        let mut weathered = Assets::<crate::weathered::WeatheredMaterial>::default();
        weathered.add(crate::weathered::weathered(
            StandardMaterial {
                base_color_texture: Some(banner.clone()),
                ..default()
            },
            true,
        ));
        assert_eq!(rebind(&banner, &mut standard, &mut weathered), 3);
        assert_eq!(rebind(&other, &mut standard, &mut weathered), 1);
    }
}
