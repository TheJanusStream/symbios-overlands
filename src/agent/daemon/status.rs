//! `agent status` (#1416): who, where and with whom the agent is, as one
//! JSON object.
//!
//! Directions are given in the agent's own frame - so many metres ahead, so
//! many to the right - rather than as angles, which would need a convention
//! explained before they meant anything. Positions are world coordinates,
//! for the commands that take a point.
//!
//! Besides the people in the world, `nearby` names the placed things around
//! the agent - buildings, plants, signs, and the gateways and portals that
//! lead elsewhere - where they are drawn, which is not always where the
//! record puts them (a placement kept out of the water is moved to dry land).
//! A walk into one of them ends `stuck`; this is how the agent sees it coming.
//! A thing's name is its world owner's words, so it says whose (`named_by`).
//!
//! `editing` says whether the world the agent stands in is its own to edit,
//! whether it may save at all, which of its records hold unsaved edits or a
//! save under way, and what an undo or a redo would step (#1422). `gifts`
//! names the admin's offer waiting for an answer, if one is, and the
//! agent's own offers still waiting for theirs (#1423).

use avian3d::prelude::CollidingEntities;
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde_json::{Value, json};

use crate::config::agent::{NEARBY_MAX, NEARBY_RADIUS_M};
use crate::network::{LinkPhase, PeerResolve};
use crate::pds::{Generator, GeneratorKind, LocomotionConfig, Placement};
use crate::state::{
    AppState, CurrentRoomDid, LiveAvatarRecord, LiveRoomRecord, LocalPlayer, RemotePeer,
    TravelingTo,
};
use crate::ui::gateway::{GatewayDismissed, GatewayPicker};
use crate::world_builder::{GatewayMarker, PlacementMarker, PlacementUnit, PortalMarker};

use super::super::admin::Admin;
use super::{hundredths, hundredths3};

/// Where the agent's body is and which way it faces on the ground.
pub(super) struct Pose {
    pub(super) position: Vec3,
    /// Unit, horizontal.
    pub(super) forward: Vec3,
}

impl Pose {
    /// `point` in the agent's own frame: metres ahead and metres to the
    /// right (negative is behind, and to the left).
    pub(super) fn frame_of(&self, point: Vec3) -> (f32, f32) {
        let offset = point - self.position;
        let right = self.forward.cross(Vec3::Y);
        (offset.dot(self.forward), offset.dot(right))
    }
}

/// The whole status object.
pub(super) fn snapshot(world: &mut World) -> Value {
    let pose = local_pose(world);
    let admin = world.get_resource::<Admin>().cloned();
    json!({
        "state": state_word(world.resource::<State<AppState>>().get()),
        "account": world.get_resource::<AtprotoSession>().map(|s| json!({
            "did": s.did,
            "handle": s.handle,
        })),
        "admin": admin.as_ref().map(Admin::to_json),
        "chat": chat_heard(admin.as_ref()),
        "room_did": world.get_resource::<CurrentRoomDid>().map(|room| room.0.clone()),
        "travelling_to": world.get_resource::<TravelingTo>().map(|t| t.target_did.clone()),
        "link": world.get_resource::<crate::network::LinkState>().map(|l| link_word(l.phase())),
        "locomotion": world
            .get_resource::<LiveAvatarRecord>()
            .map(|live| locomotion_word(&live.0.locomotion)),
        "position": pose.as_ref().map(|p| hundredths3(p.position)),
        "height_m": super::movement::height(world).map(hundredths),
        "facing": pose.as_ref().map(|p| [hundredths(p.forward.x), hundredths(p.forward.z)]),
        "movement": super::movement::describe(world),
        "peers": peers(world, pose.as_ref(), admin.as_ref()),
        "nearby": nearby(world, pose.as_ref()),
        "zone": zone(world),
        "editing": super::edit::describe(world),
        "gifts": super::gifts::describe(world),
        "interface": super::ui::describe(world),
    })
}

/// Whose chat the agent hears (#1427), and when it is nobody's, why.
fn chat_heard(admin: Option<&Admin>) -> Value {
    match admin {
        Some(_) => json!({ "hears": "admin_only" }),
        None => json!({
            "hears": "nobody",
            "why": "the agent was started without --admin",
        }),
    }
}

