//! Where a four-legged body wears a prop (#1807, owner decision D9).
//!
//! The engine's landmarks follow the person rule on a beast - its Crown is
//! the occiput with the skull's axis for a direction, so `Socket::seat` lands
//! ahead of the face - and its node radii say nothing of a neck much deeper
//! than it is wide. So every seat here is read off what was built: the ear
//! joints in its rig, and the skin itself, cut by a plane or dropped onto along
//! a line.
//!
//! **Only off what a body keeps across the web's worker boundary** (#1813). A
//! body built on a worker comes back drawable and no more: its closed surface
//! (`Parts::body`) and its declared head (`Parts::beast`) stay behind, and a
//! seat read off them leaves a browser's beast wearing nothing. The rig and the
//! drawn meshes both arrive, and every client reads the same two.
//!
//! A seat is a local transform on the joint it rides, at rest, so it follows
//! the pose the way the skin nearest that joint does. Each rides the joint
//! that skin is bound to, which is the one a bone leads into: between the ears,
//! the head joint the ears hang from; a collar, the neck; a pad, the second
//! joint of the chest.

use bevy::prelude::*;
use symbios_avatar::rig::Role;
use symbios_avatar::{Avatar, HeadKind, MeshKind, PolyMesh, Rig, Socket, Zone};

/// Air between a seat and the skin it stands on, in metres: a prop stands
/// on a head, where the person's seat keeps clear of a chest.
const SKIN_AIR: f32 = 0.003;

/// How much roomier than the cut skin a fitted band runs, as a factor: the
/// section's facets lie inside the ellipse fitted to them, the band's own
/// facets do not.
const BAND_EASE: f32 = 1.04;

/// How far from a bone a cut still counts as that bone's section, in measured
/// radii: far enough for a section off its bone, short of a head hanging past
/// the neck or a leg under the chest.
const CUT_REACH: f32 = 1.7;

/// How far along the back the slope a pad lies at is read, in the trunk's
/// measured radii: about the pad's own length.
const PAD_RUN: f32 = 0.7;

/// How far of the way along that run a pad's fit also reads the barrel.
const PAD_FIT: f32 = 0.4;

/// The joint a beast wears `socket` at, or `None` for a head that declares no
/// ears.
pub(super) fn carrier(avatar: &Avatar, socket: Socket) -> Option<usize> {
    match socket {
        Socket::Crown => Ears::of(&avatar.rig).map(|ears| ears.head),
        // The engine's back is the first joint of the chest. The pad lies on
        // the bone after it, whose skin the next joint carries: on the first,
        // a sitting dog's back swings out from under it.
        Socket::Back => avatar.rig.in_zone(Zone::Chest).get(1).copied(),
        _ => socket.joint(&avatar.rig),
    }
}

/// The seat of a prop with an identity offset on a beast, in the rest frame
/// of `joint`, the socket's [`carrier`]. `None` for a socket a beast is not
/// dressed at, or a body whose skin there cannot be measured.
pub(super) fn seat(
    avatar: &Avatar,
    socket: Socket,
    joint: usize,
    fit_beast_mm: u32,
) -> Option<Transform> {
    let cut = (fit_beast_mm != 0).then(|| fit_beast_mm as f32 / 1000.0);
    match socket {
        Socket::Crown => between_the_ears(avatar, joint, cut),
        Socket::Neck => collar(avatar, joint, cut),
        Socket::Back => saddle(avatar, joint, cut),
        _ => None,
    }
}

/// Between the ears: on the skin under the midpoint of the ear bases, facing
/// up. Read off the bases rather than the tips, which a rolled ear folds
/// anywhere. A prop cut for a beast scales with the span between them, and
/// anything else, the circlet, stays the size it was authored.
fn between_the_ears(avatar: &Avatar, joint: usize, across: Option<f32>) -> Option<Transform> {
    let ears = Ears::of(&avatar.rig)?;
    let head = avatar.rig.joints[joint];
    let middle = (ears.left + ears.right) * 0.5;
    let reach = avatar.parts.surface.widest(joint).max(head.radius);
    let skin = skin_under(avatar, middle, reach).unwrap_or(middle.y);
    let span = (ears.left - ears.right).length();
    Some(Transform {
        translation: Vec3::new(middle.x, skin + SKIN_AIR, middle.z) - head.position,
        rotation: Quat::IDENTITY,
        scale: Vec3::splat(across.map_or(1.0, |across| span / across)),
    })
}

