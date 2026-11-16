//! WebGL2's cluster index list as Bevy 0.20.1 fills it (#1921): a port of
//! the count `bevy_light`'s CPU cluster assignment
//! (`cluster/assign.rs`, `assign_objects_to_clusters`) makes for a point
//! light and for a spot light (#1926), so the budget can ask how much of
//! the list a set of lights will take before it draws them.
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
//! end, as measured in the browser and as Bevy's own assignment counts it
//! (the tests below).
//!
//! A spot light takes only the clusters of its sphere its cone reaches
//! ([`Cone`]): Bevy tests each cluster the sphere touches against the cone,
//! by the sphere round the cluster's box (`compute_aabb_for_cluster`) and
//! the spot's way and outer angle - its angle, front and back culls - and
//! counts a place for each cluster that passes. That is ported line for
//! line too, in the same float operations, so a beam is counted as Bevy
//! counts it: a headlight seen from behind its car takes about a tenth of
//! the places its sphere would, and one seen from ahead about a third.
//!
//! Bevy ends the last depth slice at the farthest light in view, read from
//! the frame before (`Clusters::last_frame_farthest_z`): [`ClusterView::reach`]
//! is one light's part in it and [`ClusterView::slices`] lays the slices out
//! for a far end. The view is the game's: the default [`ClusterConfig`]
//! (4,096 clusters in 24 depth slices, the first 5 m deep) and no resizing
//! (the budget keeps the list from overflowing, which is the only thing
//! that makes Bevy resize it). The camera's scale is read as Bevy reads it
//! ([`ClusterView::set_eye`]): a camera's transform decomposes to a scale a
//! hair off 1, and Bevy takes the first slice's depth and each light's
//! sphere through it.
//!
//! # What is kept between counts (#1926)
//!
//! A spot's count tests every cluster its sphere touches against the cone,
//! by the sphere round the cluster's box, and those spheres hang on the
//! lens, the screen and the slices alone - not on the light or on where the
//! camera stands. A [`Slices`] keeps them once made, a row of clusters at a
//! time, for as long as it stays laid out for the same far end of the same
//! view; laid out again for another, it makes them again as they are asked
//! for. A cluster's depth is read from the slice planes the slices hold,
//! which are the very expressions `compute_aabb_for_cluster` works out
//! again for each cluster, so the spheres are Bevy's to the bit; and the
//! cone test runs a row at a time over spheres stored side by side, the
//! same operations as Bevy's for each cluster.
//!
//! [`ClusterConfig`]: bevy::light::cluster::ClusterConfig

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

use bevy::camera::Projection;
use bevy::math::{Mat4, UVec2, UVec3, Vec2, Vec3, Vec3A, Vec3Swizzles, Vec4, Vec4Swizzles, ops};
use bevy::shape::{HalfSpace, ViewFrustum};
use bevy::transform::components::GlobalTransform;

/// The places in WebGL2's cluster index list: Bevy's
/// `view_cluster_bindings_max_indices` where storage buffers are missing.
pub const INDEX_LIST_LEN: u32 = 16_384;

// Bevy 0.20's default `ClusterConfig`, which the game keeps.
const CLUSTER_TOTAL: u32 = 4096;
const Z_SLICES: u32 = 24;
const FIRST_SLICE_M: f32 = 5.0;

/// Numbers each lens and screen any view takes up, across every view, so
/// slices laid out by one view are never read as another's.
static LAYOUTS: AtomicU64 = AtomicU64::new(1);

/// How the world camera's view is cut into clusters, for one frame: the
/// lens, the screen's tiles and the frustum Bevy culls lights by.
#[derive(Clone, Debug)]
pub struct ClusterView {
    clip_from_view: Mat4,
    far_plane: f32,
    screen: UVec2,
    orthographic: bool,
    /// The lens's first slice depth, before the camera's scale.
    lens_first: f32,
    /// The camera's view-from-world scale and its largest magnitude, as
    /// Bevy reads them ([`Self::set_eye`]).
    scale: Vec3,
    scale_max: f32,
    dims: UVec3,
    frustum: ViewFrustum,
    x_planes: Vec<HalfSpace>,
    y_planes: Vec<HalfSpace>,
    /// A number for this lens and screen, new each time either changes and
    /// never shared with another view's, so slices laid out for another
    /// lens make their cluster spheres again.
    generation: u64,
    /// Each screen tile's corners in view space, as
    /// `compute_aabb_for_cluster` unprojects them (row by row): the near
    /// plane's for a perspective lens, at depth 0 for an orthographic one.
    corners: Vec<(Vec3, Vec3)>,
    /// The same corners a coordinate at a time - the low corner's x, y and
    /// z, then the high one's - for making a row's spheres four at a time.
    tiles: [Vec<f32>; 6],
}

/// A spot light's cone, as Bevy's assignment culls clusters by it (#1926):
/// the spot's back - its local `+Z`, the way back toward its lamp, a Bevy
/// spot shining down its `-Z` - in view space, and the sine and cosine of
/// its outer angle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cone {
    back: Vec3,
    sin: f32,
    cos: f32,
}

impl Cone {
    /// The cone of a spot whose back is `back` in the world (its
    /// `GlobalTransform::back`) and whose outer angle is `outer` (radians),
    /// seen from a camera whose view-from-world matrix is
    /// `view_from_world`: taken as Bevy takes it, in the same operations.
    pub fn new(view_from_world: &Mat4, back: Vec3, outer: f32) -> Self {
        let (sin, cos) = ops::sin_cos(outer);
        Self {
            back: (*view_from_world * back.extend(0.0)).truncate().normalize(),
            sin,
            cos,
        }
    }
}

