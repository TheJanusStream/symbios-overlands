//! Where a winged body wears a prop (#1879, the owner's choice): a collar at
//! its neck, anything at its crown, and a band on either leg - an animal's
//! cut, the beast's `fitBeastMm` reused, and the crown's circlet at its
//! authored size. Not its back, where the folded wings lie, nor its hands, a
//! wing's wrists ([`crate::pds::avatar::WINGED_SOCKETS`]).
//!
//! **The beast's seats do not fit it** ([`super::beast_seat`]). A bird has no
//! spring ears for the crown seat to read, so the crown stands on the head
//! itself, a bat's and a glider's too. And a bird is dressed in feathers drawn
//! as cards (`MeshKind::Hair`) standing off its skin: a collar fitted to the
//! skin, as a beast's is, would ring the neck under them. So every seat here
//! is read off what is drawn, skin and plumage together - a bat's ruff and a
//! glider's fur included, which the cuts reach the same way.
//!
//! **Only off what crosses the web's worker** (#1813): the rig, its measured
//! surface and the drawn meshes. A body built on a worker keeps neither its
//! closed surface (`Parts::body`), its declared head nor its wings' build
//! (`Parts::wings`), and a seat read off them would leave a browser's bird
//! wearing nothing.
//!
//! A seat is a local transform on the joint it rides, at rest, as a beast's
//! is: the crown the skull, the collar the neck joint whose turn carries the
//! neck, a band the joint above the leg bone it rings.

use bevy::prelude::*;
use symbios_avatar::rig::Role;
use symbios_avatar::{Avatar, MeshKind, PolyMesh, Rig, Socket, Zone};

use super::beast_seat::{BAND_EASE, CUT_REACH, SKIN_AIR, height_on};

/// What a seat reads as the body's outside: the skin, and the plumage and fur
/// drawn over it.
const DRAWN: [MeshKind; 2] = [MeshKind::Skin, MeshKind::Hair];

/// Where along the neck a collar may sit, as shares of the bone from the neck
/// joint into the head: the narrowest of them takes it, clear of the breast
/// below and a facial disc above.
const NECK_STATIONS: [f32; 5] = [0.2, 0.35, 0.5, 0.65, 0.8];

/// Where along the leg bone a band sits, as a share of it from the joint
/// above: a bird's ring at the middle of its tarsus.
const BAND_AT: f32 = 0.5;

/// Directions round a leg a band reads the skin's nearest point in
/// ([`Ring::leg`]).
const SECTORS: usize = 24;

/// How far out from a leg's bone its skin may stand, in the leg skin's own
/// reach from it: a bird's scaled and feathered tarsus, short of the flank.
const LEG_COAT: f32 = 1.6;

/// How far out from a leg's bone the plumage and fur a band holds may stand,
/// in the leg skin's own reach from it: no further than the skin, so feather
/// cards standing off the tarsus pass through the band, as a ringed bird's do.
/// At [`LEG_COAT`] the cards held stood a band on an owl crossed toward a bat
/// at over twice its leg's node radius, floating clear of the leg (#1879).
const PLUMAGE_COAT: f32 = 1.0;

/// How much of the head's drawn width an ornament cut for an animal spans:
/// about the share a dog's ear bases span of its head, which is what such an
/// ornament is cut for.
const CROWN_SPAN: f32 = 0.6;

/// The joint a winged body wears `socket` at, or `None` for one it is not
/// dressed at.
pub(super) fn carrier(avatar: &Avatar, socket: Socket) -> Option<usize> {
    let rig = &avatar.rig;
    match socket {
        Socket::Crown => skull(rig),
        Socket::Neck => socket.joint(rig),
        // The engine's foot is the joint the leg ends at; the band rings the
        // bone into it, which the joint above carries.
        Socket::LeftFoot | Socket::RightFoot => rig.joints[socket.joint(rig)?].parent,
        _ => None,
    }
}