/// A beast's ears, as its rig holds them.
struct Ears {
    /// The head joint they hang from.
    head: usize,
    /// One base, in body space.
    left: Vec3,
    /// The other.
    right: Vec3,
}

impl Ears {
    /// Found as the engine's own driver finds them: each base is the parent of
    /// a spring at an ear's tip, on a joint of a beast's head. `None` for a
    /// body with no such pair, whatever else hangs off its head.
    fn of(rig: &Rig) -> Option<Self> {
        if rig.anatomy.head != HeadKind::Beast {
            return None;
        }
        let mut bases = rig
            .joints
            .iter()
            .filter(|joint| joint.role == Role::Spring)
            .filter_map(|tip| tip.parent)
            .filter(|&base| {
                rig.joints[base]
                    .parent
                    .is_some_and(|head| rig.joints[head].zone == Zone::Head)
            });
        let (left, right) = (bases.next()?, bases.next()?);
        Some(Self {
            head: rig.joints[left].parent?,
            left: rig.joints[left].position,
            right: rig.joints[right].position,
        })
    }
}

/// Every face of the drawn skin, with the mesh it indexes.
fn skin_faces(avatar: &Avatar) -> impl Iterator<Item = (&PolyMesh, &[u32])> {
    avatar
        .meshes
        .iter()
        .filter(|drawn| drawn.kind == MeshKind::Skin)
        .flat_map(|drawn| {
            let mesh = &drawn.mesh;
            mesh.faces.iter().map(move |face| (mesh, face.as_slice()))
        })
}

/// Round the neck, at the joint the bone into it ends at: the point of the
/// neck a turn of that joint leaves where it is. Lined up with the bone and
/// centred on the section, which hangs off the bone on a deep neck, and
/// scaled across and through so the bore follows a horse's narrow, deep neck
/// as it does a cat's round one. The band's `+Z` turns to the throat.
fn collar(avatar: &Avatar, neck: usize, across: Option<f32>) -> Option<Transform> {
    let rig = &avatar.rig;
    let (start, end) = rig.bone(neck);
    let axis = (end - start).try_normalize()?;
    let reach = avatar.parts.surface.radius(neck, 1.0) * CUT_REACH;
    let section = Section::cut(avatar, end, axis, reach)?;
    let (half_side, half_deep) = section.half_extents();
    let centre = section.centre();
    let fill = section.fill(centre, half_side, half_deep) * BAND_EASE;
    let bore = across? * 0.5;
    let scale_side = half_side * fill / bore;
    let scale_deep = half_deep * fill / bore;
    Some(Transform {
        translation: end + section.deep * centre - rig.joints[neck].position,
        rotation: Quat::from_rotation_arc(Vec3::Y, axis),
        scale: Vec3::new(scale_side, (scale_side * scale_deep).sqrt(), scale_deep),
    })
}

/// Over the back: the top of the skin between the first two joints of the
/// chest, behind the withers, facing up and pitched with the skin, with no
/// turn - a person's back faces behind them, a beast's the sky. The skin and
/// not the spine, whose first chest bone stands up to 30 degrees off a back
/// that barely slopes. Scaled across and down to the upper half of the trunk's
/// sections along the pad, so its arc lies on the barrel, and along by their
/// mean.
fn saddle(avatar: &Avatar, joint: usize, across: Option<f32>) -> Option<Transform> {
    let rig = &avatar.rig;
    let joints = rig.in_zone(Zone::Chest);
    let (start, end) = rig.bone(joint);
    let at = (start + end) * 0.5;
    let widest = avatar.parts.surface.widest(joint);
    // As far along as the pad runs, short of where the withers rise.
    let withers = joints
        .last()
        .map_or(at.z, |last| rig.joints[*last].position.z);
    let run = (widest * PAD_RUN).min((withers - at.z) * 0.9);
    let along = (run > 0.0)
        .then(|| {
            let skin = |z: f32| skin_under(avatar, Vec3::new(at.x, at.y + widest, z), widest * 1.5);
            Some(Vec3::new(
                0.0,
                skin(at.z + run)? - skin(at.z - run)?,
                2.0 * run,
            ))
        })
        .flatten()
        .and_then(Vec3::try_normalize)
        .unwrap_or(Vec3::Z);
    // Fitted a little either way of its middle, or the barrel's swell toward
    // the loin comes up through the rear of the cloth. To the whole of its
    // length it stands off the back like a box, and more so on one that bends.
    let reach = widest * CUT_REACH;
    let mut section = Section::cut(avatar, at, along, reach)?;
    for end in [run * PAD_FIT, -run * PAD_FIT] {
        if let Some(cut) = Section::cut(avatar, at + along * end, along, reach) {
            section.points.extend(cut.points);
        }
    }
    let section = section.over_the_spine();
    let (half_side, _) = section.half_extents();
    let high = section.points.iter().fold(0.0f32, |top, p| top.max(p.y));
    let fill = section.fill(0.0, half_side, high) * BAND_EASE;
    let radius = across? * 0.5;
    let scale_side = half_side * fill / radius;
    let scale_up = high * fill / radius;
    Some(Transform {
        translation: at + section.deep * (high * fill + SKIN_AIR) - end,
        rotation: Quat::from_rotation_arc(Vec3::Z, along),
        scale: Vec3::new(scale_side, scale_up, (scale_side * scale_up).sqrt()),
    })
}

