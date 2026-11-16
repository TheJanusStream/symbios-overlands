//! WebGL2's cluster index list as Bevy 0.20.1 fills it (#1921): a port of
//! the count `bevy_light`'s CPU cluster assignment
//! (`cluster/assign.rs`, `assign_objects_to_clusters`) makes for a point
//! light, so the budget can ask how much of the list a set of lights will
//! take before it draws them.
//!
//! WebGL2 has no storage buffers, so Bevy keeps each screen cluster's lights
//! in one list of [`INDEX_LIST_LEN`] places over the view's clusters, and
//! past it blocks of the screen go unlit ("Clusterable object index lists
//! are full!"). Bevy fills it by refining each light's sphere slice by
//! slice and row by row against the cluster planes, and counts a place for
//! every cluster the refined sphere touches. An estimate of that missed a
//! light's share by up to 30 % where its sphere reached into view from
//! beside or behind the eye; this is the count itself, line for line, and
//! what it returns is what Bevy counts for the same light, view and far
//! end, as measured in the browser (the tests below).
//!
//! Bevy ends the last depth slice at the farthest light in view, read from
//! the frame before (`Clusters::last_frame_farthest_z`): [`ClusterView::reach`]
//! is one light's part in it and [`ClusterView::slices`] lays the slices out
//! for a far end. The view is the game's: the default [`ClusterConfig`]
//! (4,096 clusters in 24 depth slices, the first 5 m deep), an unscaled
//! camera, and no resizing (the budget keeps the list from overflowing,
//! which is the only thing that makes Bevy resize it). A spot light is
//! counted as a point light of its range, a count on the high side: Bevy
//! culls a spot's clusters by its cone too.
//!
//! [`ClusterConfig`]: bevy::light::cluster::ClusterConfig

use bevy::camera::Projection;
use bevy::math::{Mat4, UVec2, UVec3, Vec2, Vec3, Vec3A, Vec3Swizzles, Vec4, Vec4Swizzles, ops};
use bevy::shape::{HalfSpace, ViewFrustum};

/// The places in WebGL2's cluster index list: Bevy's
/// `view_cluster_bindings_max_indices` where storage buffers are missing.
pub const INDEX_LIST_LEN: u32 = 16_384;

// Bevy 0.20's default `ClusterConfig`, which the game keeps.
const CLUSTER_TOTAL: u32 = 4096;
const Z_SLICES: u32 = 24;
const FIRST_SLICE_M: f32 = 5.0;

/// How the world camera's view is cut into clusters, for one frame: the
/// lens, the screen's tiles and the frustum Bevy culls lights by.
#[derive(Clone, Debug)]
pub struct ClusterView {
    clip_from_view: Mat4,
    far_plane: f32,
    screen: UVec2,
    orthographic: bool,
    first_slice: f32,
    dims: UVec3,
    frustum: ViewFrustum,
    x_planes: Vec<HalfSpace>,
    y_planes: Vec<HalfSpace>,
}

/// The depth slices for one far end, as Bevy lays them out.
#[derive(Clone, Copy, Debug, Default)]
pub struct Slices {
    factors: Vec2,
    /// Each slice's near edge (view space, negative ahead), and the last
    /// slice's far edge.
    planes: [f32; Z_SLICES as usize + 1],
}

impl Default for ClusterView {
    /// Bevy's default 45 degree lens, which the world camera keeps, on a
    /// 1280 x 720 screen.
    fn default() -> Self {
        Self::of(
            &Projection::Perspective(Default::default()),
            UVec2::new(1280, 720),
        )
    }
}

impl ClusterView {
    /// The view of a camera whose projection is `clip_from_view`, its far
    /// plane at `far_plane`, on a screen of `screen` physical pixels (Bevy
    /// clusters none on an empty one; this then reads a 1 x 1 screen).
    pub fn new(clip_from_view: Mat4, far_plane: f32, screen: UVec2) -> Self {
        let mut view = Self {
            clip_from_view: Mat4::NAN,
            far_plane: f32::NAN,
            screen: UVec2::ZERO,
            orthographic: false,
            first_slice: FIRST_SLICE_M,
            dims: UVec3::ONE,
            frustum: ViewFrustum {
                half_spaces: [HalfSpace::default(); 6],
            },
            x_planes: Vec::new(),
            y_planes: Vec::new(),
        };
        view.set(clip_from_view, far_plane, screen);
        view
    }

    /// The view of `projection` on a screen of `screen` physical pixels.
    pub fn of(projection: &Projection, screen: UVec2) -> Self {
        let mut view = Self::new(Mat4::IDENTITY, 1.0, UVec2::ONE);
        view.set_projection(projection, screen);
        view
    }