/// The seat of a prop with an identity offset on a winged body, in the rest
/// frame of `joint`, the socket's [`carrier`]. `None` for a socket a winged
/// body is not dressed at, or a body whose outside there cannot be measured.
pub(super) fn seat(
    avatar: &Avatar,
    socket: Socket,
    joint: usize,
    fit_beast_mm: u32,
) -> Option<Transform> {
    let cut = (fit_beast_mm != 0).then(|| fit_beast_mm as f32 / 1000.0);
    match socket {
        Socket::Crown => on_the_head(avatar, joint, cut),
        Socket::Neck => collar(avatar, joint, cut),
        Socket::LeftFoot | Socket::RightFoot => leg_band(avatar, socket.joint(&avatar.rig)?, cut),
        _ => None,
    }
}

/// The skull: the head's widest joint, whose turn carries the crown, the
/// face and - on a bat or a glider - the ears.
fn skull(rig: &Rig) -> Option<usize> {
    rig.in_zone(Zone::Head)
        .iter()
        .copied()
        .filter(|&joint| rig.joints[joint].role == Role::Deform)
        .max_by(|&a, &b| rig.joints[a].radius.total_cmp(&rig.joints[b].radius))
}

/// On the head: the top of what is drawn over the skull joint, a bird's
/// crown feathers included, facing up. A prop cut for an animal spans
/// [`CROWN_SPAN`] of the head's drawn width there; anything else, the
/// circlet, stays the size it was authored.
fn on_the_head(avatar: &Avatar, joint: usize, across: Option<f32>) -> Option<Transform> {
    let head = avatar.rig.joints[joint];
    let reach = avatar.parts.surface.widest(joint).max(head.radius) * 2.0;
    let top = drawn_over(avatar, head.position, reach)?;
    let scale = match across {
        Some(across) => {
            let ring = Ring::cut(avatar, head.position, Vec3::Z, Vec3::Y, reach)?;
            let wide = ring
                .points
                .iter()
                .filter(|point| point.y >= 0.0)
                .fold(0.0f32, |wide, point| wide.max(point.x.abs()));
            CROWN_SPAN * 2.0 * wide / across
        }
        None => 1.0,
    };
    Some(Transform {
        translation: Vec3::new(0.0, top + SKIN_AIR - head.position.y, 0.0),
        rotation: Quat::IDENTITY,
        scale: Vec3::splat(scale),
    })
}

/// Round the neck, over its plumage: across the narrowest of
/// [`NECK_STATIONS`] along the bone from the neck joint into the head,
/// square to it, centred on what is drawn there and scaled across and
/// through to hold all of it. The band's `+Z` turns to the throat.
fn collar(avatar: &Avatar, neck: usize, across: Option<f32>) -> Option<Transform> {
    let rig = &avatar.rig;
    let head = Socket::Crown.joint(rig)?;
    let (start, end) = (rig.joints[neck].position, rig.joints[head].position);
    let axis = (end - start).try_normalize()?;
    let throat = Vec3::X.cross(axis).try_normalize()?;
    let (at, ring) = NECK_STATIONS
        .iter()
        .filter_map(|&along| {
            let at = start.lerp(end, along);
            let reach = avatar.parts.surface.radius(head, along) * CUT_REACH;
            Some((at, Ring::cut(avatar, at, axis, throat, reach)?))
        })
        .min_by(|(_, a), (_, b)| a.area().total_cmp(&b.area()))?;
    ring.fitted(at, axis, across?, rig.joints[neck].position)
}