/// The skin a plane cuts round a bone: every point where an edge of the body
/// crosses it, in the plane's own two directions.
struct Section {
    /// The cut, `x` across the body and `y` along [`Self::deep`].
    points: Vec<Vec2>,
    /// The plane's direction through the body's midline, toward the back of
    /// a neck and the top of a trunk.
    deep: Vec3,
}

impl Section {
    /// Cuts the body's skin through `at`, square to `axis`, keeping what
    /// stands within `reach` of `at`. `None` for a plane that finds no skin.
    fn cut(avatar: &Avatar, at: Vec3, axis: Vec3, reach: f32) -> Option<Self> {
        let deep = axis.cross(Vec3::X).try_normalize()?;
        let mut points = Vec::new();
        for (mesh, face) in skin_faces(avatar) {
            for (index, &from) in face.iter().enumerate() {
                let to = face[(index + 1) % face.len()];
                let (a, b) = (mesh.positions[from as usize], mesh.positions[to as usize]);
                let (above, below) = ((a - at).dot(axis), (b - at).dot(axis));
                if above * below >= 0.0 {
                    continue;
                }
                let crossing = a + (b - a) * (above / (above - below)) - at;
                if crossing.length() <= reach {
                    points.push(Vec2::new(crossing.x, crossing.dot(deep)));
                }
            }
        }
        (!points.is_empty()).then_some(Self { points, deep })
    }

    /// The half of the section above the bone, which is the half a pad lies on.
    fn over_the_spine(mut self) -> Self {
        self.points.retain(|point| point.y >= 0.0);
        self
    }

    /// The section's half width across the body and half depth through it.
    fn half_extents(&self) -> (f32, f32) {
        let side = self
            .points
            .iter()
            .fold(0.0f32, |side, p| side.max(p.x.abs()));
        let (low, high) = self.span();
        (side, (high - low) * 0.5)
    }

    /// The middle of the section along [`Self::deep`].
    fn centre(&self) -> f32 {
        let (low, high) = self.span();
        (low + high) * 0.5
    }

    fn span(&self) -> (f32, f32) {
        self.points
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), p| {
                (low.min(p.y), high.max(p.y))
            })
    }

    /// How far an ellipse of these half extents about `centre` must grow to
    /// hold every point of the cut.
    fn fill(&self, centre: f32, half_side: f32, half_deep: f32) -> f32 {
        self.points.iter().fold(0.0f32, |fill, p| {
            fill.max(Vec2::new(p.x / half_side, (p.y - centre) / half_deep).length())
        })
    }
}

/// The highest skin on the vertical line through `point`, within `reach` of
/// it, in height: where something dropped there lands.
fn skin_under(avatar: &Avatar, point: Vec3, reach: f32) -> Option<f32> {
    let at = Vec2::new(point.x, point.z);
    let mut top: Option<f32> = None;
    for (mesh, face) in skin_faces(avatar) {
        let Some((first, rest)) = face.split_first() else {
            continue;
        };
        let corner = mesh.positions[*first as usize];
        for pair in rest.windows(2) {
            let (b, c) = (
                mesh.positions[pair[0] as usize],
                mesh.positions[pair[1] as usize],
            );
            if let Some(height) = height_on(at, corner, b, c)
                && (height - point.y).abs() <= reach
            {
                top = Some(top.map_or(height, |top| top.max(height)));
            }
        }
    }
    top
}

