//! `--cover-map PATH` (#1674): the ground of a `--world` as whatever grows
//! on it reads it, drawn from above.
//!
//! One pixel per cover texel (a metre at the default 2 m cell), north (-Z)
//! up and east (+X) right. The ground is hill-shaded and tinted by its
//! splat layers - the record's first layer green, the second brown, the
//! third grey, the fourth white, as a default room's grass, dirt, rock and
//! snow are - and where the cover is blocked it is painted over: blue
//! where the ground lies under the water line, red under a road, a
//! building or any other solid thing standing on it. The file is rewritten
//! each time the cover is redrawn, and a world settles only once the cover
//! has been drawn and held still, so the file a run leaves is the cover
//! its frame was taken over.

use std::path::PathBuf;

use bevy::prelude::*;

use crate::config::terrain::ground_cover as cfg;
use crate::ground_cover::{GroundCover, mesh_height};
use crate::interaction::TerrainSurfaceQuery;
use crate::state::LiveRoomRecord;
use crate::terrain::FinishedHeightMap;

/// Where `--cover-map` writes.
#[derive(Resource)]
pub(super) struct CoverMapOut(pub(super) PathBuf);

/// The layers' tints, in the record's layer order.
const LAYER_TINTS: [[f32; 3]; 4] = [
    [0.36, 0.56, 0.24],
    [0.56, 0.42, 0.27],
    [0.52, 0.52, 0.52],
    [0.95, 0.95, 0.97],
];
/// Where the cover is blocked: water, and anything standing on the ground.
const WATER_TINT: [f32; 3] = [0.15, 0.35, 0.85];
const BLOCKED_TINT: [f32; 3] = [0.9, 0.12, 0.08];
/// How much of a blocked pixel the blocking tint takes.
const BLOCKED_SHARE: f32 = 0.7;
/// The hill-shading's light, from the north-west and above, and the share
/// of the light a face turned away from it still gets.
const SUN: Vec3 = Vec3::new(-0.6, 1.0, -0.6);
const SHADE_FLOOR: f32 = 0.35;

/// Write the cover map whenever the cover has been redrawn.
pub(super) fn write_cover_map(
    out: Res<CoverMapOut>,
    cover: Res<GroundCover>,
    heightmap: Option<Res<FinishedHeightMap>>,
    surface: Option<Res<TerrainSurfaceQuery>>,
    record: Option<Res<LiveRoomRecord>>,
) {
    if !cover.is_changed() || cover.revision == 0 {
        return;
    }
    let Some(heightmap) = heightmap else {
        return;
    };
    let hm = &heightmap.0;
    let mask = &cover.mask;
    let water = record
        .as_ref()
        .and_then(|r| crate::world_builder::compile::drawn_water_level(&r.0, Some(&*heightmap)));
    let size = mask.size() as u32;
    let sun = SUN.normalize();
    let step = mask.texel();
    let mut image = image::RgbImage::new(size, size);
    for j in 0..mask.size() {
        for i in 0..mask.size() {
            let xz = mask.centre(i, j);
            let h = |dx: f32, dz: f32| mesh_height(hm, xz + Vec2::new(dx, dz));
            let normal = Vec3::new(
                h(-step, 0.0) - h(step, 0.0),
                2.0 * step,
                h(0.0, -step) - h(0.0, step),
            )
            .normalize_or(Vec3::Y);
            let shade = SHADE_FLOOR + (1.0 - SHADE_FLOOR) * normal.dot(sun).max(0.0);
            let weights = surface
                .as_ref()
                .map_or([1.0, 0.0, 0.0, 0.0], |s| s.weights_at(xz.x, xz.y));
            let mut rgb = [0.0f32; 3];
            for (w, tint) in weights.iter().zip(LAYER_TINTS) {
                for (channel, t) in rgb.iter_mut().zip(tint) {
                    *channel += w * t * shade;
                }
            }
            if mask.bytes()[j * mask.size() + i] == crate::ground_cover::mask::BLOCKED {
                let under_water =
                    water.is_some_and(|level| h(0.0, 0.0) < level + cfg::WATER_MARGIN);
                let tint = if under_water {
                    WATER_TINT
                } else {
                    BLOCKED_TINT
                };
                for (channel, t) in rgb.iter_mut().zip(tint) {
                    *channel = *channel * (1.0 - BLOCKED_SHARE) + t * BLOCKED_SHARE;
                }
            }
            image.put_pixel(
                i as u32,
                j as u32,
                image::Rgb(rgb.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)),
            );
        }
    }
    match image.save(&out.0) {
        Ok(()) => info!(
            "wrote cover map {} ({size}x{size} px, revision {}, {:.1}% open)",
            out.0.display(),
            cover.revision,
            mask.open_share() * 100.0
        ),
        Err(error) => warn!("could not write cover map {}: {error}", out.0.display()),
    }
}