/// Round the leg: across the bone into the foot joint `foot` at
/// [`BAND_AT`], square to it, centred on the leg there and scaled across and
/// through to hold it - a bird's tarsus and the feathers lying on it, a bat's
/// or a glider's shin ([`Ring::leg`]: not a membrane's fold or a flank's
/// feathers that the plane also cuts). The band's `+Z` turns out of the body, to the side the
/// leg is on.
fn leg_band(avatar: &Avatar, foot: usize, across: Option<f32>) -> Option<Transform> {
    let rig = &avatar.rig;
    let knee = rig.joints[foot].parent?;
    let (start, end) = (rig.joints[knee].position, rig.joints[foot].position);
    // Up the leg, so the band is never turned over end to end.
    let axis = (start - end).try_normalize()?;
    let outward = Vec3::X * end.x.signum();
    let face = (outward - axis * outward.dot(axis)).try_normalize()?;
    let at = start.lerp(end, BAND_AT);
    let reach = avatar.parts.surface.radius(foot, BAND_AT) * CUT_REACH;
    Ring::leg(avatar, at, axis, face, reach)?.fitted(at, axis, across?, start)
}

/// What a plane cuts of the drawn body round a bone, in the plane's own two
/// directions: `x` across ([`Self::across`]) and `y` along the way the band's
/// face turns ([`Self::face`]).
struct Ring {
    points: Vec<Vec2>,
    across: Vec3,
    face: Vec3,
}

impl Ring {
    /// Cuts what is drawn through `at`, square to `axis`, keeping what stands
    /// within `reach` of `at`; `face` is the way the band's `+Z` turns, square
    /// to `axis`. `None` for a plane that finds nothing.
    fn cut(avatar: &Avatar, at: Vec3, axis: Vec3, face: Vec3, reach: f32) -> Option<Self> {
        Self::cut_of(avatar, &DRAWN, at, axis, face, reach)
    }

    /// [`Self::cut`] through the meshes of `kinds` alone.
    fn cut_of(
        avatar: &Avatar,
        kinds: &[MeshKind],
        at: Vec3,
        axis: Vec3,
        face: Vec3,
        reach: f32,
    ) -> Option<Self> {
        let across = axis.cross(face).try_normalize()?;
        let mut points = Vec::new();
        for (mesh, face_indices) in faces_of(avatar, kinds) {
            for (index, &from) in face_indices.iter().enumerate() {
                let to = face_indices[(index + 1) % face_indices.len()];
                let (a, b) = (mesh.positions[from as usize], mesh.positions[to as usize]);
                let (above, below) = ((a - at).dot(axis), (b - at).dot(axis));
                if above * below >= 0.0 {
                    continue;
                }
                let crossing = a + (b - a) * (above / (above - below)) - at;
                if crossing.length() <= reach {
                    points.push(Vec2::new(crossing.dot(across), crossing.dot(face)));
                }
            }
        }
        (!points.is_empty()).then_some(Self {
            points,
            across,
            face,
        })
    }

    /// The leg a plane through `at` cuts, as [`Self::cut`] reads it: the leg's
    /// reach from its bone found from the skin's nearest point in each of
    /// [`SECTORS`] directions round it, which is never a membrane folded
    /// beside it or the flank above; then every point of skin within
    /// [`LEG_COAT`] of that reach, and of plumage and fur within
    /// [`PLUMAGE_COAT`] - a bird's tarsus, the cards lying on it - and not a
    /// membrane's fold, a toe or the breast feathers hanging past it. `None`
    /// for a plane that finds no leg.
    fn leg(avatar: &Avatar, at: Vec3, axis: Vec3, face: Vec3, reach: f32) -> Option<Self> {
        use std::f32::consts::TAU;
        let skin = Self::cut_of(avatar, &[MeshKind::Skin], at, axis, face, reach)?;
        let mut nearest: [Option<Vec2>; SECTORS] = [None; SECTORS];
        for &point in &skin.points {
            let sector = ((point.y.atan2(point.x) / TAU + 0.5) * SECTORS as f32) as usize % SECTORS;
            if nearest[sector].is_none_or(|was| was.length() > point.length()) {
                nearest[sector] = Some(point);
            }
        }
        // The leg's reach from its bone: the middle of those nearest points,
        // so a toe or the belly the plane also cuts in a direction or two
        // cannot stand in for it.
        let mut reaches: Vec<f32> = nearest
            .iter()
            .flatten()
            .map(|point| point.length())
            .collect();
        if reaches.is_empty() {
            return None;
        }
        reaches.sort_by(f32::total_cmp);
        let round = reaches[reaches.len() / 2];
        let coat = Self::cut_of(avatar, &[MeshKind::Hair], at, axis, face, reach);
        let points = skin
            .points
            .into_iter()
            .filter(|point| point.length() <= round * LEG_COAT)
            .chain(
                coat.into_iter()
                    .flat_map(|coat| coat.points)
                    .filter(|point| point.length() <= round * PLUMAGE_COAT),
            )
            .collect();
        let leg = Self {
            points,
            across: skin.across,
            face: skin.face,
        };
        Some(leg)
    }

