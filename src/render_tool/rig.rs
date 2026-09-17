//! The camera rig behind the tool's single-camera output: `--world` sheets
//! and every `--frames` clip. It answers one question - where the camera
//! sits and what it looks at, at a point `t` along the clip - from the
//! `--focus` / `--dist` / `--elev` / `--yaw` / `--sweep` flags, and leaves
//! the *world* side of the question (where the walker is, how high the
//! ground is at the origin) to whoever resolves the [`Focus`] into a point.
//!
//! Angles follow the sheet cameras' convention (see `ANGLES` in the parent
//! module): yaw is measured about `+Y`, and yaw 180° puts the camera on the
//! `-Z` side looking toward `+Z` - the "front" of a subject that faces `-Z`.

use bevy::prelude::*;

/// What the rig orbits. Resolved to a world point by the mode that owns the
/// subject: [`Self::Origin`] and [`Self::Landing`] need the heightmap,
/// [`Self::Walker`] the walking body, [`Self::Subject`] the framed bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Focus {
    /// The room origin - the spawn square, at terrain height. The login
    /// backdrop's own focus, which frames the settlement a fresh room grows
    /// around it.
    Origin,
    /// The record's default landing: the gateway forecourt, where a visitor
    /// arrives.
    Landing,
    /// The centre of the placed structures - the centroid of the record's
    /// `Absolute` placements - which is where the settlement actually
    /// stands, as opposed to the spawn square it grew around.
    Settlement,
    /// The `--walker` body, followed as it walks.
    Walker,
    /// The auto-framed bounds of a single subject - the turntable case.
    Subject,
    /// An explicit world point: `x,z` on the terrain surface, or `x,y,z`.
    Point { x: f32, y: Option<f32>, z: f32 },
}

impl Focus {
    /// Parse a `--focus` value: a keyword, `x,z` or `x,y,z`.
    pub(super) fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "origin" | "spawn" => return Ok(Self::Origin),
            "landing" | "gateway" => return Ok(Self::Landing),
            "settlement" | "town" => return Ok(Self::Settlement),
            "walker" => return Ok(Self::Walker),
            "subject" => return Ok(Self::Subject),
            _ => {}
        }
        let parts: Result<Vec<f32>, _> = s.split(',').map(|p| p.trim().parse::<f32>()).collect();
        match parts.as_deref() {
            Ok([x, z]) => Ok(Self::Point {
                x: *x,
                y: None,
                z: *z,
            }),
            Ok([x, y, z]) => Ok(Self::Point {
                x: *x,
                y: Some(*y),
                z: *z,
            }),
            _ => Err(format!(
                "--focus {s:?}: expected origin | landing | settlement | walker | subject | x,z | x,y,z"
            )),
        }
    }
}

/// The orbit: a focus, a lift above it, and a start→end pair for distance
/// and elevation plus a yaw sweep, all interpolated linearly over the clip.
/// A still is the `t = 0` pose.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct CameraRig {
    pub(super) focus: Focus,
    /// Metres above the resolved focus point the camera looks at, so a
    /// world shot centres on the built-up band rather than the ground and a
    /// walker shot on the torso rather than the feet.
    pub(super) lift: f32,
    /// Camera distance from the look-at point, metres, at `t = 0` and `t = 1`;
    /// `None` leaves it to the subject's auto-framing.
    pub(super) dist: Option<(f32, f32)>,
    /// Elevation above the look-at point, degrees, at `t = 0` and `t = 1`;
    /// `None` leaves it to the mode's default orbit.
    pub(super) elev: Option<(f32, f32)>,
    /// Yaw at `t = 0`, degrees. For a [`Focus::Walker`] rig this is measured
    /// from directly behind the walker, so 0 follows and 180 faces it.
    pub(super) yaw: f32,
    /// How far the yaw turns over the clip, degrees. 360 is one full
    /// turntable revolution; 0 holds the angle.
    pub(super) sweep: f32,
    /// Divides the *auto-framed* distance (a turntable's, from the subject's
    /// bounds): 1 is the sheet cameras' fit, 1.5 sits a third closer. An
    /// explicit `--dist` is absolute and ignores it.
    pub(super) zoom: f32,
}

