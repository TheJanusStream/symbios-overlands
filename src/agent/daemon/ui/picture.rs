//! `agent ui --picture` (#1424): the interface as a person sees it, drawn
//! into a PNG when asked and at no other time.
//!
//! The interface draws through the world camera, which the daemon parks
//! (`park_world_camera`). For a picture that camera is pointed at an image
//! instead of the window nobody sees, switched on for a few frames with its
//! view of the world emptied - the interface over the fog's colour; `agent
//! look` is what shows the world - read back once, and put back as it was:
//! parked, aimed at its window, seeing the world. The image is the size the
//! interface is laid out at, so nothing about the layout moves, and
//! nothing the world draws is prepared for it: a camera switched back on
//! after a long park can leave a mesh undrawn (#1351), and here none is
//! drawn at all.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy_egui::egui;
use serde_json::{Value, json};

use crate::camera::IsWorldCamera;
use crate::config::agent::{LOOKS_KEPT, VIEWPORT};

use super::super::look::{pictures_dir, save_picture, trim_pictures};
use super::UiWork;

/// A render layer nothing in the game is on: the camera looks through it
/// while it draws the interface alone. (The item preview is layer 1.)
const NOTHING_LAYER: usize = 31;

/// Frames the camera draws before its picture is read back: the first draws
/// the interface once its pipelines are ready, the second is for anything
/// that was still being prepared.
const WARMUP_FRAMES: u32 = 2;

/// How long a picture may take before it is given up and the camera put
/// back. The first in a daemon's life compiles the camera's pipelines.
const GIVE_UP_AFTER: Duration = Duration::from_secs(60);

/// A picture being taken of the interface.
pub(super) struct Picturing {
    camera: Entity,
    target: Handle<Image>,
    /// How the camera was, to put it back so.
    was_target: RenderTarget,
    was_active: bool,
    was_layers: Option<RenderLayers>,
    warm: u32,
    readback: Option<Entity>,
    data: Option<Vec<u8>>,
    /// The part of the frame to keep, in pixels: a window and a margin.
    crop: Option<egui::Rect>,
    started: Instant,
    /// What the command answers, the picture's path added.
    pub(super) answer: Value,
}

/// Point the world camera at an image and switch it on, the world out of
/// its view, to draw the interface; `crop` is the part of the frame the
/// answer is about, in egui points.
pub(super) fn begin(
    world: &mut World,
    answer: Value,
    crop: Option<egui::Rect>,
    pixels_per_point: f32,
) -> Result<Picturing, String> {
    let (camera, was_target, was_active, was_layers) = world
        .query_filtered::<(Entity, &RenderTarget, &Camera, Option<&RenderLayers>), IsWorldCamera>()
        .iter(world)
        .next()
        .map(|(entity, target, camera, layers)| {
            (entity, target.clone(), camera.is_active, layers.cloned())
        })
        .ok_or("there is no camera to draw the interface with yet")?;
    let target = world
        .resource_mut::<Assets<Image>>()
        .add(crate::render_tool::new_target(VIEWPORT));
    let mut entity = world.entity_mut(camera);
    entity.insert((
        RenderTarget::Image(target.clone().into()),
        RenderLayers::layer(NOTHING_LAYER),
    ));
    if let Some(mut camera) = entity.get_mut::<Camera>() {
        camera.is_active = true;
    }
    let crop = crop.map(|rect| {
        let px = egui::Rect::from_min_max(
            (rect.min.to_vec2() * pixels_per_point).to_pos2(),
            (rect.max.to_vec2() * pixels_per_point).to_pos2(),
        );
        px.expand(8.0).intersect(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(VIEWPORT.0 as f32, VIEWPORT.1 as f32),
        ))
    });
    Ok(Picturing {
        camera,
        target,
        was_target,
        was_active,
        was_layers,
        warm: WARMUP_FRAMES,
        readback: None,
        data: None,
        crop,
        started: Instant::now(),
        answer,
    })
}

/// What a frame of a picture comes to.
pub(super) enum Step {
    Pending,
    Done(Result<Value, String>),
}

/// One frame of the picture in `work`: count the camera's frames down, read
/// the image back once, then put the camera back and write the file.
pub(super) fn advance(world: &mut World, work: &mut UiWork) -> Step {
    let Some(picturing) = work.picture.as_mut() else {
        return Step::Pending;
    };
    if picturing.started.elapsed() > GIVE_UP_AFTER {
        let picturing = work.picture.take().expect("checked above");
        put_back(world, &picturing);
        return Step::Done(Err(format!(
            "the picture was not ready within {} s",
            GIVE_UP_AFTER.as_secs()
        )));
    }
    if picturing.warm > 0 {
        picturing.warm -= 1;
        return Step::Pending;
    }
    if picturing.readback.is_none() {
        let readback = world
            .spawn(Readback::texture(picturing.target.clone()))
            .observe(on_readback)
            .id();
        picturing.readback = Some(readback);
        return Step::Pending;
    }
    if picturing.data.is_none() {
        return Step::Pending;
    }
    let picturing = work.picture.take().expect("checked above");
    put_back(world, &picturing);
    Step::Done(write(world, picturing))
}