    /// The middle of the cut, and its half extents about it.
    fn extents(&self) -> (Vec2, Vec2) {
        let (low, high) = self
            .points
            .iter()
            .fold((Vec2::MAX, Vec2::MIN), |(low, high), point| {
                (low.min(*point), high.max(*point))
            });
        ((low + high) * 0.5, (high - low) * 0.5)
    }

    /// How far an ellipse of half extents `half` about `centre` must grow to
    /// hold every point of the cut.
    fn fill(&self, centre: Vec2, half: Vec2) -> f32 {
        self.points.iter().fold(0.0f32, |fill, point| {
            fill.max(((*point - centre) / half).length())
        })
    }

    /// The area of the band that holds the cut.
    fn area(&self) -> f32 {
        let (centre, half) = self.extents();
        let fill = self.fill(centre, half);
        half.x * half.y * fill * fill
    }

    /// A band of authored bore `across` holding this cut, through `at` and
    /// lined up with `axis`, in the rest frame of the joint at `carrier`.
    fn fitted(&self, at: Vec3, axis: Vec3, across: f32, carrier: Vec3) -> Option<Transform> {
        let (centre, half) = self.extents();
        if half.min_element() <= 0.0 {
            return None;
        }
        let fill = self.fill(centre, half) * BAND_EASE;
        let bore = across * 0.5;
        let (side, through) = (half.x * fill / bore, half.y * fill / bore);
        Some(Transform {
            translation: at + self.across * centre.x + self.face * centre.y - carrier,
            rotation: Quat::from_mat3(&Mat3::from_cols(self.across, axis, self.face)),
            scale: Vec3::new(side, (side * through).sqrt(), through),
        })
    }
}

/// Every face of the drawn meshes of `kinds`, with the mesh it indexes.
fn faces_of<'a>(
    avatar: &'a Avatar,
    kinds: &'a [MeshKind],
) -> impl Iterator<Item = (&'a PolyMesh, &'a [u32])> {
    avatar
        .meshes
        .iter()
        .filter(move |drawn| kinds.contains(&drawn.kind))
        .flat_map(|drawn| {
            let mesh = &drawn.mesh;
            mesh.faces.iter().map(move |face| (mesh, face.as_slice()))
        })
}