pub(super) fn local_pose(world: &mut World) -> Option<Pose> {
    let transform = *world
        .query_filtered::<&GlobalTransform, With<LocalPlayer>>()
        .iter(world)
        .next()?;
    Some(Pose {
        position: transform.translation(),
        forward: flat_forward(&transform),
    })
}

/// The way a body faces on the ground: its forward, level and unit.
fn flat_forward(transform: &GlobalTransform) -> Vec3 {
    let forward = transform.forward().as_vec3();
    Vec3::new(forward.x, 0.0, forward.z).normalize_or(Vec3::NEG_Z)
}

/// The players in the world whose identity is known, nearest first, each
/// where they are and which way they face - the world direction, as the
/// agent's own `facing` is given (#1437: building around a player meant
/// guessing it from a picture, and the guess was wrong). One whose body has
/// not been placed yet - no movement has reached the agent from them, as
/// from a browser tab asleep since they arrived - is listed with neither,
/// rather than at the spot it waits at, the map's centre ten metres up; and
/// one gone quiet says so.
fn peers(world: &mut World, pose: Option<&Pose>, admin: Option<&Admin>) -> Vec<Value> {
    let mut query = world.query::<(&RemotePeer, &GlobalTransform, Option<&PeerResolve>)>();
    let mut peers: Vec<(f32, Value)> = query
        .iter(world)
        .filter_map(|(peer, transform, resolve)| {
            let did = peer.did.as_ref()?;
            let position = transform.translation();
            let placed = resolve.is_some_and(|r| r.placed);
            let mut entry = json!({
                "did": did,
                "handle": peer.handle,
                "admin": admin.is_some_and(|admin| &admin.did == did),
                "muted": peer.muted,
                "placed": placed,
                "quiet": resolve.is_some_and(|r| r.quiet),
                "position": placed.then(|| hundredths3(position)),
                "facing": placed.then(|| {
                    let facing = flat_forward(transform);
                    [hundredths(facing.x), hundredths(facing.z)]
                }),
            });
            if !placed {
                return Some((f32::INFINITY, entry));
            }
            let distance = pose.map_or(f32::INFINITY, |pose| {
                let (ahead, right) = pose.frame_of(position);
                entry["distance_m"] = json!(hundredths(position.distance(pose.position)));
                entry["ahead_m"] = json!(hundredths(ahead));
                entry["right_m"] = json!(hundredths(right));
                position.distance(pose.position)
            });
            Some((distance, entry))
        })
        .collect();
    peers.sort_by(|a, b| a.0.total_cmp(&b.0));
    peers.into_iter().map(|(_, entry)| entry).collect()
}