    /// Become [`Self::of`] `projection` on `screen`.
    pub fn set_projection(&mut self, projection: &Projection, screen: UVec2) {
        let mut projection = projection.clone();
        let screen = screen.max(UVec2::ONE);
        projection.update(screen.x as f32, screen.y as f32);
        self.set(projection.get_clip_from_view(), projection.far(), screen);
    }

    /// Become the view [`Self::new`] makes of these, reusing the planes'
    /// storage, and doing nothing when they are what it already is.
    pub fn set(&mut self, clip_from_view: Mat4, far_plane: f32, screen: UVec2) {
        let screen = screen.max(UVec2::ONE);
        if self.clip_from_view == clip_from_view
            && self.far_plane == far_plane
            && self.screen == screen
        {
            return;
        }
        self.clip_from_view = clip_from_view;
        self.far_plane = far_plane;
        self.screen = screen;
        self.orthographic = clip_from_view.w_axis.w == 1.0;
        self.first_slice = if self.orthographic {
            (clip_from_view.w_axis.z - 1.0) / clip_from_view.z_axis.z
        } else {
            FIRST_SLICE_M
        };
        // `ClusterConfig::dimensions_for_screen_size`, then
        // `Clusters::update`.
        let aspect = screen.x as f32 / screen.y as f32;
        let per_layer = CLUSTER_TOTAL as f32 / Z_SLICES as f32;
        let down = (per_layer / aspect).sqrt();
        let (mut x, mut y) = ((down * aspect) as u32, down as u32);
        if x == 0 {
            (x, y) = (1, per_layer as u32);
        }
        if y == 0 {
            (x, y) = (per_layer as u32, 1);
        }
        let tile = (screen.as_vec2() / UVec2::new(x, y).as_vec2())
            .ceil()
            .as_uvec2()
            .max(UVec2::ONE);
        self.dims = (screen.as_vec2() / tile.as_vec2())
            .ceil()
            .as_uvec2()
            .extend(Z_SLICES)
            .max(UVec3::ONE);
        // `Frustum` as `compute_frustum` makes it, in view space.
        self.frustum = ViewFrustum::from_clip_from_world_custom_far(
            &clip_from_view,
            &Vec3::ZERO,
            &Vec3::Z,
            far_plane,
        );
        let view_from_clip = clip_from_view.inverse();
        let to_view = |clip: Vec4| {
            let v = view_from_clip * clip;
            v / v.w
        };
        self.x_planes.clear();
        self.y_planes.clear();
        let (across, down) = (self.dims.x as f32, self.dims.y as f32);
        for i in 0..=self.dims.x {
            let x = i as f32 / across * 2.0 - 1.0;
            self.x_planes.push(if self.orthographic {
                HalfSpace::new(Vec3::X.extend(to_view(Vec4::new(x, 0.0, 1.0, 1.0)).x))
            } else {
                let nb = to_view(Vec4::new(x, -1.0, 1.0, 1.0)).xyz();
                let nt = to_view(Vec4::new(x, 1.0, 1.0, 1.0)).xyz();
                let normal = nb.cross(nt);
                HalfSpace::new(normal.extend(nb.dot(normal)))
            });
        }
        for j in 0..=self.dims.y {
            let y = (1.0 - j as f32 / down) * 2.0 - 1.0;
            self.y_planes.push(if self.orthographic {
                HalfSpace::new(Vec3::Y.extend(to_view(Vec4::new(0.0, y, 1.0, 1.0)).y))
            } else {
                let nl = to_view(Vec4::new(-1.0, y, 1.0, 1.0)).xyz();
                let nr = to_view(Vec4::new(1.0, y, 1.0, 1.0)).xyz();
                let normal = nr.cross(nl);
                HalfSpace::new(normal.extend(nr.dot(normal)))
            });
        }
    }

    /// A light of `range` at `centre` (view space)'s part in where Bevy ends
    /// the last slice next frame: how far its sphere reaches, if it is in
    /// the view at all (`None` for one Bevy culls, which takes no place).
    pub fn reach(&self, centre: Vec3, range: f32) -> Option<f32> {
        let at = centre.extend(1.0);
        self.frustum
            .half_spaces
            .iter()
            .all(|half| half.normal_d().dot(at) + range > 0.0)
            .then_some(-centre.z + range)
    }