/// The highest of what is drawn on the vertical line through `point`, within
/// `reach` of it, in height: where something dropped there lands.
fn drawn_over(avatar: &Avatar, point: Vec3, reach: f32) -> Option<f32> {
    let at = Vec2::new(point.x, point.z);
    let mut top: Option<f32> = None;
    for (mesh, face) in faces_of(avatar, &DRAWN) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::avatar::EngineAvatarRecord;
    use symbios_avatar::{Archetype, AvatarConfig, WingedFamily, WingedParams};

    const COLLAR_MM: u32 = 200;
    const BOW_MM: u32 = 140;
    const BAND_MM: u32 = 35;

    /// The widest a band's bore may read against its leg's own node radius
    /// and stay snug on it, a bird's tarsus and the feathers lying on it. A
    /// band holding the cards standing off an owl crossed toward a bat read
    /// 2.2 and floated clear of the leg in the render.
    const SNUG: f32 = 2.0;

    /// An owl, a bat and a glider as the editor makes them, and rolled bodies
    /// of each family and crosses as a re-roll draws them, each by the family
    /// that leads it.
    fn bodies() -> Vec<(String, Avatar)> {
        let mut records: Vec<(String, EngineAvatarRecord)> =
            [WingedFamily::Bird, WingedFamily::Bat, WingedFamily::Glider]
                .into_iter()
                .map(|family| {
                    (
                        format!("the default {family:?}"),
                        crate::pds::avatar::locomotion::default_winged(family),
                    )
                })
                .collect();
        // A short-legged owl, and owl-led crosses toward a bat and a glider,
        // whose legs are the thinnest a band rings.
        let owl = |name: &str, shape: &dyn Fn(&mut WingedParams)| {
            let mut record = crate::pds::avatar::locomotion::default_winged(WingedFamily::Bird);
            if let Archetype::Winged(params) = &mut record.archetype {
                shape(params);
            }
            record.sanitize();
            (name.to_string(), record)
        };
        records.push(owl("a short-legged owl", &|params| {
            params.leg_length = -1.0
        }));
        for (other, blend) in [(WingedFamily::Bat, 0.3), (WingedFamily::Glider, 0.49)] {
            records.push(owl(&format!("an owl {blend} a {other:?}"), &|params| {
                params.set_kind(WingedFamily::Bird, Some(other), blend);
                params.blend = blend;
            }));
        }
        for seed in 0..9i64 {
            let mut record = EngineAvatarRecord::rolled(
                "Seat",
                Archetype::Winged(WingedParams::default()),
                seed,
            );
            record.sanitize();
            records.push((format!("rolled seed {seed}"), record));
        }
        records
            .into_iter()
            .map(|(name, record)| {
                let config = AvatarConfig {
                    atlas: 16,
                    ..Default::default()
                };
                let avatar = Avatar::build_with(&record, &config).expect("a winged body builds");
                let family = crate::pds::avatar::RiggedKind::of_rig(&avatar.rig);
                (format!("{name} ({family:?})"), avatar)
            })
            .collect()
    }

    fn world_of(avatar: &Avatar, joint: usize, seat: &Transform) -> Vec3 {
        avatar.rig.joints[joint].position + seat.translation
    }

    /// What a seated band holds: the furthest any point of `ring` lies out
    /// in the band's own bore, which is inside it at 1 or under.
    fn filled(ring: &Ring, seat: &Transform, at: Vec3, carrier: Vec3, bore: f32) -> f32 {
        let centre = seat.translation + carrier - at;
        let centre = Vec2::new(centre.dot(ring.across), centre.dot(ring.face));
        let half = Vec2::new(bore * seat.scale.x, bore * seat.scale.z);
        ring.fill(centre, half)
    }

    /// **A collar rings the neck over the plumage, on every body** (#1879):
    /// lined up with the bone from the neck into the head, its `+Z` at the
    /// throat, its bore holding every point of skin and feather the plane
    /// cuts while hugging one. A bird's feathers stand outside its skin
    /// there, and a ring fitted to the skin alone - the beast's way - leaves
    /// them outside it: the premise, measured.
    #[test]
    fn a_collar_rings_the_neck_over_its_plumage() {
        let bore = COLLAR_MM as f32 / 2000.0;
        let mut feathered = 0;
        for (name, avatar) in bodies() {
            let rig = &avatar.rig;
            let neck = carrier(&avatar, Socket::Neck).expect("a neck");
            assert_eq!(rig.joints[neck].zone, Zone::Neck, "{name}");
            let seated = seat(&avatar, Socket::Neck, neck, COLLAR_MM).expect("a seat");
            let at = world_of(&avatar, neck, &seated);
            let head = Socket::Crown.joint(rig).expect("a head");
            let axis = (rig.joints[head].position - rig.joints[neck].position).normalize();
            let ring_axis = seated.rotation * Vec3::Y;
            assert!(
                ring_axis.dot(axis) > 0.9999,
                "{name}: the ring lies at {ring_axis}"
            );
            let throat = seated.rotation * Vec3::Z;
            assert!(
                throat.dot(Vec3::X.cross(axis).normalize()) > 0.9999,
                "{name}: the buckle faces {throat}"
            );
            // The station the seat chose: the plane through it, square to the
            // bone, cuts what the band holds.
            let along = (at - rig.joints[neck].position).dot(axis);
            let station = rig.joints[neck].position + axis * along;
            let reach = avatar.parts.surface.widest(head) * CUT_REACH * 1.5;
            let face = Vec3::X.cross(axis).normalize();
            let drawn = [MeshKind::Skin, MeshKind::Hair];
            let ring = Ring::cut_of(&avatar, &drawn, station, axis, face, reach).expect("a neck");
            let fill = filled(&ring, &seated, station, rig.joints[neck].position, bore);
            assert!(
                fill <= 1.0 && fill > 0.9,
                "{name}: what is drawn fills {fill:.3} of the bore"
            );
            // Hair drawn outside the skin there: a ring fitted to the skin
            // alone leaves it out.
            let skin_only = {
                let skin = [MeshKind::Skin];
                let neck_ring =
                    Ring::cut_of(&avatar, &skin, station, axis, face, reach).expect("skin");
                let (centre, half) = neck_ring.extents();
                let tight = neck_ring.fill(centre, half);
                ring.fill(centre, half * tight)
            };
            println!("{name}: the collar holds {fill:.3}; a skin-fitted ring {skin_only:.3}");
            if skin_only > 1.05 {
                feathered += 1;
            }
            let hit = rig.nearest_bone(at);
            assert!(
                matches!(rig.joints[hit.joint].zone, Zone::Neck | Zone::Head),
                "{name}: the collar stands nearest {:?}",
                rig.joints[hit.joint].zone
            );
        }
        assert!(
            feathered > 0,
            "the premise: no plumage stood outside a skin-fitted ring"
        );
    }

    /// **The crown stands on the head, on every body** (#1879): over the
    /// skull joint, on top of what is drawn there with nothing drawn over it,
    /// riding a joint of the head. A bird has no spring ears, so the beast's
    /// seat finds no crown on it at all - the premise. An ornament cut for an
    /// animal spans its share of the head; the circlet keeps its size.
    #[test]
    fn a_crown_stands_on_the_head() {
        for (name, avatar) in bodies() {
            let rig = &avatar.rig;
            let skull = carrier(&avatar, Socket::Crown).expect("a skull");
            assert_eq!(rig.joints[skull].zone, Zone::Head, "{name}");
            let seated = seat(&avatar, Socket::Crown, skull, BOW_MM).expect("a seat");
            let at = world_of(&avatar, skull, &seated);
            let joint = rig.joints[skull].position;
            assert!(
                (at.x - joint.x).abs() < 1e-5 && (at.z - joint.z).abs() < 1e-5,
                "{name}: the crown {at} is not over the skull {joint}"
            );
            // Everything drawn on the line through the seat, however high or
            // low: the seat stands on the highest, just clear of it.
            let over = drawn_over(&avatar, at, f32::MAX).expect("a head under the crown");
            assert!(
                at.y >= over && at.y - over < 0.01,
                "{name}: the crown stands {:.4} m over the highest thing drawn under it",
                at.y - over
            );
            let width = seated.scale.x * BOW_MM as f32 / 1000.0 / CROWN_SPAN;
            assert!(
                width > rig.joints[skull].radius && width < rig.joints[skull].radius * 5.0,
                "{name}: a {width:.3} m head on a {:.3} m skull",
                rig.joints[skull].radius
            );
            let circlet = seat(&avatar, Socket::Crown, skull, 0).expect("a seat");
            assert_eq!(circlet.scale, Vec3::ONE, "{name}: the circlet resized");
            // The owl has no spring ears, so the beast's seat finds no crown
            // on it at all (a cross may grow a bat's or a glider's).
            if name.starts_with("the default Bird") {
                let joint = super::super::beast_seat::carrier(&avatar, Socket::Crown);
                assert_eq!(joint, None, "{name}: the premise, an owl with ears");
            }
        }
    }

    /// **A band rings the leg, on every body** (#1879): on the bone into the
    /// engine's foot joint - a bird's tarsus, a bat's or a glider's shin -
    /// riding the joint above it, lined up with it, its plate turned out of
    /// the body, and its bore holding the leg's skin and the feathers lying
    /// on it while staying the size of the leg: a membrane anchored near the
    /// ankle does not swell it, nor do the feather cards standing off an owl's
    /// tarsus ([`SNUG`]) - an owl crossed toward a bat, and a short-legged
    /// owl, among the bodies.
    #[test]
    fn a_band_rings_the_leg() {
        let bore = BAND_MM as f32 / 2000.0;
        for (name, avatar) in bodies() {
            let rig = &avatar.rig;
            for socket in [Socket::LeftFoot, Socket::RightFoot] {
                let foot = socket.joint(rig).expect("a foot");
                let knee = carrier(&avatar, socket).expect("a leg");
                assert_eq!(rig.joints[foot].parent, Some(knee));
                let seated = seat(&avatar, socket, knee, BAND_MM).expect("a seat");
                let at = world_of(&avatar, knee, &seated);
                let (start, end) = (rig.joints[knee].position, rig.joints[foot].position);
                let axis = (start - end).normalize();
                assert!(
                    (seated.rotation * Vec3::Y).dot(axis) > 0.9999,
                    "{name} at {socket:?}: the band lies off the leg"
                );
                let out = seated.rotation * Vec3::Z;
                assert!(
                    out.x * end.x > 0.0,
                    "{name} at {socket:?}: the plate faces {out}, into the body"
                );
                let station = start.lerp(end, BAND_AT);
                let face = out;
                let reach = avatar.parts.surface.radius(foot, BAND_AT) * CUT_REACH;
                let ring = Ring::leg(&avatar, station, axis, face, reach).expect("a leg");
                let fill = filled(&ring, &seated, station, start, bore);
                assert!(
                    fill <= 1.0 && fill > 0.9,
                    "{name} at {socket:?}: the leg fills {fill:.3} of the bore"
                );
                // Every point of the leg's skin the plane cuts, measured
                // without the band's own rule: the skin within the bone's own
                // node radius there, a little over. The band holds all of it.
                // (A glider's folded membrane, past it, runs through the band:
                // #434's slab, anchored at the ankle, is not the leg.)
                let (r_knee, r_foot) = rig.bone_radii(foot);
                let node = r_knee + (r_foot - r_knee) * BAND_AT;
                let skin = [MeshKind::Skin];
                let mut close = Ring::cut_of(&avatar, &skin, station, axis, face, node * 1.2)
                    .expect("a leg's skin");
                close.points.retain(|point| point.length() <= node * 1.2);
                let skin_fill = filled(&close, &seated, station, start, bore);
                assert!(
                    skin_fill <= 1.0,
                    "{name} at {socket:?}: skin out to {skin_fill:.3} of the band"
                );
                // And it is the size of the leg, not of what is folded by it:
                // by the joints' own radii, which no membrane widens.
                let (across, through) = (bore * seated.scale.x, bore * seated.scale.z);
                println!(
                    "{name} at {socket:?}: a {across:.3} by {through:.3} m bore on a \
                     {node:.3} m leg"
                );
                assert!(
                    across.max(through) < node * SNUG && across.min(through) > node * 0.4,
                    "{name} at {socket:?}: a {across:.3} by {through:.3} m bore on a \
                     {node:.3} m leg"
                );
                let hit = rig.nearest_bone(at);
                assert!(
                    hit.joint == foot || hit.joint == knee,
                    "{name} at {socket:?}: the band stands nearest {:?}",
                    rig.joints[hit.joint].zone
                );
            }
        }
    }
}