/// The placed things within [`NEARBY_RADIUS_M`] of the agent, nearest first
/// and at most [`NEARBY_MAX`] of them, each by the name its world gives it.
///
/// A name is whatever the world's owner typed - in a stranger's world, up
/// to a few hundred characters of anything at all - so each one is marked
/// with whose it is (`named_by`, the world owner's DID) for the agent to
/// weigh against its own DID and its admin's (#1427).
fn nearby(world: &mut World, pose: Option<&Pose>) -> Vec<Value> {
    let Some(pose) = pose else {
        return Vec::new();
    };
    let named_by = world
        .get_resource::<CurrentRoomDid>()
        .map(|room| room.0.clone());
    let Some(record) = world.get_resource::<LiveRoomRecord>() else {
        return Vec::new();
    };
    // Named by placement index, which is what a drawn placement is tagged
    // with; `None` for what is not a thing to walk into.
    let named: Vec<Option<(String, &'static str)>> = record
        .0
        .placements
        .iter()
        .map(|placement| match placement {
            Placement::Absolute { generator_ref, .. } => record
                .0
                .generators
                .get(generator_ref)
                .and_then(thing_kind)
                .map(|kind| (generator_ref.clone(), kind)),
            _ => None,
        })
        .collect();
    let mut query = world.query::<(&PlacementMarker, &GlobalTransform)>();
    let mut things: Vec<(f32, Value)> = query
        .iter(world)
        .filter_map(|(marker, transform)| {
            let (name, kind) = named.get(marker.0)?.as_ref()?;
            let position = transform.translation();
            let distance = position.distance(pose.position);
            if distance > NEARBY_RADIUS_M {
                return None;
            }
            let (ahead, right) = pose.frame_of(position);
            Some((
                distance,
                json!({
                    "name": name,
                    "named_by": named_by,
                    "kind": kind,
                    "position": hundredths3(position),
                    "distance_m": hundredths(distance),
                    "ahead_m": hundredths(ahead),
                    "right_m": hundredths(right),
                }),
            ))
        })
        .collect();
    things.sort_by(|a, b| a.0.total_cmp(&b.0));
    things.truncate(NEARBY_MAX);
    things.into_iter().map(|(_, thing)| thing).collect()
}

/// The way out the agent's body stands in, if it stands in one (#1452): a
/// gateway's walk-in zone or a portal's, found among what the body touches -
/// the contacts the game's own zone watchers read - and named by the
/// placement it belongs to. For a gateway, `picker` says what the game made
/// of it: `open` (its list of destinations is up, which the agent never
/// reads), `dismissed` (closed while the body still stands in it), or
/// `not_open`. Walking into a gateway the agent built and reading `open`
/// here is how it knows the gateway works.
fn zone(world: &mut World) -> Value {
    let touching: Vec<Entity> = world
        .query_filtered::<&CollidingEntities, With<LocalPlayer>>()
        .iter(world)
        .flat_map(|touching| touching.iter().copied())
        .collect();
    let mut zones = world.query::<(
        Has<GatewayMarker>,
        Has<PortalMarker>,
        Option<&PlacementUnit>,
    )>();
    let found = touching.iter().find_map(|&entity| {
        let (gateway, portal, unit) = zones.get(world, entity).ok()?;
        let kind = match (gateway, portal) {
            (true, _) => "gateway",
            (false, true) => "portal",
            (false, false) => return None,
        };
        Some((kind, unit.map(|unit| unit.0)))
    });
    let Some((kind, index)) = found else {
        return Value::Null;
    };
    let name = index.and_then(|index| {
        let record = &world.get_resource::<LiveRoomRecord>()?.0;
        match record.placements.get(index)? {
            Placement::Absolute { generator_ref, .. }
            | Placement::Scatter { generator_ref, .. }
            | Placement::Grid { generator_ref, .. } => Some(generator_ref.clone()),
            Placement::Unknown => None,
        }
    });
    let mut zone = json!({ "kind": kind, "name": name });
    if kind == "gateway" {
        zone["picker"] = json!(if world.contains_resource::<GatewayPicker>() {
            "open"
        } else if world.contains_resource::<GatewayDismissed>() {
            "dismissed"
        } else {
            "not_open"
        });
    }
    zone
}

/// What a placed generator tree is to someone walking about - or `None` for
/// the ground, the water, the roads and the weather, which are not in the
/// way.
///
/// A way out is looked for through the whole tree, not just its root: the
/// seeded social gateway is a structure whose gateway zone is one of its
/// nodes, and a catalogue building can carry a portal the same way.
pub(super) fn thing_kind(generator: &Generator) -> Option<&'static str> {
    match &generator.kind {
        GeneratorKind::Terrain(_)
        | GeneratorKind::Water { .. }
        | GeneratorKind::RoadNetwork(_)
        | GeneratorKind::ParticleSystem(_) => return None,
        _ => {}
    }
    if holds(generator, &|kind| {
        matches!(kind, GeneratorKind::Gateway { .. })
    }) {
        return Some("gateway");
    }
    if holds(generator, &|kind| {
        matches!(kind, GeneratorKind::Portal { .. })
    }) {
        return Some("portal");
    }
    Some(match generator.kind {
        GeneratorKind::Sign { .. } => "sign",
        GeneratorKind::LSystem { .. } => "plant",
        _ => "structure",
    })
}

/// Does `generator`, or any node under it, have a kind `is` accepts?
fn holds(generator: &Generator, is: &dyn Fn(&GeneratorKind) -> bool) -> bool {
    is(&generator.kind) || generator.children.iter().any(|child| holds(child, is))
}

/// How the agent's body moves, which decides what `walk-to` can do with it.
fn locomotion_word(locomotion: &LocomotionConfig) -> &'static str {
    match locomotion {
        LocomotionConfig::Humanoid(_) => "humanoid",
        LocomotionConfig::Car(_) => "car",
        LocomotionConfig::HoverBoat(_) => "hover_boat",
        LocomotionConfig::Airplane(_) => "airplane",
        LocomotionConfig::Helicopter(_) => "helicopter",
        LocomotionConfig::Unknown => "unknown",
    }
}

fn state_word(state: &AppState) -> &'static str {
    match state {
        AppState::Login => "signing_in",
        AppState::Loading => "loading",
        AppState::InGame => "in_world",
    }
}