/// A readback landed. It fires every frame while its entity lives; the
/// first set of bytes is the picture.
fn on_readback(trigger: On<ReadbackComplete>, work: Option<ResMut<UiWork>>) {
    let Some(mut work) = work else {
        return;
    };
    let event = trigger.event();
    if let Some(picturing) = work.picture.as_mut()
        && picturing.readback == Some(event.entity)
        && picturing.data.is_none()
    {
        picturing.data = Some(event.data.clone());
    }
}

/// The camera as it was - parked, aimed at its window, seeing the world -
/// and the image and the readback gone.
pub(super) fn put_back(world: &mut World, picturing: &Picturing) {
    if let Ok(mut entity) = world.get_entity_mut(picturing.camera) {
        entity.insert(picturing.was_target.clone());
        match &picturing.was_layers {
            Some(layers) => {
                entity.insert(layers.clone());
            }
            None => {
                entity.remove::<RenderLayers>();
            }
        }
        if let Some(mut camera) = entity.get_mut::<Camera>()
            && camera.is_active != picturing.was_active
        {
            camera.is_active = picturing.was_active;
        }
    }
    if let Some(readback) = picturing.readback {
        world.despawn(readback);
    }
    world
        .resource_mut::<Assets<Image>>()
        .remove(&picturing.target);
}

/// Write the picture, cropped to what it is of, where the agent's pictures
/// go, and say where.
fn write(world: &World, picturing: Picturing) -> Result<Value, String> {
    let Picturing {
        data,
        crop,
        started,
        mut answer,
        ..
    } = picturing;
    let data = data.ok_or("the picture never arrived")?;
    let (rgba, size) = match crop {
        Some(rect) => cropped(&data, VIEWPORT, rect)?,
        None => (data, VIEWPORT),
    };
    let dir = pictures_dir(world)?;
    let path: PathBuf = dir.join(format!("ui-{}.png", chrono::Utc::now().timestamp_millis()));
    save_picture(&path, Some(&dir), &rgba, size)?;
    trim_pictures(&dir, LOOKS_KEPT);
    answer["picture"] = json!({
        "path": path,
        "size": [size.0, size.1],
        "took_ms": u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    });
    Ok(answer)
}

/// The part `rect` (pixels) of an RGBA frame of `size`.
fn cropped(
    rgba: &[u8],
    (width, height): (u32, u32),
    rect: egui::Rect,
) -> Result<(Vec<u8>, (u32, u32)), String> {
    if rgba.len() != width as usize * height as usize * 4 {
        return Err(format!(
            "the readback is {} bytes, not a {width}x{height} frame's",
            rgba.len()
        ));
    }
    let x0 = (rect.min.x.max(0.0) as u32).min(width);
    let y0 = (rect.min.y.max(0.0) as u32).min(height);
    let x1 = (rect.max.x.ceil().max(0.0) as u32).clamp(x0, width);
    let y1 = (rect.max.y.ceil().max(0.0) as u32).clamp(y0, height);
    if x1 == x0 || y1 == y0 {
        return Err("the window is not on the screen".to_owned());
    }
    let row = width as usize * 4;
    let mut out = Vec::with_capacity((x1 - x0) as usize * (y1 - y0) as usize * 4);
    for y in y0..y1 {
        let start = y as usize * row + x0 as usize * 4;
        out.extend_from_slice(&rgba[start..start + (x1 - x0) as usize * 4]);
    }
    Ok((out, (x1 - x0, y1 - y0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A window's part of the frame is its rows and columns, nothing
    /// shifted, and a window off the frame is refused.
    #[test]
    fn a_crop_keeps_its_pixels_in_place() {
        let (w, h) = (4, 3);
        let frame: Vec<u8> = (0..w * h).flat_map(|i| [i as u8, 0, 0, 255]).collect();
        let (part, size) = cropped(
            &frame,
            (w, h),
            egui::Rect::from_min_max(egui::pos2(1.0, 1.0), egui::pos2(3.0, 3.0)),
        )
        .expect("crops");
        assert_eq!(size, (2, 2));
        let reds: Vec<u8> = part.chunks_exact(4).map(|p| p[0]).collect();
        assert_eq!(reds, [5, 6, 9, 10]);
        assert!(
            cropped(
                &frame,
                (w, h),
                egui::Rect::from_min_max(egui::pos2(9.0, 9.0), egui::pos2(12.0, 12.0))
            )
            .is_err()
        );
    }
}