impl CameraRig {
    /// Camera position and look-at point for clip progress `t` (0..=1),
    /// orbiting `focus`. `yaw_base` is added to the rig's yaw - the walker
    /// mode passes the direction behind the body, everything else 0 - and
    /// `auto` is the (distance, elevation) the mode framed for itself, used
    /// wherever the rig left the value unset.
    pub(super) fn pose_at(
        &self,
        focus: Vec3,
        yaw_base_deg: f32,
        t: f32,
        auto: (f32, f32),
    ) -> (Vec3, Vec3) {
        let t = t.clamp(0.0, 1.0);
        let look = focus + Vec3::Y * self.lift;
        let auto_dist = auto.0 / self.zoom.max(0.01);
        let (d0, d1) = self.dist.unwrap_or((auto_dist, auto_dist));
        let (e0, e1) = self.elev.unwrap_or((auto.1, auto.1));
        let dist = d0 + (d1 - d0) * t;
        let elev = (e0 + (e1 - e0) * t).to_radians();
        let yaw = (yaw_base_deg + self.yaw + self.sweep * t).to_radians();
        let horiz = dist * elev.cos();
        let pos = look + Vec3::new(horiz * yaw.sin(), dist * elev.sin(), horiz * yaw.cos());
        (pos, look)
    }

    /// The yaw that puts a camera directly behind a body heading along
    /// `dir` (horizontal, world space): the offset from the focus to the
    /// camera is `-dir`, and yaw is `atan2(x, z)` of that offset.
    pub(super) fn yaw_behind(dir: Vec3) -> f32 {
        (-dir.x).atan2(-dir.z).to_degrees()
    }
}

// ---------------------------------------------------------------------------
// The play view (#1360)
// ---------------------------------------------------------------------------

/// Metres the play view stands the camera from its subject - the game's own
/// chase-camera orbit radius, read from [`crate::config::camera`] so the
/// preset cannot drift from the thing it is imitating.
pub(super) const PLAY_DIST: f32 = crate::config::camera::ORBIT_RADIUS;

/// The play view's elevation above the subject, in the degrees the rig
/// speaks - the game's own orbit pitch, which is stored in radians.
pub(super) fn play_elev_deg() -> f32 {
    crate::config::camera::ORBIT_PITCH.to_degrees()
}

/// How many pixels a metre resolves to at `dist`, in a frame `lines` tall on
/// this tool's lens ([`super::FOV`], Bevy's default, which is the lens the
/// game leaves alone).
///
/// The number the whole play view exists to hold: a frame's vertical extent
/// at distance `d` is `2 · d · tan(fov/2)`, so the scale is
/// `lines / (2 · tan(fov/2)) / d` - about `1304 / d` at 1080 lines, and so
/// about 109 px a metre at the chase camera's rest. It is reported in the
/// log rather than measured off the picture: this is a camera, not an
/// instrument.
pub(super) fn px_per_metre(lines: u32, dist: f32) -> f32 {
    lines as f32 / (2.0 * (super::FOV * 0.5).tan() * dist.max(1e-3))
}

/// The frame's *horizontal* half-angle (radians) at `width × height`. The
/// lens is specified vertically, so a line-up's angular spread is checked
/// against this rather than against half of [`super::FOV`].
pub(super) fn half_fov_x(width: u32, height: u32) -> f32 {
    ((super::FOV * 0.5).tan() * width as f32 / height.max(1) as f32).atan()
}

/// Clip progress for frame `i` of `frames`: 0 for a still, and both ends
/// inclusive for a clip so a 360° sweep's last frame is *not* a repeat of
/// its first - the loop point is the step between them.
pub(super) fn progress(i: u32, frames: u32) -> f32 {
    if frames <= 1 {
        0.0
    } else {
        i as f32 / frames as f32
    }
}