fn link_word(phase: LinkPhase) -> &'static str {
    match phase {
        LinkPhase::Down => "offline",
        LinkPhase::Connecting => "connecting",
        LinkPhase::Connected => "connected",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bevy looks down -Z by default; a body turned to face +Z has -X on its
    /// right. Pinned because the whole point of the frame is that an agent
    /// can act on "to the right" without knowing any of that.
    #[test]
    fn right_is_the_bodys_right() {
        let facing_z = Pose {
            position: Vec3::ZERO,
            forward: Vec3::Z,
        };
        assert_eq!(facing_z.frame_of(Vec3::new(0.0, 0.0, 5.0)), (5.0, 0.0));
        assert_eq!(facing_z.frame_of(Vec3::new(-3.0, 0.0, 0.0)), (0.0, 3.0));

        let facing_default = Pose {
            position: Vec3::new(10.0, 0.0, 10.0),
            forward: Vec3::NEG_Z,
        };
        assert_eq!(
            facing_default.frame_of(Vec3::new(12.0, 7.0, 10.0)),
            (0.0, 2.0),
            "Bevy's own right, +X, for a body looking down -Z; height ignored"
        );
    }

    fn peer(did: Option<&str>) -> RemotePeer {
        RemotePeer {
            peer_id: serde_json::from_str("\"00000000-0000-0000-0000-000000000001\"")
                .expect("a uuid"),
            did: did.map(str::to_owned),
            handle: Some("bob.test".into()),
            muted: false,
            avatar: None,
            build: None,
            connected_at: 0.0,
        }
    }

    /// The snapshot a signed-in agent standing in a world gets: its pose, and
    /// each peer whose identity is known, nearest first, in its own frame.
    #[test]
    fn a_snapshot_places_the_peers_around_the_agent() {
        let mut world = World::new();
        world.insert_resource(State::new(AppState::InGame));
        world.insert_resource(CurrentRoomDid("did:plc:home".into()));
        world.spawn((
            LocalPlayer,
            GlobalTransform::from(Transform::from_xyz(0.0, 1.0, 0.0).looking_to(Vec3::Z, Vec3::Y)),
        ));
        let placed = || PeerResolve {
            placed: true,
            ..default()
        };
        world.spawn((
            peer(Some("did:plc:far")),
            placed(),
            GlobalTransform::from(Transform::from_xyz(0.0, 1.0, 20.0)),
        ));
        world.spawn((
            peer(Some("did:plc:near")),
            placed(),
            GlobalTransform::from(Transform::from_xyz(-4.0, 1.0, 0.0)),
        ));
        world.spawn((
            peer(None),
            GlobalTransform::from(Transform::from_xyz(1.0, 1.0, 1.0)),
        ));
        world.insert_resource(Admin {
            did: "did:plc:far".into(),
            handle: Some("far.test".into()),
        });

        let status = snapshot(&mut world);

        assert_eq!(status["state"], "in_world");
        assert_eq!(status["room_did"], "did:plc:home");
        assert_eq!(status["position"], json!([0.0, 1.0, 0.0]));
        assert_eq!(status["facing"], json!([0.0, 1.0]));
        let peers = status["peers"].as_array().expect("peers");
        assert_eq!(peers.len(), 2, "a peer with no DID yet is not listed");
        assert_eq!(peers[0]["did"], "did:plc:near", "nearest first");
        assert_eq!(peers[0]["right_m"], 4.0);
        assert_eq!(peers[1]["ahead_m"], 20.0);
        assert_eq!(peers[1]["distance_m"], 20.0);
        assert_eq!(
            (&peers[0]["admin"], &peers[1]["admin"]),
            (&json!(false), &json!(true)),
            "the admin is picked out by DID"
        );
        assert_eq!(
            status["admin"],
            json!({ "did": "did:plc:far", "handle": "far.test" })
        );
        assert_eq!(status["chat"], json!({ "hears": "admin_only" }));
    }

    /// THE CASE THAT ASKED FOR THIS (#1421): a player whose browser tab slept
    /// through their arrival was listed at the map's centre ten metres up -
    /// their spawn stand-in - as if they stood there. Unplaced, they are
    /// listed with no position, after everyone who has one.
    #[test]
    fn a_player_not_yet_placed_has_no_position() {
        let mut world = World::new();
        world.insert_resource(State::new(AppState::InGame));
        world.spawn((
            LocalPlayer,
            GlobalTransform::from(Transform::from_xyz(0.0, 1.0, 0.0)),
        ));
        world.spawn((
            peer(Some("did:plc:asleep")),
            PeerResolve::default(),
            GlobalTransform::from(Transform::from_xyz(0.0, 10.0, 0.0)),
        ));
        world.spawn((
            peer(Some("did:plc:here")),
            PeerResolve {
                placed: true,
                ..default()
            },
            GlobalTransform::from(Transform::from_xyz(30.0, 1.0, 0.0)),
        ));

        let status = snapshot(&mut world);

        let peers = status["peers"].as_array().expect("peers");
        assert_eq!(peers[0]["did"], "did:plc:here", "the placed one first");
        assert_eq!(peers[1]["did"], "did:plc:asleep");
        assert_eq!(peers[1]["placed"], false);
        assert!(peers[1]["position"].is_null(), "{}", peers[1]);
        assert!(peers[1]["distance_m"].is_null(), "{}", peers[1]);
    }

    /// Each placed player says which way they face, as the agent's own
    /// `facing` does (#1437); one not yet placed has no facing to give.
    #[test]
    fn a_snapshot_says_which_way_each_placed_player_faces() {
        let mut world = World::new();
        world.insert_resource(State::new(AppState::InGame));
        world.spawn((
            LocalPlayer,
            GlobalTransform::from(Transform::from_xyz(0.0, 1.0, 0.0)),
        ));
        world.spawn((
            peer(Some("did:plc:east")),
            PeerResolve {
                placed: true,
                ..default()
            },
            GlobalTransform::from(
                Transform::from_xyz(5.0, 1.0, 0.0).looking_to(Vec3::new(1.0, -0.3, 1.0), Vec3::Y),
            ),
        ));
        world.spawn((
            peer(Some("did:plc:asleep")),
            PeerResolve::default(),
            GlobalTransform::from(Transform::from_xyz(0.0, 10.0, 0.0)),
        ));

        let status = snapshot(&mut world);

        let peers = status["peers"].as_array().expect("peers");
        assert_eq!(peers[0]["did"], "did:plc:east");
        assert_eq!(
            peers[0]["facing"],
            json!([0.71, 0.71]),
            "level and unit, pitch dropped"
        );
        assert!(peers[1]["facing"].is_null(), "{}", peers[1]);
    }

    /// Before the world is loaded there is no body: the snapshot says so with
    /// nulls rather than inventing a pose.
    #[test]
    fn a_snapshot_before_the_world_has_no_pose() {
        let mut world = World::new();
        world.insert_resource(State::new(AppState::Loading));

        let status = snapshot(&mut world);

        assert_eq!(status["state"], "loading");
        assert!(status["position"].is_null());
        assert!(status["account"].is_null());
        assert_eq!(status["peers"], json!([]));
    }

    /// No admin: `status` says the agent hears nobody, and why, so an agent
    /// that expected chat knows it was never going to come.
    #[test]
    fn a_snapshot_without_an_admin_says_chat_is_off_and_why() {
        let mut world = World::new();
        world.insert_resource(State::new(AppState::InGame));

        let status = snapshot(&mut world);

        assert!(status["admin"].is_null());
        assert_eq!(status["chat"]["hears"], "nobody");
        assert!(
            status["chat"]["why"]
                .as_str()
                .is_some_and(|why| why.contains("--admin")),
            "{}",
            status["chat"]
        );
    }

    /// THE CASE THAT ASKED FOR THIS: an agent walked straight into its own
    /// world's gateway and could only report `stuck`. With the seeded world
    /// the agent actually has, `nearby` names the gateway and the monument
    /// where they are drawn, nearest first, and leaves the ground out.
    #[test]
    fn nearby_names_what_is_in_the_way_and_leaves_the_ground_out() {
        let record = crate::pds::RoomRecord::default_for_did("did:plc:nearby");
        let index_of = |name: &str| {
            record
                .placements
                .iter()
                .position(|p| {
                    matches!(p, Placement::Absolute { generator_ref, .. } if generator_ref == name)
                })
                .unwrap_or_else(|| panic!("a seeded {name}"))
        };
        let (gateway, monument, terrain) = (
            index_of("social_gateway"),
            index_of("owner_monument"),
            index_of("base_terrain"),
        );
        let mut world = World::new();
        world.insert_resource(State::new(AppState::InGame));
        world.insert_resource(LiveRoomRecord(record));
        world.insert_resource(CurrentRoomDid("did:plc:nearby".into()));
        world.spawn((
            LocalPlayer,
            GlobalTransform::from(Transform::from_xyz(0.0, 0.0, 0.0).looking_to(Vec3::Z, Vec3::Y)),
        ));
        let at = |x: f32, z: f32| GlobalTransform::from(Transform::from_xyz(x, 0.0, z));
        world.spawn((PlacementMarker(gateway), at(0.0, 10.0)));
        world.spawn((PlacementMarker(monument), at(3.0, 0.0)));
        world.spawn((PlacementMarker(terrain), at(0.0, 0.0)));
        world.spawn((PlacementMarker(gateway), at(0.0, NEARBY_RADIUS_M + 1.0)));

        let status = snapshot(&mut world);

        let nearby = status["nearby"].as_array().expect("nearby");
        let names: Vec<&str> = nearby.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, ["owner_monument", "social_gateway"], "{nearby:?}");
        assert_eq!(
            nearby[0]["right_m"], -3.0,
            "to the left of a body facing +Z"
        );
        assert_eq!(nearby[1]["kind"], "gateway");
        assert_eq!(nearby[1]["ahead_m"], 10.0);
        assert!(
            nearby.iter().all(|t| t["named_by"] == "did:plc:nearby"),
            "every name says whose it is: {nearby:?}"
        );
    }

    /// A world with the seeded social gateway at `gateway`'s index, and the
    /// agent's body touching `touching`.
    fn standing_in(touching: impl FnOnce(&mut World, usize) -> Vec<Entity>) -> World {
        let record = crate::pds::RoomRecord::default_for_did("did:plc:zones");
        let gateway = record
            .placements
            .iter()
            .position(|p| {
                matches!(p, Placement::Absolute { generator_ref, .. } if generator_ref == "social_gateway")
            })
            .expect("a seeded gateway");
        let mut world = World::new();
        world.insert_resource(State::new(AppState::InGame));
        world.insert_resource(LiveRoomRecord(record));
        let touched = touching(&mut world, gateway);
        world.spawn((
            LocalPlayer,
            GlobalTransform::default(),
            CollidingEntities(touched.into_iter().collect()),
        ));
        world
    }

    /// THE CASE THAT ASKED FOR THIS (#1452): the admin found the agent's own
    /// gateway did nothing, and the agent, landed in its zone, could tell
    /// neither whether it stood in it nor whether the game had made anything
    /// of it. It names the gateway it stands in, and what became of its list
    /// of destinations.
    #[test]
    fn a_body_in_a_gateway_names_it_and_whether_its_picker_is_up() {
        let mut world = standing_in(|world, gateway| {
            let ground = world.spawn(PlacementUnit(0)).id();
            let zone = world.spawn((GatewayMarker, PlacementUnit(gateway))).id();
            vec![ground, zone]
        });

        assert_eq!(
            snapshot(&mut world)["zone"],
            json!({ "kind": "gateway", "name": "social_gateway", "picker": "not_open" })
        );
        world.insert_resource(GatewayPicker::default());
        assert_eq!(snapshot(&mut world)["zone"]["picker"], "open");
        world.remove_resource::<GatewayPicker>();
        world.insert_resource(GatewayDismissed);
        assert_eq!(snapshot(&mut world)["zone"]["picker"], "dismissed");
    }

    /// A portal is a way out too, with no picker: it takes the body away.
    #[test]
    fn a_body_in_a_portal_names_it() {
        let mut world = standing_in(|world, _| {
            let portal = PortalMarker {
                target_did: "did:plc:elsewhere".into(),
                target_pos: Vec3::ZERO,
            };
            vec![
                world
                    .spawn((portal, PlacementUnit(PlacementUnit::NONE)))
                    .id(),
            ]
        });

        assert_eq!(
            snapshot(&mut world)["zone"],
            json!({ "kind": "portal", "name": null })
        );
    }

    /// Touching the ground and a wall is standing in no way out at all.
    #[test]
    fn a_body_touching_only_solid_things_stands_in_no_zone() {
        let mut world = standing_in(|world, _| vec![world.spawn(PlacementUnit(0)).id()]);

        assert!(snapshot(&mut world)["zone"].is_null());
    }
}