/// The depth slices for one far end, as Bevy lays them out.
#[derive(Clone, Debug, Default)]
pub struct Slices {
    /// Where the first slice ends and the last one does (m): Bevy's
    /// `first_slice_depth` and `far_z`.
    near: f32,
    far: f32,
    factors: Vec2,
    /// Each slice's near edge (view space, negative ahead), and the last
    /// slice's far edge.
    planes: [f32; Z_SLICES as usize + 1],
    /// What they were laid out for: the farthest reach asked about, and
    /// the view's lens and screen ([`ClusterView::generation`]); the first
    /// slice, which the camera's scale moves, is `near`.
    farthest: f32,
    generation: u64,
    /// The cluster spheres a spot's cull reads, made as they are asked for
    /// (see the module docs).
    spheres: RefCell<Spheres>,
}

/// Each cluster's sphere under one [`Slices`], for a spot's cull: the
/// sphere round its box, as Bevy's assignment makes it
/// (`cluster_aabb_spheres`), stored side by side along each row of
/// clusters - one depth slice's along one screen row, index
/// `(z * rows + y) * across + x` - so a row's cone test runs over
/// neighbours.
#[derive(Clone, Debug, Default)]
struct Spheres {
    /// Whether each row's spheres are made yet, by row (`z * rows + y`).
    made: Vec<bool>,
    x: Vec<f32>,
    y: Vec<f32>,
    z: Vec<f32>,
    radius: Vec<f32>,
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
            lens_first: FIRST_SLICE_M,
            scale: Vec3::ONE,
            scale_max: 1.0,
            dims: UVec3::ONE,
            frustum: ViewFrustum {
                half_spaces: [HalfSpace::default(); 6],
            },
            x_planes: Vec::new(),
            y_planes: Vec::new(),
            generation: 0,
            corners: Vec::new(),
            tiles: Default::default(),
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
        self.generation = LAYOUTS.fetch_add(1, Ordering::Relaxed);
        self.orthographic = clip_from_view.w_axis.w == 1.0;
        self.lens_first = if self.orthographic {
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
        // `compute_aabb_for_cluster`'s tile corners, which hang on the
        // screen alone: its near-plane points for a perspective lens.
        let (tile, screen) = (tile.as_vec2(), screen.as_vec2());
        let ndc_z = if self.orthographic { 0.0 } else { 1.0 };
        self.corners.clear();
        for y in 0..self.dims.y {
            for x in 0..self.dims.x {
                let p_min = UVec2::new(x, y).as_vec2() * tile;
                let p_max = p_min + tile;
                self.corners.push((
                    screen_to_view(screen, view_from_clip, p_min, ndc_z).xyz(),
                    screen_to_view(screen, view_from_clip, p_max, ndc_z).xyz(),
                ));
            }
        }
        for (k, store) in self.tiles.iter_mut().enumerate() {
            store.clear();
            store.extend(self.corners.iter().map(|&(lo, hi)| {
                let corner = if k < 3 { lo } else { hi };
                corner[k % 3]
            }));
        }
    }

    /// Read the camera's scale as Bevy's assignment reads it, from its
    /// pose `eye`: the scale its transform decomposes to, inverted - a hair
    /// off 1 for most turns of an unscaled camera. Bevy's first slice is
    /// 5 m times its depth part, and a light's sphere grows by its largest.
    /// Until this is called the view reads an unscaled camera.
    pub fn set_eye(&mut self, eye: &GlobalTransform) {
        let scale = eye.compute_transform().scale.recip();
        self.scale = scale;
        self.scale_max = scale.abs().max_element();
    }

    /// Where Bevy ends the first slice: the lens's depth times the camera's
    /// depth scale.
    fn first_slice(&self) -> f32 {
        self.lens_first * self.scale.z
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
            .then_some(-centre.z + range * self.scale.z)
    }

    /// The slices Bevy lays out when the farthest light it drew last frame
    /// reached `farthest` (0 for none in view).
    pub fn slices(&self, farthest: f32) -> Slices {
        let mut slices = Slices::default();
        self.reslice(&mut slices, farthest);
        slices
    }

    /// Whether `slices` are laid out as [`Self::slices`] would lay them out
    /// for `farthest` now: for this lens and screen, this first slice and
    /// this far end.
    pub fn laid_out(&self, slices: &Slices, farthest: f32) -> bool {
        slices.generation == self.generation
            && slices.near.to_bits() == self.first_slice().to_bits()
            && slices.farthest.to_bits() == farthest.to_bits()
    }

    /// Lay `slices` out again as [`Self::slices`] would, keeping the
    /// cluster spheres they hold when they are laid out so already (and
    /// the store they are kept in, whatever).
    pub fn reslice(&self, slices: &mut Slices, farthest: f32) {
        let rows = (self.dims.y * self.dims.z) as usize;
        let clusters = rows * self.dims.x as usize;
        if self.laid_out(slices, farthest) && slices.spheres.get_mut().made.len() == rows {
            return;
        }
        let spheres = slices.spheres.get_mut();
        spheres.made.clear();
        spheres.made.resize(rows, false);
        for store in [
            &mut spheres.x,
            &mut spheres.y,
            &mut spheres.z,
            &mut spheres.radius,
        ] {
            store.resize(clusters, 0.0);
        }
        slices.farthest = farthest;
        slices.generation = self.generation;
        let near = self.first_slice();
        let far = farthest.max(near);
        slices.near = near;
        slices.far = far;
        slices.factors = if self.orthographic {
            Vec2::new(-near, Z_SLICES as f32 / (-far - -near))
        } else {
            let k = (Z_SLICES as f32 - 1.0) / ops::ln(far / near);
            Vec2::new(k, ops::ln(near) * k)
        };
        for (k, plane) in slices.planes.iter_mut().enumerate() {
            let k = k as u32;
            *plane = if self.orthographic {
                -near - (far - near) * k as f32 / Z_SLICES as f32
            } else if k == 0 {
                0.0
            } else {
                -near * ops::powf(far / near, (k - 1) as f32 / (Z_SLICES - 1) as f32)
            };
        }
    }