/// The delay a GIF frame carries for `fps`, in centiseconds - GIF's unit,
/// which is why the tool's default frame rate is 12.5 (8 cs) rather than a
/// rounder number that the format cannot express. Floored at 2 cs, below
/// which browsers substitute their own (slower) minimum.
pub(super) fn delay_cs(fps: f32) -> u16 {
    ((100.0 / fps.max(0.01)).round() as u16).max(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig() -> CameraRig {
        CameraRig {
            focus: Focus::Origin,
            lift: 0.0,
            dist: Some((10.0, 10.0)),
            elev: Some((0.0, 0.0)),
            yaw: 180.0,
            sweep: 0.0,
            zoom: 1.0,
        }
    }

    const AUTO: (f32, f32) = (99.0, 45.0);

    #[test]
    fn yaw_180_sits_on_the_minus_z_side_like_the_front_tile() {
        let (pos, look) = rig().pose_at(Vec3::ZERO, 0.0, 0.0, AUTO);
        assert!((pos - Vec3::new(0.0, 0.0, -10.0)).length() < 1e-4, "{pos}");
        assert_eq!(look, Vec3::ZERO);
    }

    #[test]
    fn elevation_lifts_the_camera_and_shortens_the_horizontal_leg() {
        let mut r = rig();
        r.elev = Some((30.0, 30.0));
        let (pos, _) = r.pose_at(Vec3::ZERO, 0.0, 0.0, AUTO);
        assert!((pos.y - 5.0).abs() < 1e-4, "{pos}");
        assert!((pos.length() - 10.0).abs() < 1e-4, "{pos}");
    }

    #[test]
    fn a_sweep_interpolates_over_the_clip_and_the_dolly_with_it() {
        let mut r = rig();
        r.sweep = 90.0;
        r.dist = Some((10.0, 20.0));
        let (a, _) = r.pose_at(Vec3::ZERO, 0.0, 0.0, AUTO);
        let (b, _) = r.pose_at(Vec3::ZERO, 0.0, 1.0, AUTO);
        assert!((a - Vec3::new(0.0, 0.0, -10.0)).length() < 1e-4, "{a}");
        // 270°: the camera has come round to the -X side, 20 m out.
        assert!((b - Vec3::new(-20.0, 0.0, 0.0)).length() < 1e-3, "{b}");
    }

    #[test]
    fn an_unset_distance_and_elevation_take_the_framed_values() {
        let mut r = rig();
        r.dist = None;
        r.elev = None;
        let (pos, _) = r.pose_at(Vec3::ZERO, 0.0, 0.0, (20.0, 30.0));
        assert!((pos.length() - 20.0).abs() < 1e-3, "{pos}");
        assert!((pos.y - 10.0).abs() < 1e-3, "{pos}");
        // Zoom divides the framed distance, and only that.
        r.zoom = 2.0;
        let (pos, _) = r.pose_at(Vec3::ZERO, 0.0, 0.0, (20.0, 30.0));
        assert!((pos.length() - 10.0).abs() < 1e-3, "{pos}");
        r.dist = Some((40.0, 40.0));
        let (pos, _) = r.pose_at(Vec3::ZERO, 0.0, 0.0, (20.0, 30.0));
        assert!(
            (pos.length() - 40.0).abs() < 1e-3,
            "an explicit distance is absolute: {pos}"
        );
    }

    #[test]
    fn yaw_behind_a_heading_puts_the_camera_at_the_body_back() {
        // Heading -Z: behind is +Z, which is yaw 0 in this convention.
        assert!(CameraRig::yaw_behind(Vec3::NEG_Z).abs() < 1e-4);
        // Heading +X: behind is -X, yaw -90 (or 270).
        let y = CameraRig::yaw_behind(Vec3::X);
        assert!((y + 90.0).abs() < 1e-4, "{y}");
    }

    #[test]
    fn the_last_frame_of_a_clip_stops_short_of_the_loop_point() {
        assert_eq!(progress(0, 1), 0.0);
        assert_eq!(progress(0, 4), 0.0);
        assert_eq!(progress(3, 4), 0.75);
    }

    #[test]
    fn focus_parses_keywords_and_points() {
        assert_eq!(Focus::parse("origin").unwrap(), Focus::Origin);
        assert_eq!(Focus::parse(" Walker ").unwrap(), Focus::Walker);
        assert_eq!(Focus::parse("settlement").unwrap(), Focus::Settlement);
        assert_eq!(
            Focus::parse("3,-4").unwrap(),
            Focus::Point {
                x: 3.0,
                y: None,
                z: -4.0
            }
        );
        assert_eq!(
            Focus::parse("1,2,3").unwrap(),
            Focus::Point {
                x: 1.0,
                y: Some(2.0),
                z: 3.0
            }
        );
        assert!(Focus::parse("north").is_err());
        assert!(Focus::parse("1").is_err());
    }

    /// #1360. The play view's whole claim is that it is the game's camera,
    /// so every number in it is read from [`crate::config::camera`] or from
    /// Bevy's own projection default rather than written down here. This is
    /// the test that keeps that true: change `ORBIT_RADIUS` in the game and
    /// the preset follows; change the tool's `FOV` away from the lens the
    /// game leaves alone and this fails by name.
    #[test]
    fn the_play_view_is_the_games_own_camera() {
        use crate::config::camera;
        assert_eq!(PLAY_DIST, camera::ORBIT_RADIUS, "distance");
        assert_eq!(play_elev_deg(), camera::ORBIT_PITCH.to_degrees(), "pitch");
        assert!(
            (play_elev_deg() - 22.918).abs() < 1e-2,
            "0.4 rad is about 22.9 degrees, got {}",
            play_elev_deg()
        );
        // The game sets no `fov`, so its lens is Bevy's default. The tool
        // states that as a constant, which is exactly how a lens drifts.
        assert_eq!(
            super::super::FOV,
            bevy::camera::PerspectiveProjection::default().fov,
            "the tool's lens is no longer Bevy's default, which is the game's"
        );
    }

    /// The play view's scale arithmetic, which is what makes "sub-pixel in
    /// play" a checkable statement rather than a feeling (#1359's diagnosis):
    /// 1304/d pixels a metre at 1080 lines.
    #[test]
    fn a_metre_is_about_109_pixels_at_the_chase_cameras_rest() {
        assert!(
            (px_per_metre(1080, 1.0) - 1304.0).abs() < 1.0,
            "{}",
            px_per_metre(1080, 1.0)
        );
        let at_rest = px_per_metre(1080, PLAY_DIST);
        assert!((at_rest - 108.7).abs() < 0.5, "{at_rest}");
        // Half the lines, half the scale; twice the distance, half again.
        assert!((px_per_metre(540, PLAY_DIST) - at_rest * 0.5).abs() < 1e-3);
        assert!((px_per_metre(1080, PLAY_DIST * 2.0) - at_rest * 0.5).abs() < 1e-3);
        // 16:9 is wider than it is tall, so the horizontal half-angle beats
        // the vertical one the lens is specified in.
        let hx = half_fov_x(1920, 1080).to_degrees();
        assert!((hx - 36.36).abs() < 0.05, "{hx}");
        assert!(hx > (super::super::FOV * 0.5).to_degrees());
    }

    #[test]
    fn gif_delays_are_whole_centiseconds() {
        assert_eq!(delay_cs(12.5), 8);
        assert_eq!(delay_cs(10.0), 10);
        assert_eq!(delay_cs(25.0), 4);
        // Faster than the format can say is clamped, not zero.
        assert_eq!(delay_cs(100.0), 2);
    }
}