/// The height of a triangle over the point `at` of the ground plane, if it
/// lies over it.
fn height_on(at: Vec2, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let flat = |p: Vec3| Vec2::new(p.x, p.z);
    let (u, v, w) = (flat(b) - flat(a), flat(c) - flat(a), at - flat(a));
    let area = u.perp_dot(v);
    if area.abs() <= f32::EPSILON {
        return None;
    }
    let (s, t) = (w.perp_dot(v) / area, u.perp_dot(w) / area);
    (s >= 0.0 && t >= 0.0 && s + t <= 1.0).then_some(a.y + (b.y - a.y) * s + (c.y - a.y) * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::avatar::EngineAvatarRecord;
    use symbios_avatar::plan::{Family, QuadrupedParams};
    use symbios_avatar::{Archetype, AvatarConfig};

    /// Rolls a family is judged over: D9's test bar.
    const ROLLS: usize = 60;
    const EAR_SPAN_MM: u32 = 140;
    const BORE_MM: u32 = 200;
    const WIDTH_MM: u32 = 600;

    /// `ROLLS` of the engine's own rolled bodies per family, crosses among
    /// them: a re-roll draws its own family, so they are sorted as they fall.
    fn rolled() -> Vec<(Family, Avatar)> {
        rolled_up_to(ROLLS)
    }

    /// `rolls` of them per family.
    fn rolled_up_to(rolls: usize) -> Vec<(Family, Avatar)> {
        let mut out = Vec::new();
        let mut counts = [0usize; 3];
        let families = [Family::Dog, Family::Cat, Family::Horse];
        for seed in 0..4000i64 {
            if counts.iter().all(|count| *count >= rolls) {
                break;
            }
            let mut record = EngineAvatarRecord {
                archetype: Archetype::Quadruped(QuadrupedParams::default()),
                ..EngineAvatarRecord::default()
            };
            record.reroll(seed);
            let Archetype::Quadruped(params) = &record.archetype else {
                continue;
            };
            let family = params.family;
            let Some(slot) = families.iter().position(|f| *f == family) else {
                continue;
            };
            if counts[slot] >= rolls {
                continue;
            }
            let config = AvatarConfig {
                atlas: 16,
                ..Default::default()
            };
            if let Some(avatar) = Avatar::build_with(&record, &config) {
                counts[slot] += 1;
                out.push((family, avatar));
            }
        }
        assert_eq!(counts, [rolls; 3], "too few rolls came out of each family");
        out
    }

    fn world_of(avatar: &Avatar, joint: usize, seat: &Transform) -> Vec3 {
        avatar.rig.joints[joint].position + seat.translation
    }

    /// **Between the ears, on the skull, on every roll** (#1807): the
    /// midpoint of the bases whatever the ears' shape or size, with no skin
    /// over it and skin close under it. The engine's Crown seat lands ahead of
    /// the face and the first assert is what says so. The circlet, which is
    /// not cut for a beast, keeps its authored size.
    #[test]
    fn a_crown_seat_stands_on_the_skull_between_the_ears() {
        for (family, avatar) in rolled() {
            let rig = &avatar.rig;
            let joint = carrier(&avatar, Socket::Crown).expect("ears");
            let seated = seat(&avatar, Socket::Crown, joint, EAR_SPAN_MM).expect("a seat");
            let at = world_of(&avatar, joint, &seated);
            let ears = &avatar.parts.beast.as_ref().expect("a beast's head").ears;
            let head = rig.joints[ears.head].position;
            let middle = head + (ears.left.base + ears.right.base) * 0.5;
            let span = (ears.left.base - ears.right.base).length();
            assert!(
                (at.x - middle.x).abs() < 0.001 && (at.z - middle.z).abs() < 0.001,
                "{family:?}: the seat {at} is not over the ears' midpoint {middle}"
            );
            let hit = rig.nearest_bone(at);
            assert!(
                rig.joints[hit.joint].zone == Zone::Head,
                "{family:?}: the seat is nearest {:?}",
                rig.joints[hit.joint].zone
            );
            let near: Vec<Vec3> = avatar
                .parts
                .body
                .positions
                .iter()
                .copied()
                .filter(|p| {
                    Vec2::new(p.x - at.x, p.z - at.z).length() < span * 0.2
                        && (p.y - at.y).abs() < span * 1.5
                })
                .collect();
            let above = near.iter().map(|p| p.y - at.y).fold(f32::MIN, f32::max);
            let below = near.iter().map(|p| at.y - p.y).fold(f32::MAX, f32::min);
            assert!(
                above < span * 0.12,
                "{family:?}: skin stands {above:.3} m over a seat on a {span:.3} m span"
            );
            assert!(
                below < span * 0.35,
                "{family:?}: the seat floats {below:.3} m over a {span:.3} m span's skin"
            );
            assert!(
                (seated.scale.x - span / 0.14).abs() < 1e-4 && seated.rotation == Quat::IDENTITY,
                "{family:?}: a bow's scale {} on a {span:.3} m span",
                seated.scale
            );
            let circlet = seat(&avatar, Socket::Crown, joint, 0).expect("a seat");
            assert_eq!(circlet.scale, Vec3::ONE, "{family:?}: the circlet resized");
        }
    }

    /// **A collar rings the neck, on every roll** (#1807): lined up with the
    /// bone, its `+Z` at the throat, and its bore holding every point of the
    /// skin's cut while hugging at least one - from a cat's round neck to a
    /// horse's deep one, which a uniform ring leaves hanging off the sides.
    #[test]
    fn a_collar_rings_the_neck_and_holds_its_skin() {
        let bore = BORE_MM as f32 / 2000.0;
        for (family, avatar) in rolled() {
            let rig = &avatar.rig;
            let neck = carrier(&avatar, Socket::Neck).expect("a neck");
            assert_eq!(rig.joints[neck].zone, Zone::Neck);
            let seated = seat(&avatar, Socket::Neck, neck, BORE_MM).expect("a seat");
            let at = world_of(&avatar, neck, &seated);
            let (start, end) = rig.bone(neck);
            let axis = (end - start).normalize();
            let ring_axis = seated.rotation * Vec3::Y;
            assert!(
                ring_axis.dot(axis) > 0.9999,
                "{family:?}: the ring lies at {ring_axis} on a bone along {axis}"
            );
            let reach = avatar.parts.surface.radius(neck, 1.0) * CUT_REACH;
            let section = Section::cut(&avatar, end, axis, reach).expect("a neck's skin");
            let throat = seated.rotation * Vec3::Z;
            assert!(
                throat.dot(section.deep) < -0.99,
                "{family:?}: the buckle faces {throat}, the crest is {}",
                section.deep
            );
            // The ring's own bore, in the plane: every point of the cut lies
            // in it, and the tightest within a tenth of it.
            let across = bore * seated.scale.x;
            let through = bore * seated.scale.z;
            let centre = (at - end).dot(section.deep);
            let filled = section.points.iter().fold(0.0f32, |most, p| {
                most.max(Vec2::new(p.x / across, (p.y - centre) / through).length())
            });
            assert!(
                filled <= 1.0 && filled > 0.9,
                "{family:?}: the skin fills {filled:.3} of the bore"
            );
            // And by the engine's own measure of the neck, independent of the
            // cut: a collar the size of the neck and not of something else.
            let measured = avatar.parts.surface.radius(neck, 1.0);
            assert!(
                through > measured * 0.5 && through < measured * 1.6,
                "{family:?}: a {through:.3} m bore on a {measured:.3} m neck"
            );
            assert!(
                across > measured * 0.35 && across < measured * 1.6,
                "{family:?}: a {across:.3} m bore across a {measured:.3} m neck"
            );
            let hit = rig.nearest_bone(at);
            // On the joint, the bones in and out of it tie, and the bone out
            // of it is the rest of the neck, though it leads into the head.
            assert!(
                hit.joint == neck || rig.joints[hit.joint].parent == Some(neck),
                "{family:?}: the collar stands nearest {:?}",
                rig.joints[hit.joint].zone
            );
        }
    }

    /// **A pad lies on the back, on every roll** (#1807): facing up and along
    /// the body, with no half turn, on the top of the skin behind the withers,
    /// its width the width of the back it lies on.
    #[test]
    fn a_saddle_pad_lies_on_the_back_facing_up() {
        let radius = WIDTH_MM as f32 / 2000.0;
        for (family, avatar) in rolled() {
            let rig = &avatar.rig;
            let chest = carrier(&avatar, Socket::Back).expect("a chest");
            let seated = seat(&avatar, Socket::Back, chest, WIDTH_MM).expect("a seat");
            let at = world_of(&avatar, chest, &seated);
            let up = seated.rotation * Vec3::Y;
            let along = seated.rotation * Vec3::Z;
            assert!(up.y > 0.95, "{family:?}: the pad faces {up}");
            assert!(along.z > 0.95, "{family:?}: the pad runs along {along}");

            let hit = rig.nearest_bone(at);
            assert!(
                matches!(rig.joints[hit.joint].zone, Zone::Chest | Zone::Abdomen),
                "{family:?}: the pad stands nearest {:?}",
                rig.joints[hit.joint].zone
            );
            // The skin's top at the seat, from the plane through it: the seat
            // stands over it, and by no more than its fit's slack.
            let reach = avatar.parts.surface.widest(chest) * 2.0;
            let (start, end) = rig.bone(chest);
            assert_eq!(rig.joints[chest].zone, Zone::Chest);
            let middle = (start + end) * 0.5;
            let spine = middle + along * (at - middle).dot(along);
            let section = Section::cut(&avatar, spine, along, reach)
                .expect("a trunk's skin")
                .over_the_spine();
            let top = section.points.iter().fold(0.0f32, |top, p| top.max(p.y));
            let standing = (at - spine).dot(section.deep);
            assert!(
                standing >= top * 0.97 && standing < top * 1.45,
                "{family:?}: a seat {standing:.3} m over the spine on skin {top:.3} m over it"
            );
            let (side, _) = section.half_extents();
            let wide = radius * seated.scale.x;
            assert!(
                wide > side && wide < side * 2.0,
                "{family:?}: a pad {wide:.3} m wide on a back {side:.3} m wide"
            );
        }
    }

    /// **The drawn skin seats a prop as the closed surface does** (#1813),
    /// which only a body built here still has. The same to the bit when this
    /// was written, ears and jaw merged into the draw and all: they do not
    /// reach the cuts. Should a roll ever bring one there, this says so.
    #[test]
    fn the_drawn_skin_seats_as_the_closed_surface_does() {
        const SOCKETS: [(Socket, u32); 3] = [
            (Socket::Crown, EAR_SPAN_MM),
            (Socket::Neck, BORE_MM),
            (Socket::Back, WIDTH_MM),
        ];
        for (family, mut avatar) in rolled_up_to(20) {
            let seated = |avatar: &Avatar, socket: Socket, mm: u32| {
                let joint = carrier(avatar, socket).expect("a joint");
                seat(avatar, socket, joint, mm).expect("a seat")
            };
            let drawn = SOCKETS.map(|(socket, mm)| seated(&avatar, socket, mm));
            avatar.meshes = vec![symbios_avatar::AvatarMesh {
                kind: MeshKind::Skin,
                mesh: avatar.parts.body.clone(),
            }];
            for ((socket, mm), drawn) in SOCKETS.into_iter().zip(drawn) {
                let whole = seated(&avatar, socket, mm);
                let moved = drawn.translation.distance(whole.translation);
                let grew = (drawn.scale / whole.scale - Vec3::ONE).abs().max_element();
                let turned = drawn.rotation.angle_between(whole.rotation).to_degrees();
                assert!(
                    moved < 0.002 && grew < 0.01 && turned < 0.5,
                    "{family:?} at {socket:?}: moved {moved:.4} m, grew {grew:.3}, turned \
                     {turned:.2} deg"
                );
            }
        }
    }

    /// The test over the seat rule itself, on the shape a plane cuts: a cut
    /// through a quad finds its edges, and a line finds its height. These two
    /// are every measure in the file.
    #[test]
    fn the_measures_read_a_triangle_as_it_lies() {
        let height = height_on(
            Vec2::new(0.25, 0.25),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 2.0, 0.0),
            Vec3::new(0.0, 3.0, 1.0),
        );
        assert_eq!(height, Some(1.0 + 0.25 + 0.5));
        let outside = height_on(Vec2::new(2.0, 2.0), Vec3::ZERO, Vec3::X, Vec3::Z);
        assert_eq!(outside, None);
    }
}