    /// The slices Bevy lays out when the farthest light it drew last frame
    /// reached `farthest` (0 for none in view).
    pub fn slices(&self, farthest: f32) -> Slices {
        let far = farthest.max(self.first_slice);
        let near = self.first_slice;
        let factors = if self.orthographic {
            Vec2::new(-near, Z_SLICES as f32 / (-far - -near))
        } else {
            let k = (Z_SLICES as f32 - 1.0) / ops::ln(far / near);
            Vec2::new(k, ops::ln(near) * k)
        };
        let mut planes = [0.0; Z_SLICES as usize + 1];
        for (k, plane) in planes.iter_mut().enumerate() {
            let k = k as u32;
            *plane = if self.orthographic {
                -near - (far - near) * k as f32 / Z_SLICES as f32
            } else if k == 0 {
                0.0
            } else {
                -near * ops::powf(far / near, (k - 1) as f32 / (Z_SLICES - 1) as f32)
            };
        }
        Slices { factors, planes }
    }

    /// The places in the cluster index list a light of `range` at `centre`
    /// (view space) takes under `slices`: Bevy's own count for a point
    /// light, 0 for one out of the view.
    pub fn count(&self, slices: &Slices, centre: Vec3, range: f32) -> u32 {
        if self.reach(centre, range).is_none() {
            return 0;
        }
        let ortho = self.orthographic;
        let dims = self.dims;
        let (lo, hi) = self.aabb(centre, range);
        let lo_cluster = self.cluster_of(slices, lo, lo.z);
        let hi_cluster = self.cluster_of(slices, hi, hi.z);
        let (lo_cluster, hi_cluster) = (lo_cluster.min(hi_cluster), lo_cluster.max(hi_cluster));
        let sphere = (Vec3A::from(centre), range);
        let clip = self.clip_from_view * centre.extend(1.0);
        let ndc = clip.xyz() / clip.w;
        let at = self.cluster_of(slices, ndc, centre.z);
        let z_centre = (ndc.z <= 1.0).then_some(at.z);
        let y_centre = if ndc.y > 1.0 {
            None
        } else if ndc.y < -1.0 {
            Some(dims.y + 1)
        } else {
            Some(at.y)
        };
        let mut total = 0;
        for z in lo_cluster.z..=hi_cluster.z {
            let mut z_sphere = sphere;
            if z_centre != Some(z) {
                // The plane nearer the light cuts the wider circle.
                let plane = if z_centre.is_some_and(|c| z < c) {
                    slices.planes[(z + 1) as usize]
                } else {
                    slices.planes[z as usize]
                };
                let gap = plane - z_sphere.0.z;
                if gap.abs() > z_sphere.1 {
                    continue;
                }
                z_sphere = (
                    Vec3A::from(z_sphere.0.xy().extend(plane)),
                    (z_sphere.1 * z_sphere.1 - gap * gap).sqrt(),
                );
            }
            for y in lo_cluster.y..=hi_cluster.y {
                let mut y_sphere = z_sphere;
                if y_centre != Some(y) {
                    let plane = if y_centre.is_some_and(|c| y < c) {
                        self.y_planes[(y + 1) as usize]
                    } else {
                        self.y_planes[y as usize]
                    };
                    let gap = if ortho {
                        plane.d() - y_sphere.0.y
                    } else {
                        -y_sphere.0.yz().dot(plane.normal_d().yz())
                    };
                    if gap.abs() > y_sphere.1 {
                        continue;
                    }
                    y_sphere = (
                        y_sphere.0 + gap * plane.normal(),
                        (y_sphere.1 * y_sphere.1 - gap * gap).sqrt(),
                    );
                }
                let distance_x = |plane: HalfSpace| {
                    if ortho {
                        y_sphere.0.x - plane.d()
                    } else {
                        plane.normal_d().xz().dot(y_sphere.0.xz())
                    }
                };
                let mut lo_x = lo_cluster.x;
                while lo_x < hi_cluster.x
                    && -distance_x(self.x_planes[(lo_x + 1) as usize]) + y_sphere.1 <= 0.0
                {
                    lo_x += 1;
                }
                let mut hi_x = hi_cluster.x;
                while hi_x > lo_x && distance_x(self.x_planes[hi_x as usize]) + y_sphere.1 <= 0.0 {
                    hi_x -= 1;
                }
                total += hi_x - lo_x + 1;
            }
        }
        total
    }

    /// `cluster_space_clusterable_object_aabb`: the light's box, x and y in
    /// NDC clamped to the screen, z in view space.
    fn aabb(&self, centre: Vec3, range: f32) -> (Vec3, Vec3) {
        let half = Vec3::splat(range);
        let (mut lo, mut hi) = (centre - half, centre + half);
        lo.z = lo.z.min(-f32::MIN_POSITIVE);
        hi.z = hi.z.min(-f32::MIN_POSITIVE);
        let ndc = |p: Vec3| {
            let clip = self.clip_from_view * p.extend(1.0);
            clip.xyz() / clip.w
        };
        let corners = [
            ndc(lo),
            ndc(lo.xy().extend(hi.z)),
            ndc(hi.xy().extend(lo.z)),
            ndc(hi),
        ];
        let min = corners[0].min(corners[1]).min(corners[2]).min(corners[3]);
        let max = corners[0].max(corners[1]).max(corners[2]).max(corners[3]);
        (
            min.xy().clamp(Vec2::NEG_ONE, Vec2::ONE).extend(lo.z),
            max.xy().clamp(Vec2::NEG_ONE, Vec2::ONE).extend(hi.z),
        )
    }