    /// The places in the cluster index list a light of `range` at `centre`
    /// (view space) takes under `slices`: Bevy's own count for a point
    /// light, or for a spot light of `cone` (#1926); 0 for one out of the
    /// view.
    pub fn count(&self, slices: &Slices, centre: Vec3, range: f32, cone: Option<Cone>) -> u32 {
        if self.reach(centre, range).is_none() {
            return 0;
        }
        let ortho = self.orthographic;
        let dims = self.dims;
        let (lo, hi) = self.aabb(centre, range);
        let lo_cluster = self.cluster_of(slices, lo, lo.z);
        let hi_cluster = self.cluster_of(slices, hi, hi.z);
        let (lo_cluster, hi_cluster) = (lo_cluster.min(hi_cluster), lo_cluster.max(hi_cluster));
        // The light's sphere in view space, its radius grown by the
        // camera's largest scale.
        let radius = range * self.scale_max;
        let sphere = (Vec3A::from(centre), radius);
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
        let mut spheres = cone.map(|_| slices.spheres.borrow_mut());
        let test = cone.map(|cone| ConeTest::new(centre, radius, cone));
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
                match (&test, spheres.as_deref_mut()) {
                    // Each cluster the refined sphere touches, culled by the
                    // cone (Bevy's "cull that cone" test, with its sphere
                    // round the cluster's box).
                    (Some(test), Some(spheres)) => {
                        let row = (z * dims.y + y) as usize;
                        if !spheres.made[row] {
                            self.make_row(slices, spheres, y, z);
                        }
                        let first = row * dims.x as usize;
                        total +=
                            test.run(spheres, first + lo_x as usize..first + hi_x as usize + 1);
                    }
                    _ => total += hi_x - lo_x + 1,
                }
            }
        }
        total
    }

    /// The places `lights` (each its centre in view space, its range and a
    /// spot's cone) take together, the last slice ending at the farthest:
    /// a still view's count, for tests and measurement.
    #[cfg(test)]
    pub(crate) fn cost(&self, lights: &[(Vec3, f32, Option<Cone>)]) -> u32 {
        let far = lights
            .iter()
            .filter_map(|&(c, r, _)| self.reach(c, r))
            .fold(0.0, f32::max);
        let slices = self.slices(far);
        lights
            .iter()
            .map(|&(c, r, cone)| self.count(&slices, c, r, cone))
            .sum()
    }

    /// `cluster_space_clusterable_object_aabb`: the light's box, x and y in
    /// NDC clamped to the screen, z in view space.
    fn aabb(&self, centre: Vec3, range: f32) -> (Vec3, Vec3) {
        let half = range * self.scale.abs();
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

    /// Make the spheres of the clusters along screen row `y` of depth slice
    /// `z` under `slices`, into `spheres`: [`Self::cluster_sphere`]'s, four
    /// at a time - each lane's arithmetic is the one cluster's, rounded
    /// alike (a sum with nothing comes out as the thing itself, so where
    /// `line_intersection_to_z_plane` adds the eye, at the origin, this
    /// adds nothing) - and the row's last few one by one.
    fn make_row(&self, slices: &Slices, spheres: &mut Spheres, y: u32, z: u32) {
        let row = (z * self.dims.y + y) as usize;
        let first = row * self.dims.x as usize;
        let tiles = (y * self.dims.x) as usize;
        let across = self.dims.x as usize;
        let whole = across - across % 4;
        let near = Vec4::splat(slices.planes[z as usize]);
        let far = Vec4::splat(slices.planes[z as usize + 1]);
        let [tx, ty, tz, ux, uy, uz] = &self.tiles;
        let half = Vec4::splat(0.5);
        for x in (0..whole).step_by(4) {
            let t = tiles + x;
            let (ax, ay, az) = (
                Vec4::from_slice(&tx[t..t + 4]),
                Vec4::from_slice(&ty[t..t + 4]),
                Vec4::from_slice(&tz[t..t + 4]),
            );
            let (bx, by, bz) = (
                Vec4::from_slice(&ux[t..t + 4]),
                Vec4::from_slice(&uy[t..t + 4]),
                Vec4::from_slice(&uz[t..t + 4]),
            );
            let (lo_x, lo_y, lo_z, hi_x, hi_y, hi_z) = if self.orthographic {
                (
                    ax.min(bx),
                    ay.min(by),
                    near.min(far),
                    ax.max(bx),
                    ay.max(by),
                    near.max(far),
                )
            } else {
                let (a_near, a_far) = (near / az, far / az);
                let (b_near, b_far) = (near / bz, far / bz);
                let (px, py, pz) = (a_near * ax, a_near * ay, a_near * az);
                let (qx, qy, qz) = (a_far * ax, a_far * ay, a_far * az);
                let (rx, ry, rz) = (b_near * bx, b_near * by, b_near * bz);
                let (sx, sy, sz) = (b_far * bx, b_far * by, b_far * bz);
                (
                    px.min(qx).min(rx.min(sx)),
                    py.min(qy).min(ry.min(sy)),
                    pz.min(qz).min(rz.min(sz)),
                    px.max(qx).max(rx.max(sx)),
                    py.max(qy).max(ry.max(sy)),
                    pz.max(qz).max(rz.max(sz)),
                )
            };
            let (hx, hy, hz) = (
                half * (hi_x - lo_x),
                half * (hi_y - lo_y),
                half * (hi_z - lo_z),
            );
            let at = first + x;
            (half * (hi_x + lo_x)).write_to_slice(&mut spheres.x[at..at + 4]);
            (half * (hi_y + lo_y)).write_to_slice(&mut spheres.y[at..at + 4]);
            (half * (hi_z + lo_z)).write_to_slice(&mut spheres.z[at..at + 4]);
            (hx * hx + hy * hy + hz * hz)
                .sqrt()
                .write_to_slice(&mut spheres.radius[at..at + 4]);
        }
        for x in whole as u32..self.dims.x {
            let (middle, radius) = self.cluster_sphere(slices, UVec3::new(x, y, z));
            let at = first + x as usize;
            spheres.x[at] = middle.x;
            spheres.y[at] = middle.y;
            spheres.z[at] = middle.z;
            spheres.radius[at] = radius;
        }
        spheres.made[row] = true;
    }

    /// The sphere round cluster `ijk`'s box under `slices`, as Bevy's
    /// assignment makes it for a spot's cull: `compute_aabb_for_cluster`,
    /// then the box's middle and the length of its half extents. The
    /// cluster's near and far depths are read from the slice planes: the
    /// same expressions as `compute_aabb_for_cluster`'s own, in the same
    /// operations (its `-far / -near` is `far / near` to the bit, and so is
    /// its orthographic `-near + (near - far) k / slices`).
    fn cluster_sphere(&self, slices: &Slices, ijk: UVec3) -> (Vec3A, f32) {
        let (p_min, p_max) = self.corners[(ijk.y * self.dims.x + ijk.x) as usize];
        let cluster_near = slices.planes[ijk.z as usize];
        let cluster_far = slices.planes[ijk.z as usize + 1];
        let (lo, hi) = if self.orthographic {
            let (mut p_min, mut p_max) = (p_min, p_max);
            p_min.z = cluster_near;
            p_max.z = cluster_far;
            (p_min.min(p_max), p_min.max(p_max))
        } else {
            let p_min_near = line_intersection_to_z_plane(Vec3::ZERO, p_min, cluster_near);
            let p_min_far = line_intersection_to_z_plane(Vec3::ZERO, p_min, cluster_far);
            let p_max_near = line_intersection_to_z_plane(Vec3::ZERO, p_max, cluster_near);
            let p_max_far = line_intersection_to_z_plane(Vec3::ZERO, p_max, cluster_far);
            (
                p_min_near.min(p_min_far).min(p_max_near.min(p_max_far)),
                p_min_near.max(p_min_far).max(p_max_near.max(p_max_far)),
            )
        };
        // `Aabb::from_min_max`.
        let (lo, hi) = (Vec3A::from(lo), Vec3A::from(hi));
        (0.5 * (hi + lo), (0.5 * (hi - lo)).length())
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

/// A spot's cone test against a row of cluster spheres ([`Self::run`]):
/// the light's middle, its sphere's radius and its cone, each also four
/// times over, for four clusters at a time.
struct ConeTest {
    centre: Vec3,
    radius: f32,
    cone: Cone,
    lanes: [Vec4; 9],
}

impl ConeTest {
    fn new(centre: Vec3, radius: f32, cone: Cone) -> Self {
        let Cone { back, sin, cos } = cone;
        Self {
            centre,
            radius,
            cone,
            lanes: [
                centre.x, centre.y, centre.z, back.x, back.y, back.z, sin, cos, radius,
            ]
            .map(Vec4::splat),
        }
    }

    /// How many clusters of `run` (a stretch of one row in `spheres`) the
    /// spot reaches: Bevy's cone test for each, its angle, front and back
    /// culls, in Bevy's own operations (`Vec3::length_squared` and
    /// `Vec3::dot` sum left to right), four clusters at a time - each
    /// lane's arithmetic is the one cluster's, rounded alike.
    fn run(&self, spheres: &Spheres, run: std::ops::Range<usize>) -> u32 {
        let (xs, ys) = (&spheres.x[run.clone()], &spheres.y[run.clone()]);
        let (zs, rs) = (&spheres.z[run.clone()], &spheres.radius[run]);
        let whole = xs.len() - xs.len() % 4;
        let [cx, cy, cz, bx, by, bz, sin4, cos4, radius4] = self.lanes;
        let mut culled = 0;
        for k in (0..whole).step_by(4) {
            let (ox, oy, oz) = (
                cx - Vec4::from_slice(&xs[k..k + 4]),
                cy - Vec4::from_slice(&ys[k..k + 4]),
                cz - Vec4::from_slice(&zs[k..k + 4]),
            );
            let r = Vec4::from_slice(&rs[k..k + 4]);
            let distance_sq = ox * ox + oy * oy + oz * oz;
            let v1_len = ox * bx + oy * by + oz * bz;
            let closest = (cos4 * (distance_sq - v1_len * v1_len).sqrt()) - v1_len * sin4;
            let cull = closest.cmpgt(r) | v1_len.cmpgt(r + radius4) | v1_len.cmplt(-r);
            culled += cull.bitmask().count_ones();
        }
        let (centre, Cone { back, sin, cos }) = (self.centre, self.cone);
        let mut total = whole as u32 - culled;
        for k in whole..xs.len() {
            let (ox, oy, oz) = (centre.x - xs[k], centre.y - ys[k], centre.z - zs[k]);
            let r = rs[k];
            let distance_sq = ox * ox + oy * oy + oz * oz;
            let v1_len = ox * back.x + oy * back.y + oz * back.z;
            let closest = (cos * (distance_sq - v1_len * v1_len).sqrt()) - v1_len * sin;
            let angle_cull = closest > r;
            let front_cull = v1_len > r + self.radius;
            let back_cull = v1_len < -r;
            total += u32::from(!angle_cull & !front_cull & !back_cull);
        }
        total
    }
}

/// Bevy's `screen_to_view`: a point on the screen (physical pixels) at
/// NDC depth `ndc_z`, in view space.
fn screen_to_view(screen_size: Vec2, view_from_clip: Mat4, screen: Vec2, ndc_z: f32) -> Vec4 {
    let tex_coord = screen / screen_size;
    let clip = Vec4::new(
        tex_coord.x * 2.0 - 1.0,
        (1.0 - tex_coord.y) * 2.0 - 1.0,
        ndc_z,
        1.0,
    );
    let view = view_from_clip * clip;
    view / view.w
}

/// Bevy's `line_intersection_to_z_plane`: where the line from `origin`
/// through `p` meets the plane at view depth `z`.
fn line_intersection_to_z_plane(origin: Vec3, p: Vec3, z: f32) -> Vec3 {
    let v = p - origin;
    let t = (z - Vec3::Z.dot(origin)) / Vec3::Z.dot(v);
    origin + t * v
}

#[cfg(test)]
mod tests {
    use super::Cone;
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
            centres
                .iter()
                .map(|&c| view.count(&slices, c, range, None))
                .sum(),
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
        assert!(view.count(&slices, Vec3::new(6.0, 0.0, 1.0), 20.0, None) > 100);
        assert!(view.reach(Vec3::new(6.0, 0.0, 1.0), 20.0).is_some());
        assert_eq!(view.count(&slices, Vec3::new(0.0, 0.0, 30.0), 5.6, None), 0);
        assert_eq!(
            view.count(&slices, Vec3::new(40.0, 0.0, -5.0), 5.6, None),
            0
        );
        assert!(view.count(&slices, Vec3::new(0.0, 0.0, -20.0), 5.6, None) > 0);
    }

    /// One light for [`bevy_counts`]: its pose (a spot shines down its
    /// `-Z`), its range, and a spot's outer angle (radians); `None` for a
    /// point light.
    type Light = (Transform, f32, Option<f32>);

    /// Bevy 0.20.1's own CPU cluster assignment (#1926): `LightPlugin`'s
    /// `assign_objects_to_clusters`, run as WebGL2 runs it (no storage
    /// buffers, no GPU clustering), for a camera posed `eye` with the
    /// default lens on a `screen`, the last slice ending at `far` - the
    /// places it counts for each of `lights` alone.
    fn bevy_counts(screen: UVec2, eye: Transform, far: f32, lights: &[Light]) -> Vec<u32> {
        use bevy::camera::visibility::ViewVisibility;
        use bevy::camera::{ComputedCameraValues, RenderTargetInfo};
        use bevy::light::cluster::{Clusters, GlobalClusterSettings};
        let mut app = App::new();
        app.add_plugins((
            bevy::asset::AssetPlugin::default(),
            bevy::light::LightPlugin,
        ));
        // What the plugin's other systems read; the assignment reads none.
        app.init_asset::<Image>();
        app.insert_resource(GlobalClusterSettings {
            supports_storage_buffers: false,
            clustered_decals_are_usable: false,
            gpu_clustering: None,
            max_uniform_buffer_clusterable_objects: 204,
            view_cluster_bindings_max_indices: INDEX_LIST_LEN as usize,
        });
        let mut projection = Projection::Perspective(Default::default());
        projection.update(screen.x as f32, screen.y as f32);
        let global = GlobalTransform::from(eye);
        let camera = app
            .world_mut()
            .spawn((
                Camera {
                    computed: ComputedCameraValues {
                        clip_from_view: projection.get_clip_from_view(),
                        target_info: Some(RenderTargetInfo {
                            physical_size: screen,
                            scale_factor: 1.0,
                        }),
                        ..default()
                    },
                    ..default()
                },
                global,
                projection.compute_frustum(&global),
                Clusters::default(),
            ))
            .id();
        let spawned: Vec<Entity> = lights
            .iter()
            .map(|&(pose, range, outer)| {
                let mut light = app.world_mut().spawn((
                    GlobalTransform::from(pose),
                    pose,
                    ViewVisibility::HIDDEN,
                ));
                match outer {
                    Some(outer) => light.insert(SpotLight {
                        range,
                        outer_angle: outer,
                        inner_angle: outer * 0.5,
                        shadow_maps_enabled: false,
                        ..default()
                    }),
                    None => light.insert(PointLight {
                        range,
                        shadow_maps_enabled: false,
                        ..default()
                    }),
                };
                light.id()
            })
            .collect();
        spawned
            .iter()
            .map(|&shown| {
                for &e in &spawned {
                    let seen = if e == shown {
                        ViewVisibility::VISIBLE
                    } else {
                        ViewVisibility::HIDDEN
                    };
                    app.world_mut().entity_mut(e).insert(seen);
                }
                app.world_mut()
                    .get_mut::<Clusters>(camera)
                    .expect("the camera clusters")
                    .last_frame_farthest_z = Some(far);
                app.world_mut().run_schedule(PostUpdate);
                app.world()
                    .get::<Clusters>(camera)
                    .and_then(|c| c.last_frame_total_cluster_index_count)
                    .expect("Bevy assigned the lights") as u32
            })
            .collect()
    }

    /// What this port counts for each of `lights` alone, read from the
    /// light's pose and the camera's as the drawing reads them.
    fn our_counts(screen: UVec2, eye: Transform, far: f32, lights: &[Light]) -> Vec<u32> {
        let mut projection = Projection::Perspective(Default::default());
        projection.update(screen.x as f32, screen.y as f32);
        let mut view = ClusterView::new(projection.get_clip_from_view(), projection.far(), screen);
        view.set_eye(&GlobalTransform::from(eye));
        let slices = view.slices(far);
        let to_view = Mat4::from(GlobalTransform::from(eye).affine().inverse());
        lights
            .iter()
            .map(|&(pose, range, outer)| {
                let global = GlobalTransform::from(pose);
                let centre = (to_view * global.translation().extend(1.0)).truncate();
                let cone = outer.map(|outer| Cone::new(&to_view, *global.back(), outer));
                view.count(&slices, centre, range, cone)
            })
            .collect()
    }

    /// A small generator for the scenes: the same numbers every run.
    struct Lcg(u64);

    impl Lcg {
        fn unit(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }

        fn between(&mut self, lo: f32, hi: f32) -> f32 {
            lo + (hi - lo) * self.unit()
        }
    }

    /// The spot count is Bevy's, to the place (#1926): for spots ahead of
    /// the eye, beside it, behind it, round it and near the far end, aimed
    /// every way, with cones from a pencil beam to a near hemisphere,
    /// under eyes looking level, down and up on two screens, this port
    /// counts each spot exactly as Bevy 0.20.1's own assignment does - and
    /// a point light too. An undercount would overflow WebGL2's list.
    #[test]
    fn the_spot_count_is_bevys_own_assignment() {
        let mut rng = Lcg(0x5EED_1926);
        let mut spots = 0;
        let mut culled = 0;
        for (screen, pitch, far) in [
            (UVec2::new(1280, 720), -0.17, 30.0),
            (UVec2::new(1920, 1080), -0.4, 18.0),
            (UVec2::new(1280, 720), 0.25, 60.0),
            (UVec2::new(1920, 1080), 0.0, 120.0),
        ] {
            let eye = Transform::from_xyz(1.0, 1.7, -2.0).with_rotation(Quat::from_euler(
                EulerRot::YXZ,
                0.6,
                pitch,
                0.0,
            ));
            let ahead = *eye.forward();
            let mut lights: Vec<Light> = Vec::new();
            for k in 0..48 {
                // Where: ahead, beside, behind, round the eye, far off.
                let at = match k % 6 {
                    0 => eye.translation + ahead * rng.between(2.0, 15.0),
                    1 => {
                        eye.translation
                            + *eye.right() * rng.between(-12.0, 12.0)
                            + ahead * rng.between(-2.0, 4.0)
                    }
                    2 => eye.translation - ahead * rng.between(1.0, 8.0),
                    3 => {
                        eye.translation
                            + Vec3::new(
                                rng.between(-1.5, 1.5),
                                rng.between(-1.0, 1.0),
                                rng.between(-1.5, 1.5),
                            )
                    }
                    4 => eye.translation + ahead * rng.between(far * 0.7, far * 1.1),
                    _ => {
                        eye.translation
                            + Vec3::new(
                                rng.between(-20.0, 20.0),
                                rng.between(-1.5, 6.0),
                                rng.between(-20.0, 20.0),
                            )
                    }
                };
                // Aimed every way, a headlight's raked aim among them.
                let aim = if k % 4 == 0 {
                    Vec3::new(rng.between(-0.3, 0.3), -0.18, 1.0)
                } else {
                    Vec3::new(
                        rng.between(-1.0, 1.0),
                        rng.between(-1.0, 1.0),
                        rng.between(-1.0, 1.0),
                    )
                };
                let aim = Dir3::new(aim).unwrap_or(Dir3::NEG_Y);
                let up = if aim.y.abs() > 0.99 { Dir3::Z } else { Dir3::Y };
                let pose = Transform::from_translation(at).looking_to(aim, up);
                let range = rng.between(1.0, 20.0);
                let outer = rng.between(0.05, 1.5);
                lights.push((pose, range, Some(outer)));
            }
            lights.push((Transform::from_xyz(2.0, 1.0, -8.0), 9.0, None));
            let theirs = bevy_counts(screen, eye, far, &lights);
            let ours = our_counts(screen, eye, far, &lights);
            let spheres = our_counts(
                screen,
                eye,
                far,
                &lights
                    .iter()
                    .map(|&(pose, range, _)| (pose, range, None))
                    .collect::<Vec<_>>(),
            );
            for (i, light) in lights.iter().enumerate() {
                assert_eq!(
                    ours[i], theirs[i],
                    "light {i} {light:?} on {screen} pitched {pitch} to {far} m: \
                     Bevy counts {}, the port {}",
                    theirs[i], ours[i]
                );
                if light.2.is_some() && theirs[i] > 0 {
                    spots += 1;
                    culled += usize::from(theirs[i] < spheres[i]);
                }
            }
        }
        // Enough of them in view, and the cone at work in most.
        assert!(spots >= 120, "{spots} spots in view");
        assert!(
            culled * 2 > spots,
            "only {culled} of {spots} culled by their cone"
        );
    }

    /// A spot counts at most what a point light of its range does, and a
    /// beam aimed away from the eye far less than one aimed across the
    /// view: a car's headlight seen from its chase camera behind it, and
    /// seen from ahead.
    #[test]
    fn a_spot_aimed_away_costs_less_than_its_sphere() {
        let screen = UVec2::new(1920, 1080);
        let look = Vec3::new(0.0, 0.8, 0.0);
        let behind = Transform::from_xyz(0.0, 0.8 + 4.7, -11.0).looking_at(look, Vec3::Y);
        let ahead = Transform::from_xyz(0.0, 0.8 + 4.7, 11.0).looking_at(look, Vec3::Y);
        let lamp =
            Transform::from_xyz(0.4, 0.7, 1.8).looking_to(Vec3::new(0.0, -0.14, 1.0), Vec3::Y);
        let beam = [(lamp, 10.1, Some(40f32.to_radians()))];
        let sphere = [(lamp, 10.1, None)];
        for eye in [behind, ahead] {
            let (spot, point) = (
                our_counts(screen, eye, 25.0, &beam)[0],
                our_counts(screen, eye, 25.0, &sphere)[0],
            );
            assert!(spot > 0 && spot < point, "{spot} against {point}");
        }
        let from_behind = our_counts(screen, behind, 25.0, &beam)[0];
        let point = our_counts(screen, behind, 25.0, &sphere)[0];
        assert!(from_behind * 2 < point, "{from_behind} of {point}");
    }

    /// The first slice is Bevy's for a turned camera (#1926): Bevy takes
    /// it as 5 m times the depth part of the scale the camera's transform
    /// decomposes to, which for this eye is a hair under 1 - so with the
    /// last frame's lights ending at exactly 5 m, Bevy's slices still run
    /// past the first, and a light ahead takes twenty times the places it
    /// would on a camera read as unscaled. The port reads the scale too.
    #[test]
    fn the_first_slice_is_bevys_for_a_turned_camera() {
        let eye = Transform::from_xyz(13.5, 23.9, -43.2).with_rotation(
            Quat::from_xyzw(0.190_428_67, -0.905_849, 0.216_759_16, 0.310_145).normalize(),
        );
        let scale = GlobalTransform::from(eye).compute_transform().scale;
        assert_ne!(scale.z, 1.0, "the premise: {scale}");
        let ahead = *eye.forward();
        let lights: Vec<Light> = vec![
            (
                Transform::from_translation(eye.translation + ahead * 9.0),
                6.0,
                None,
            ),
            (
                Transform::from_translation(eye.translation + ahead * 3.0)
                    .looking_to(ahead + Vec3::new(0.0, -0.3, 0.0), Vec3::Y),
                4.0,
                Some(0.7),
            ),
        ];
        for screen in [UVec2::new(1280, 720), UVec2::new(320, 200)] {
            let theirs = bevy_counts(screen, eye, 5.0, &lights);
            assert_eq!(our_counts(screen, eye, 5.0, &lights), theirs, "{screen}");
            assert!(theirs[0] > 1000, "{theirs:?}");
        }
    }

    /// The cluster spheres a [`Slices`] keeps are Bevy's own (#1926):
    /// made a row at a time, four at a time, from the slice planes, each
    /// is to the bit the sphere `compute_aabb_for_cluster` makes, with its
    /// `powf` depths - for both lenses, near and far ends, every cluster.
    #[test]
    fn the_kept_cluster_spheres_are_compute_aabb_for_clusters() {
        // `compute_aabb_for_cluster` and the sphere round its box, as Bevy
        // 0.20.1 writes them.
        fn bevy_sphere(view: &ClusterView, near: f32, far: f32, ijk: UVec3) -> [f32; 4] {
            let (p_min, p_max) = view.corners[(ijk.y * view.dims.x + ijk.x) as usize];
            let k = ijk.as_vec3().z;
            let dims = view.dims;
            let (lo, hi) = if view.orthographic {
                let (mut p_min, mut p_max) = (p_min, p_max);
                p_min.z = -near + (near - far) * k / dims.z as f32;
                p_max.z = -near + (near - far) * (k + 1.0) / dims.z as f32;
                (p_min.min(p_max), p_min.max(p_max))
            } else {
                let ratio = -far / -near;
                let cluster_near = if k == 0.0 {
                    0.0
                } else {
                    -near * ops::powf(ratio, (k - 1.0) / (dims.z - 1) as f32)
                };
                let cluster_far = -near * ops::powf(ratio, k / (dims.z - 1) as f32);
                let a = line_intersection_to_z_plane(Vec3::ZERO, p_min, cluster_near);
                let b = line_intersection_to_z_plane(Vec3::ZERO, p_min, cluster_far);
                let c = line_intersection_to_z_plane(Vec3::ZERO, p_max, cluster_near);
                let d = line_intersection_to_z_plane(Vec3::ZERO, p_max, cluster_far);
                (a.min(b).min(c.min(d)), a.max(b).max(c.max(d)))
            };
            let (lo, hi) = (Vec3A::from(lo), Vec3A::from(hi));
            let middle = 0.5 * (hi + lo);
            [middle.x, middle.y, middle.z, (0.5 * (hi - lo)).length()]
        }
        let lenses = [
            Projection::Perspective(Default::default()),
            Projection::Perspective(bevy::camera::PerspectiveProjection {
                fov: 1.2,
                ..default()
            }),
            Projection::Orthographic(crate::camera::orthographic_lens(12.0)),
        ];
        for (k, lens) in lenses.iter().enumerate() {
            for screen in [
                UVec2::new(1920, 1080),
                UVec2::new(801, 601),
                UVec2::new(720, 1280),
            ] {
                let view = ClusterView::of(lens, screen);
                for farthest in [0.0, 5.0, 7.3, 26.6, 140.0, 999.0] {
                    let slices = view.slices(farthest);
                    let mut spheres = slices.spheres.borrow_mut();
                    for z in 0..view.dims.z {
                        for y in 0..view.dims.y {
                            view.make_row(&slices, &mut spheres, y, z);
                            for x in 0..view.dims.x {
                                let at = ((z * view.dims.y + y) * view.dims.x + x) as usize;
                                let ours = [
                                    spheres.x[at],
                                    spheres.y[at],
                                    spheres.z[at],
                                    spheres.radius[at],
                                ];
                                let ijk = UVec3::new(x, y, z);
                                let theirs = bevy_sphere(&view, slices.near, slices.far, ijk);
                                assert_eq!(
                                    ours.map(f32::to_bits),
                                    theirs.map(f32::to_bits),
                                    "lens {k} on {screen} to {farthest} m, cluster {ijk}: \
                                     {ours:?} against {theirs:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// What the drawing keeps counts as a fresh view does (#1926, critic
    /// E's probe): one long-lived view and two long-lived slices, laid out
    /// again through changes of lens, screen, far end and camera - its
    /// turn moves the first slice - give every spot the count a view and
    /// slices made afresh give it; and slices one view laid out are never
    /// read as another's, though both views took up a lens as often.
    #[test]
    fn kept_slices_count_as_fresh_ones() {
        let mut rng = Lcg(0xCAC4E);
        let mut kept = ClusterView::default();
        let (mut now, mut then) = (Slices::default(), Slices::default());
        let mut lens = (std::f32::consts::FRAC_PI_4, UVec2::new(1280, 720));
        let mut far = 20.0f32;
        let mut eye = Transform::from_xyz(0.0, 2.0, 0.0);
        let mut wrong = Vec::new();
        for step in 0..1500 {
            match step % 11 {
                0 => lens.0 = rng.between(0.4, 1.4),
                1 => lens.0 = std::f32::consts::FRAC_PI_4,
                2 => {
                    lens.1 = UVec2::new(
                        1 + (rng.unit() * 3000.0) as u32,
                        1 + (rng.unit() * 2000.0) as u32,
                    )
                }
                3 => lens.1 = UVec2::new(1280, 720),
                4 | 5 => far = rng.between(0.0, 80.0),
                6 => far += 0.001,
                // The far end at the first slice, where the camera's turn
                // decides where the slices run.
                7 => far = 5.0,
                _ => {}
            }
            eye.translation += Vec3::new(rng.between(-1.0, 1.0), 0.0, rng.between(-1.0, 1.0));
            eye.rotate_y(rng.between(-0.2, 0.2));
            let projection = Projection::Perspective(bevy::camera::PerspectiveProjection {
                fov: lens.0,
                ..default()
            });
            let global = GlobalTransform::from(eye);
            kept.set_projection(&projection, lens.1);
            kept.set_eye(&global);
            let then_far = if step % 3 == 0 {
                far
            } else {
                far * rng.between(0.5, 2.0)
            };
            kept.reslice(&mut now, far);
            kept.reslice(&mut then, then_far);
            let mut fresh = ClusterView::of(&projection, lens.1);
            fresh.set_eye(&global);
            let (fresh_now, fresh_then) = (fresh.slices(far), fresh.slices(then_far));
            let to_view = Mat4::from(global.affine().inverse());
            for _ in 0..8 {
                let at = eye.translation
                    + Vec3::new(
                        rng.between(-12.0, 12.0),
                        rng.between(-2.0, 6.0),
                        rng.between(-12.0, 12.0),
                    );
                let aim = Vec3::new(
                    rng.between(-1.0, 1.0),
                    rng.between(-1.0, 0.2),
                    rng.between(-1.0, 1.0),
                );
                let up = if aim.normalize_or(Vec3::Y).y.abs() > 0.99 {
                    Vec3::Z
                } else {
                    Vec3::Y
                };
                let pose =
                    GlobalTransform::from(Transform::from_translation(at).looking_to(aim, up));
                let centre = (to_view * pose.translation().extend(1.0)).truncate();
                let range = rng.between(1.0, 20.0);
                let cone = Some(Cone::new(&to_view, *pose.back(), rng.between(0.1, 1.2)));
                for (slices, fresh_slices) in [(&now, &fresh_now), (&then, &fresh_then)] {
                    let (a, b) = (
                        kept.count(slices, centre, range, cone),
                        fresh.count(fresh_slices, centre, range, cone),
                    );
                    if a != b {
                        wrong.push((step, a, b));
                    }
                }
            }
        }
        assert!(
            wrong.is_empty(),
            "{} wrong, first {:?}",
            wrong.len(),
            &wrong[..wrong.len().min(5)]
        );
        // Two views, each a lens taken up as often: B never reads A's
        // slices as its own.
        let a = ClusterView::of(&Projection::default(), UVec2::new(1280, 720));
        let b = ClusterView::of(
            &Projection::Perspective(bevy::camera::PerspectiveProjection {
                fov: 1.2,
                ..default()
            }),
            UVec2::new(1280, 720),
        );
        let mut slices = a.slices(20.0);
        let centre = Vec3::new(0.5, -1.0, -6.0);
        let cone = Some(Cone::new(
            &Mat4::IDENTITY,
            Vec3::new(0.0, 0.3, 1.0).normalize(),
            0.6,
        ));
        let _ = a.count(&slices, centre, 10.0, cone);
        b.reslice(&mut slices, 20.0);
        assert_eq!(
            b.count(&slices, centre, 10.0, cone),
            b.count(&b.slices(20.0), centre, 10.0, cone)
        );
    }
}