    /// `ndc_position_to_cluster`.
    fn cluster_of(&self, slices: &Slices, ndc: Vec3, view_z: f32) -> UVec3 {
        let dims = self.dims.as_vec3();
        let frag =
            (ndc.xy() * Vec2::new(0.5, -0.5) + Vec2::splat(0.5)).clamp(Vec2::ZERO, Vec2::ONE);
        let xy = (frag * dims.xy()).floor();
        let z = if self.orthographic {
            ((view_z - slices.factors.x) * slices.factors.y).floor() as u32
        } else {
            (ops::ln(-view_z) * slices.factors.x - slices.factors.y + 1.0) as u32
        };
        xy.as_uvec2()
            .extend(z.min(Z_SLICES - 1))
            .clamp(UVec3::ZERO, self.dims - UVec3::ONE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Affine3A;
    use bevy::prelude::*;

    /// The browser bench's eye (#1921): 1.7 m up, looking a little down,
    /// on Chromium's 1280 x 720 WebGL2 canvas.
    fn bench_eye() -> Affine3A {
        GlobalTransform::from(
            Transform::from_xyz(0.0, 1.7, 0.0).looking_to(Vec3::new(0.0, -0.17, -1.0), Vec3::Y),
        )
        .affine()
        .inverse()
    }

    /// The bench's grid: a light every `spacing` m, 2 m up, 41 x 41.
    fn grid(spacing: f32) -> Vec<Vec3> {
        let mut spots = Vec::new();
        for i in -20..=20 {
            for j in -20..=20 {
                spots.push(Vec3::new(
                    i as f32 * spacing + 0.3,
                    2.0,
                    j as f32 * spacing + 0.2,
                ));
            }
        }
        spots
    }

    /// Bevy's count of the `n` lights of `range` nearest the bench's eye,
    /// the last slice ending at the farthest, as Bevy settles on it.
    fn nearest_count(spacing: f32, range: f32, n: usize) -> (u32, f32) {
        let eye = Vec3::new(0.0, 1.7, 0.0);
        let mut spots = grid(spacing);
        spots.sort_by(|a, b| a.distance(eye).total_cmp(&b.distance(eye)));
        spots.truncate(n);
        let to_view = bench_eye();
        let view = ClusterView::default();
        let centres: Vec<Vec3> = spots
            .iter()
            .map(|&at| to_view.transform_point3(at))
            .collect();
        let farthest = centres
            .iter()
            .filter_map(|&c| view.reach(c, range))
            .fold(0.0, f32::max);
        let slices = view.slices(farthest);
        (
            centres.iter().map(|&c| view.count(&slices, c, range)).sum(),
            farthest,
        )
    }

    /// The count is Bevy's, to the place, for the sets Chromium's WebGL2
    /// logged in the bench (`Clusters::last_frame_total_cluster_index_count`,
    /// #1921): the 64 lamps nearest the eye on a 4 m grid overflow the list
    /// at 19,165 places, and on a 5.6 m grid take 10,122.
    #[test]
    fn the_count_is_bevys_as_the_browser_logged_it() {
        let (packed, far) = nearest_count(4.0, 5.6, 64);
        assert!((far - 21.126_245).abs() < 1e-3, "{far}");
        assert_eq!(packed, 19_165);
        let (spaced, far) = nearest_count(5.6, 5.6, 64);
        assert!((far - 27.435_722).abs() < 1e-3, "{far}");
        assert_eq!(spaced, 10_122);
    }

    /// A light beside or just behind the eye whose sphere reaches into the
    /// view takes places - Bevy counts it - while one wholly behind, or
    /// past the view's side, takes none.
    #[test]
    fn a_light_reaching_in_from_beside_the_eye_is_counted() {
        let view = ClusterView::default();
        let slices = view.slices(40.0);
        assert!(view.count(&slices, Vec3::new(6.0, 0.0, 1.0), 20.0) > 100);
        assert!(view.reach(Vec3::new(6.0, 0.0, 1.0), 20.0).is_some());
        assert_eq!(view.count(&slices, Vec3::new(0.0, 0.0, 30.0), 5.6), 0);
        assert_eq!(view.count(&slices, Vec3::new(40.0, 0.0, -5.0), 5.6), 0);
        assert!(view.count(&slices, Vec3::new(0.0, 0.0, -20.0), 5.6) > 0);
    }
}
