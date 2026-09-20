//! Headless render tool - renders any subject (avatar / catalogue item /
//! worn attachment / primitive / whole seeded room / the compiled world)
//! through the **real** spawn path
//! ([`crate::player::visuals::spawn_avatar_visuals`], which routes every node
//! kind - primitives, Shape grammar, L-system - through the same machinery the
//! game uses) into a multi-angle **contact-sheet** PNG, or one camera along a
//! rig into an animated **clip** (GIF). Lets the agent self-validate
//! geometry/materials without manual in-game screenshots, and make the
//! pictures a README needs.
//!
//! Lives in the library (not the `render` bin) so it can reach the
//! crate-internal `SpawnCtx`/cache resources; the bin is a one-line shim.
//!
//! ```text
//! cargo run --bin render -- --avatar 1          # seed or DID
//! cargo run --bin render -- --catalogue villa   # any catalogue slug
//! cargo run --bin render -- --prim tube         # a single primitive kind
//! cargo run --bin render -- --room 3            # the seeded settlement, flat
//! cargo run --bin render -- --world 3           # the seeded WORLD, as the game
//! #                                             # compiles it (terrain, streets,
//! #                                             # district, water, sky)
//! cargo run --bin render -- --world 3 --frames 60 --sweep 30
//! #                                             # a 60-frame orbit clip → .gif
//! cargo run --bin render -- --world 3 --walker 7 --focus walker --frames 48
//! #                                             # a seeded body walking it
//! cargo run --bin render -- --world 3 --editor --width 1280 --height 720
//! #                                             # the game's World Editor
//! #                                             # over it (#1353)
//! cargo run --bin render -- --catalogue villa --frames 36 --sweep 360
//! #                                             # a turntable clip
//! cargo run --bin render -- --generator g.json  # a dumped/edited Generator
//! cargo run --bin render -- --play-view --lineup 12,40,7 --reference-figure
//! #                                             # subjects side by side at
//! #                                             # the chase camera's range
//! cargo run --bin render -- --catalogue lsys_palm --ages 2,3,4,5
//! #                                             # age-progression grid (#908)
//! cargo run --bin render -- --wear satchel      # a wearable, worn (#1088)
//! cargo run --bin render -- --stitch a-frames,b-frames --out ab.gif
//! #                                             # PNG frame dirs → one GIF
//! # → /tmp/avatar-render/<label>.png  (front / ¾ / side / back tiles;
//! #   with --ages one such row per iteration count), or <label>.gif
//! ```
//!
//! `--wear <slug>` is the attachments instrument (#1088): it dresses seeded
//! rigged bodies in a catalogue wearable and sheets one body per row, so a
//! garment is judged on the anatomy it has to fit rather than in isolation.
//! `--wear-bodies N` sets how many bodies (default 4) and `--wear-socket
//! <engine socket>` overrides where it is seated - the tool for "what would
//! this look like on the other hip". Output is labelled
//! `wear-<slug>-<socket>`.
//!
//! `--world` (see `world.rs`) is the only subject that is *compiled* rather
//! than spawned: it registers the game's own terrain, road, lot, placement
//! and atmosphere pipelines and waits for them to settle. It always shoots
//! one camera on the rig; `--frames N` turns any single-camera shot into a
//! clip, with the camera path set by `--focus` / `--dist` / `--elev` /
//! `--yaw` / `--sweep` (and their `-end` dollies) - see `rig.rs`.
//!
//! `--editor` (see `editor.rs`) draws the game's own editing surfaces over
//! a `--world` shot: the toolbar, the World Editor, the Catalogue and the
//! transform gizmo, under an offline stand-in session for the world's owner.
//! `--editor-script` plays gestures on the tool's clock (see
//! `editor/script.rs`), and `--downscale N` writes a single-camera shot N
//! times smaller than it renders, so an interface laid out at 1280x720 can be
//! written at 640x360.
//!
//! `--play-view` (#1360) is the play-distance instrument: one 1920x1080 frame
//! from [`crate::config::camera::ORBIT_RADIUS`] metres at that module's
//! `ORBIT_PITCH`, on the lens the game leaves at Bevy's default, over a lit
//! ground plane, with every subject stood where the game stands it. The frame
//! a vehicle design is accepted on - detail that reads on a zoomed sheet is
//! sub-pixel here. `--lineup a,b,c` puts several subjects in it side by side
//! (seeds, `--generator` files or DIDs) and `--reference-figure` adds a
//! 1.75 m mannequin as the ruler.
//!
//! Subject precedence, when more than one is given: `--lineup` >
//! `--generator` > `--world` > `--terrain` > `--room` > `--prim` > `--wear` >
//! `--catalogue` > `--avatar`, with the no-render modes ahead of all of
//! them.
//!
//! The same binary also hosts fifteen no-render modes that short-circuit
//! before any render app stands up: the avatar surveys (`--family-seeds`,
//! `--outfit`, `--find-part`), the fit audits (`--settlement-drop`,
//! `--foundation-audit`, `--gateway-fit`), the census / plot tooling
//! (`--room-census`, `--scatter-census`, `--scatter-plot`), `--dump` (print a
//! subject's `Generator` JSON), `--road-dump` (road-graph topology stats),
//! `--describe` (a seeded room's roll and atmosphere, without a render),
//! `--stitch` (PNG frame directories → one GIF) and the offline session-log
//! analyzers `--analyze-session` / `--diff-sessions`. See the per-arg docs
//! on `Args`.

use std::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::prelude::*;
use bevy::time::TimeSystems;
use bevy::window::ExitCondition;
use bevy_symbios_avatar::AvatarSystems;
use clap::Parser;

use crate::pds::AvatarBody;
use crate::pds::avatar::default_visuals::{build_for_did_in_livery, build_in_livery};
use crate::pds::types::{Fp, Fp2};
use crate::pds::{Generator, GeneratorKind, RoomRecord};

mod editor;
mod figure;
mod gif;
mod headless;
mod rig;
mod text_tools;
mod world;

use headless::{Capture, Clock, PlayView, RenderJob, Ride, Subject, drive, setup};
use rig::{CameraRig, Focus};
use text_tools::{
    analyze_session, describe_rooms, diff_sessions, dump_road_graph, find_part, print_family_seeds,
    print_foundation_audit, print_gateway_fit, print_outfit, print_settlement_drop, room_census,
    scatter_census, scatter_plot,
};
use world::{WalkerSpec, WorldSpec};

/// Camera yaw per tile (degrees), left→right: front, ¾, side, back. Avatars /
/// vehicles face local -Z, so the camera sits on the -Z side (`cos 180 = -1`)
/// to face the subject.
const ANGLES: [f32; 4] = [180.0, 135.0, 90.0, 0.0];
/// Default perspective FOV (matches Bevy's `PerspectiveProjection` default).
const FOV: f32 = std::f32::consts::FRAC_PI_4;
/// Frames to run (after framing) before capturing, so that, since the clock
/// advances by a fixed slice each frame, particle emitters reach their
/// steady-state plume before the shutter opens. It is NOT what waits for
/// procedural textures: a bake is seconds on a thread pool, not frames, and
/// on a fast GPU these 200 frames pass in under a second, so the shutter
/// holds on `world::BakesInFlight` after the count instead (#1351).
const WARMUP: u32 = 200;
const OUT_DIR: &str = "/tmp/avatar-render";
/// The default clip frame rate. GIF counts delays in centiseconds, so 12.5
/// (8 cs) is the nearest rate to "about a dozen a second" the format can
/// actually express; see `rig::delay_cs`.
const DEFAULT_FPS: f32 = 12.5;
/// The default single-camera frame, width × height. 16:9, and a width that
/// is a multiple of 64 so the GPU readback needs no row padding.
const DEFAULT_FRAME: (u32, u32) = (896, 504);
/// `--play-view`'s frame: the 1080 lines the pixels-per-metre arithmetic is
/// quoted at (#1360), and 1920 is a multiple of 64 so the readback still
/// needs no padding.
const PLAY_FRAME: (u32, u32) = (1920, 1080);

#[derive(Parser)]
#[command(
    about = "Headless contact-sheet / clip renderer for avatars, catalogue, primitives, rooms and worlds"
)]
struct Args {
    /// Avatar subject: a u64 seed or a DID string.
    #[arg(long)]
    avatar: Option<String>,
    /// Judge a subject at the distance the game shows it (#1360): one
    /// 1920x1080 frame from [`crate::config::camera::ORBIT_RADIUS`] metres
    /// at [`ORBIT_PITCH`](crate::config::camera::ORBIT_PITCH), on the lens
    /// the game leaves at Bevy's default, over a ground plane.
    ///
    /// Every earlier pass at the vehicles was judged on zoomed contact
    /// sheets, where a 1.2 cm rail looks like a rail; at the chase camera's
    /// rest it is 1.3 px. This is the frame a design is accepted on, and the
    /// zoomed sheets are second.
    ///
    /// The ground plane is what makes hover and wheels legible, so the
    /// subject is stood where the game stands it: a craft that settles on a
    /// suspension goes at its derived ride height
    /// ([`ground_ride_height`](crate::pds::avatar::default_visuals::ground_ride_height)),
    /// and anything with no such record - an airship, a `--generator` file,
    /// the reference figure - rests on its own drawn bounds unless
    /// `--ride-height` says otherwise.
    ///
    /// Puts the tool on its single-camera path, so `--frames N` turns it
    /// into a clip and `--yaw` / `--sweep` / `--zoom` still apply.
    #[arg(long, default_value_t = false)]
    play_view: bool,
    /// Stand a 1.75 m mannequin, built from plain primitives, beside the
    /// subject as the last line-up slot - the scale rule a vehicle is read
    /// against. `--avatar` refuses a rigged humanoid seed, so without this
    /// there is no tool shot of a craft beside a body at all.
    #[arg(long, default_value_t = false)]
    reference_figure: bool,
    /// Draw every seeded vehicle subject in the heritage livery at this
    /// index instead of the one its seed picked - the way a curated scheme
    /// list is judged (#1365).
    ///
    /// The schemes cannot be compared by hunting for seeds that happen to
    /// have rolled each one: two seeds differ in proportion, stance, wear and
    /// craft type as well as in colour, so the comparison is never of the
    /// colour. This holds everything else still and changes only the scheme.
    /// The index is into the table the subject's craft type paints from: its
    /// family's heritage list
    /// ([`livery::BOAT_LIVERIES`](crate::pds::avatar::livery::BOAT_LIVERIES),
    /// [`livery::SKIFF_LIVERIES`](crate::pds::avatar::livery::SKIFF_LIVERIES)),
    /// or a type's own
    /// ([`livery::WAGON_LIVERIES`](crate::pds::avatar::livery::WAGON_LIVERIES),
    /// [`livery::BUGGY_LIVERIES`](crate::pds::avatar::livery::BUGGY_LIVERIES)
    /// and a raider's
    /// [`livery::RAIDER_LIVERIES`](crate::pds::avatar::livery::RAIDER_LIVERIES),
    /// [`livery::CYCLECAR_LIVERIES`](crate::pds::avatar::livery::CYCLECAR_LIVERIES),
    /// [`livery::JUNK_LIVERIES`](crate::pds::avatar::livery::JUNK_LIVERIES)).
    /// It WRAPS, so a survey loop that runs past the end draws each scheme
    /// once rather than the last one twice. `--outfit` prints the name a seed
    /// picked for itself.
    ///
    /// Applies to `--avatar` and to every seeded `--lineup` slot; a
    /// `--generator` file carries its own colours and is unaffected.
    #[arg(long)]
    livery: Option<usize>,
    /// Several subjects side by side in one shot, comma-separated. Each
    /// entry is a `u64` avatar seed, a path to a `--generator` JSON file, or
    /// a DID - so a hand-written prototype can stand next to the seeded
    /// fleet it is replacing, which is the whole point of having the view
    /// during a redesign. Outranks every other subject.
    ///
    /// With `--play-view` the slots stand on one arc at the chase camera's
    /// distance, each yawed to present the same aspect, so every one of them
    /// is at the game's range in the same frame. Without it they are
    /// sheeted the way `--ages` sheets its rows: four angles apiece, one row
    /// per slot, at a shared camera distance.
    #[arg(long)]
    lineup: Option<String>,
    /// With `--play-view`: where a slot's origin sits above the ground, in
    /// metres - one value for every slot, or a comma-separated list with
    /// `auto` for the slots that should keep their derived height. The
    /// answer for a subject the tool cannot derive one for: a `--generator`
    /// prototype has no locomotion record at all, so this is how a hovering
    /// hull is stood at its hover height rather than beached on its keel.
    #[arg(long)]
    ride_height: Option<String>,
    /// List the first `--family-count` seeds whose
    /// [`ChassisFamily`](crate::seeded_defaults::ChassisFamily) matches
    /// (`humanoid` | `boat` | `airship` | `skiff`) and exit - a survey aid for
    /// the avatar overhaul: pick seeds from the printed list, then render each
    /// with `--avatar <seed>`. Highest precedence (prints, never renders).
    #[arg(long)]
    family_seeds: Option<String>,
    /// How many seeds `--family-seeds` prints (also the cap for
    /// `--find-part`).
    #[arg(long, default_value_t = 8)]
    family_count: usize,
    /// Narrow `--family-seeds` to one seeded craft type (#1362) - `sloop`,
    /// `longship`, `steamtug`, `junk`, `runabout`, `scow` for boats;
    /// `roadster`, `dunebuggy`, `armouredcar`, `cyclecar`, `wagon`, `rover`
    /// for skiffs. The survey aid each craft-type slice opens with: find the
    /// seeds its type was picked for, then render them. Craft types are a
    /// property of the seed, so this answers for a type before anything
    /// builds it.
    #[arg(long)]
    craft: Option<String>,
    /// Print one avatar's resolved outfit (chassis / style / socio tiers /
    /// slot→slug) and exit - a `u64` seed or a DID. A no-render survey aid for
    /// the avatar overhaul: the built geometry carries no slugs, so this is how
    /// to see which optional parts an avatar rolled.
    #[arg(long)]
    outfit: Option<String>,
    /// Scan seeds and print the first `--family-count` whose outfit rolls the
    /// given part slug (e.g. `boat_bow_ram`), with each one's style + tiers,
    /// then exit - finds render-verification seeds for a styled part.
    #[arg(long)]
    find_part: Option<String>,
    /// Measure the terrain drop real seeded settlements span (#1009) over
    /// this many seeds and exit - the empirical basis for the plinth rule.
    #[arg(long)]
    settlement_drop: Option<u64>,
    /// Print the foundation-depth audit (#1009) and exit: every
    /// settlement-placeable entry against the plinth depth its footprint
    /// demands. Pass `all` to include entries that already satisfy it.
    #[arg(long)]
    foundation_audit: Option<String>,
    /// Print the gateway veil-fit report (#1006) and exit: per gateway, the
    /// translucent zone's box against the frame around it, naming any face
    /// that floats in open air or juts past the mouth. Pass a slug to check
    /// one gateway, or `all` for every one.
    #[arg(long)]
    gateway_fit: Option<String>,
    /// Wear subject (#1088): a wearable catalogue slug (e.g. `satchel`),
    /// rendered WORN on rigged seeded bodies - one grid row per body seed ×
    /// pose (rest, walk at two opposite cycle extremes), four orbit angles
    /// per row. The item is engine-seated at the entry's `wear_socket` with
    /// the outward yaw, exactly as a fresh in-game Wear lands. Judging the
    /// item as world decor stays `--catalogue`'s job.
    #[arg(long)]
    wear: Option<String>,
    /// With `--wear`: how many seeded bodies to sheet (default 4).
    #[arg(long, default_value_t = 4)]
    wear_bodies: usize,
    /// With `--wear`: override the socket (an engine socket name like
    /// `left-hip`, `crown`, `back`) instead of the entry's own
    /// `wear_socket` - the tool for "what would this look like elsewhere".
    #[arg(long)]
    wear_socket: Option<String>,
    /// Catalogue subject: an entry slug (e.g. `villa`, `bench`, `wizard_tower`).
    #[arg(long)]
    catalogue: Option<String>,
    /// With `--catalogue <plant-slug>`: apply that plant's named material
    /// re-skin (#910) before rendering - e.g.
    /// `--catalogue lsys_monopodial_tree --variant larch_gold`. Variants
    /// change bark/foliage materials only, never geometry, so this composes
    /// with `--ages`. An unknown name renders the entry's default materials
    /// (the same fallback the seeded pools get); pass `--variant list` to
    /// print the entry's available variants and exit.
    #[arg(long)]
    variant: Option<String>,
    /// Render a [`Generator`] deserialized from a JSON file. Lets the agent
    /// iterate on an L-system grammar (or any generator) without recompiling
    /// the crate: `--dump` a catalogue entry to seed the JSON, edit the
    /// grammar / scalars, re-render. Highest precedence among the render
    /// subjects (`--generator` > `--world` > `--terrain` > `--room` > `--prim`
    /// then `--wear` > `--catalogue` > `--avatar`); the no-render modes still
    /// run first.
    #[arg(long)]
    generator: Option<String>,
    /// With `--catalogue <slug>`, `--prim <tag>` (overrides applied), or
    /// `--avatar <seed|did>`: print that subject's built [`Generator`] as
    /// pretty JSON to stdout and exit (a valid seed file for `--generator`,
    /// enabling a no-recompile geometry-iteration loop).
    #[arg(long, default_value_t = false)]
    dump: bool,
    /// Age-progression sweep for an L-system subject (#908): comma-separated
    /// iteration counts (e.g. `--ages 2,3,4,5`). Renders a grid sheet instead
    /// of the single row - one row per age (top→bottom in argument order),
    /// columns = the four angles - with every row framed at one shared camera
    /// distance so relative plant size across ages stays honest. Each count
    /// overrides `iterations` on every L-system node in the subject's
    /// generator tree; combines with any single-generator subject
    /// (`--generator` > `--prim` > `--catalogue` > `--avatar` - the four that
    /// resolve to a `Subject::Single`), panics on `--room` and `--wear`,
    /// which do not, or on a subject without an L-system node. Values above the
    /// record sanitiser cap (12) are accepted here but blow up derivation
    /// size fast - the `MAX_LSYSTEM_STATE_LEN` guard still applies.
    #[arg(long)]
    ages: Option<String>,
    /// Primitive subject: a kind tag (`cuboid`, `sphere`, `tube`, `bevel`, …).
    #[arg(long)]
    prim: Option<String>,
    /// World subject: a u64 seed or DID - the seeded room compiled by the
    /// game's own pipeline, as the login backdrop and a fresh sign-in show
    /// it: real terrain under its splat, streets and the district grown
    /// along them, every placement snapped and filtered against the ground,
    /// water volumes, the room's sun, sky, fog and cloud deck. One camera on
    /// the rig (`--focus` / `--dist` / `--elev` / `--yaw` / `--sweep`), a
    /// still PNG or a `--frames N` clip. Outranks every subject but
    /// `--generator`.
    #[arg(long)]
    world: Option<String>,
    /// With `--world`: open the game's own editing surfaces over it - the
    /// toolbar and the World Editor, drawn by the game's egui systems into
    /// the same frame (#1353). The editor is owner-only, so an offline
    /// stand-in session for the world's own DID is signed in; nothing talks
    /// to the network.
    #[arg(long, default_value_t = false)]
    editor: bool,
    /// With `--editor`: the World Editor tab the shot opens on -
    /// `environment` (the default), `items`, `placements`, `effects` or
    /// `raw`.
    #[arg(long)]
    editor_tab: Option<String>,
    /// With `--editor`: an item to open selected, by its name in the record
    /// (`--describe` lists them) - its tree row on `items`, its first
    /// placement on `placements` - which is where the in-world gizmo goes.
    #[arg(long)]
    editor_select: Option<String>,
    /// With `--editor`: the Settings window's Interface scale, 0.8 to 2.0
    /// (default 1.0) - how large the interface is drawn. The lever for a
    /// frame that is shrunk afterwards: 1280x720 at 1.5 lays the editor out
    /// on an 853x480 screen with every glyph half again as tall.
    #[arg(long)]
    editor_ui_scale: Option<f32>,
    /// With `--editor`: seed a window's rect the way a prefs file restores
    /// one, `<window>=x,y,w,h` in frame pixels, the window by its layout key
    /// (`world_editor`, `catalogue`, `inventory`, ...). Repeat the flag per
    /// window.
    #[arg(long, action = clap::ArgAction::Append)]
    editor_window: Vec<String>,
    /// With `--editor`: a script of gestures to play frame by frame on the
    /// tool's clock (#1353) - glides onto widgets found by their label,
    /// clicks, typing, a drag along a gizmo axis. The steps are documented
    /// in `render_tool::editor::script` and docs/building.md.
    #[arg(long)]
    editor_script: Option<String>,
    /// Single-camera shots: write the frame this many times smaller than it
    /// renders, each output pixel the mean of a block (default 1, full
    /// size). How an interface is laid out at the size it was designed for
    /// and still written as a small picture: 1280x720 with `--downscale 2`
    /// writes 640x360.
    #[arg(long, default_value_t = 1)]
    downscale: u32,
    /// With `--world`: rigged seeded bodies (u64 seeds, comma-separated or
    /// the flag repeated) walking across the world - from the gateway
    /// forecourt toward the spawn by default - on the engine's own gait.
    /// The first is the body `--focus walker` follows; the rest walk beside
    /// it, `--walker-spread` apart, so a clip can show a world with people
    /// in it (#1352).
    #[arg(long, value_delimiter = ',', action = clap::ArgAction::Append)]
    walker: Vec<u64>,
    /// With `--walker`: walking pace, metres per second (default 1.4).
    #[arg(long, default_value_t = 1.4)]
    walker_pace: f32,
    /// With `--walker`: where the walk starts, `x,z` (default: the record's
    /// landing).
    #[arg(long)]
    walk_from: Option<String>,
    /// With `--walker`: what the walk heads toward, `x,z` (default: the room
    /// origin). The body keeps walking along the line through both points.
    #[arg(long)]
    walk_to: Option<String>,
    /// With `--walker`: catalogue wearables to dress the body in, a
    /// comma-separated list of slugs (each must have a `wear_socket`).
    #[arg(long)]
    walker_wear: Option<String>,
    /// With `--walker`: seconds of walking before the first captured frame
    /// (default 1.5), so a clip opens mid-stride.
    #[arg(long, default_value_t = 1.5)]
    walker_lead: f32,
    /// With `--editor`: open the AVATAR editor on its Body tab rather than
    /// the World Editor (#1358). That tab hosts its sculpting sections from
    /// `bevy_symbios_avatar::editor`, so it is the surface that changes when
    /// the adapter is bumped and nothing in this repo moves - which is what
    /// makes it worth a headless shot after a take.
    #[arg(long)]
    editor_avatar: bool,
    /// Camera distance (m, from a body's root) at which a walker's hair swaps
    /// to the engine's far tier, overriding
    /// [`crate::config::camera::HAIR_SWITCH`] - the game's own value, which is
    /// what a `--walker` still shows without this (#1358). The band around it
    /// is NOT settable: a non-zero one quits every WebGL2 client.
    #[arg(long)]
    hair_switch: Option<f32>,
    /// With `--walker`: a body's outfit as the avatar editor's four axes,
    /// `top_hue,top_shade,leg_hue,leg_shade`, each 0..1 (#1351). Every
    /// seeded body ships in the engine's one default outfit - a reroll
    /// never touches it - so this is the only way to see a walker in
    /// anything else. Repeat the flag once per body, in `--walker` order;
    /// bodies past the last one keep the shipped default.
    #[arg(long, action = clap::ArgAction::Append)]
    walker_outfit: Vec<String>,
    /// With `--walker`: metres between neighbouring bodies across the line
    /// of walk (default 1.6). Companions take alternate sides of the lead
    /// and each hangs half a metre further back.
    #[arg(long, default_value_t = 1.6)]
    walker_spread: f32,
    /// Single subjects (a catalogue entry, a primitive, a generator, a
    /// wearable): the studio backdrop as a hex colour, `#rrggbb` or
    /// `rrggbb` (default the blue-grey `#8592b3`). A world and a room paint
    /// their own sky and ignore it.
    #[arg(long)]
    backdrop: Option<String>,
    /// Terrain subject (#994): a u64 seed or DID - builds the room's real
    /// heightmap and its four-layer splat, then shoots four grazing landscape
    /// views across `--view` metres of it.
    ///
    /// The one render mode whose subject is the *ground*. `--room` spawns
    /// settlement structures on a flat plane and skips terrain entirely, so
    /// until this existed nothing could see a splat outside the running game -
    /// which is why the tile-repetition defect went four rounds unjudged.
    /// It runs the game's own terrain systems (see
    /// `terrain::register_headless_terrain`), waits for the splat pass to
    /// resolve rather than a frame count, and frames a fixed camera so two
    /// renders are comparable.
    #[arg(long)]
    terrain: Option<String>,
    /// How many metres of ground a `--terrain` view spans (default 300).
    /// The repetition this mode exists to judge is a function of distance:
    /// one tile covers `world_extent / tile_scale` metres, so a 300 m view
    /// shows about 26 repeats at the shipped defaults.
    #[arg(long, default_value_t = 300.0)]
    view: f32,
    /// Room subject: a u64 seed or DID - renders the seeded settlement cluster.
    #[arg(long)]
    room: Option<String>,
    /// Road-graph diagnostics: a u64 seed or DID - reproduces the room's
    /// heightmap, builds the *meshed* road graph, and prints topology +
    /// geometry-risk stats (degree histogram, dead-end spurs, spurious-junction
    /// and spike-risk counts), then exits. A no-render dump to size road-network
    /// data filtering. Runs before any render app stands up.
    #[arg(long)]
    road_dump: Option<String>,
    /// Analytic entity census over seeded rooms (#810): for seeds `0..N`, sum
    /// every placement's instance count × generator-tree node count (the
    /// record-level estimate of what the compile will spawn) and print each
    /// seed's total + top contributors, then the worst seeds. Finds the
    /// seeds/generators that drive a region toward the `MAX_ROOM_ENTITIES`
    /// cap without a browser in the loop. A no-render mode.
    #[arg(long)]
    room_census: Option<u64>,
    /// Placement census over seeded rooms (#912): for seeds `0..N`, replay the
    /// real scatter sampling loop against a heightmap rebuilt from each record
    /// and print what it actually places - yield vs. requested count, what the
    /// slope cutoff costs, the per-instance scale spread, and a Clark–Evans
    /// nearest-neighbour index measuring how clustered the survivors are
    /// against the same scatter with its naturalness zeroed. Where
    /// `--room-census` answers "how many entities", this answers "how are they
    /// arranged". A no-render mode; a few seconds per seed (it rebuilds the
    /// heightmap).
    #[arg(long)]
    scatter_census: Option<u64>,
    /// Plan-view plot of one seeded room's scatters (#912): a u64 seed or DID.
    /// Writes a PNG grid to `--out` - one row per scatter, tuned arrangement
    /// on the left, the same scatter with its naturalness zeroed on the right.
    /// The four-angle contact sheet cannot show this: a stand is hundreds of
    /// metres across, so framed to fit every instance is a speck and the
    /// clustering is invisible. A no-render mode.
    #[arg(long)]
    scatter_plot: Option<String>,
    /// Concatenate PNG frame directories (comma-separated, each as
    /// `--keep-frames` writes them, or any equal-sized PNGs) into one GIF at
    /// `--out`, at `--fps`, and exit. How several clips - a world, a walker,
    /// a turntable - become one picture. A no-render mode.
    #[arg(long)]
    stitch: Option<String>,
    /// With `--stitch`: dissolve each cut between directories, and the
    /// loop seam from the last back to the first, over this many blended
    /// frames (default 0, a hard cut). Each blended frame is a whole-frame
    /// change, so it costs what a moving-camera frame costs.
    #[arg(long, default_value_t = 0)]
    crossfade: u32,
    /// Offline session-log post-mortem: read a captured session log
    /// (`diagnostics/session-latest.jsonl`, or the wasm "Download log" dump -
    /// same NDJSON format) and print an agent-facing report (header, `[Verdict]`,
    /// `[Event Tallies]`, `[Timeline]`, `[Loading Gate]`, `[Metric Trends]`,
    /// `[Invariant Violations]`), then exit. Narrow the analysis with the
    /// `--subsystem` / `--category` / `--severity` / `--since` / `--until`
    /// filters. A no-render analysis alongside `--road-dump`. Native-only.
    #[arg(long)]
    analyze_session: Option<String>,
    /// Offline before/after diff: read two captured session logs (A = baseline,
    /// B = candidate) and print a delta report (verdict / loading-gate timings /
    /// metric peaks / invariant fires) so an agent can confirm a fix in run B
    /// improved on the baseline A, then exit. `--diff-sessions <a> <b>`. A
    /// no-render analysis; runs after `--analyze-session`. Native-only.
    #[arg(long, num_args = 2, value_names = ["A", "B"])]
    diff_sessions: Option<Vec<String>>,
    /// `--analyze-session` filter: restrict the analysis sections to one
    /// subsystem (`loading`|`network`|`offload`|`runtime`|`session`). The header
    /// (session identity) is always shown in full.
    #[arg(long)]
    subsystem: Option<String>,
    /// `--analyze-session` filter: restrict to one event category
    /// (`lifecycle`|`fetch`|`generation`|`audio`|`peer`|… - see docs/diagnostics.md).
    #[arg(long)]
    category: Option<String>,
    /// `--analyze-session` filter: restrict to events at or above this severity
    /// (`trace`|`info`|`warn`|`error`|`critical`).
    #[arg(long)]
    severity: Option<String>,
    /// `--analyze-session` filter: restrict to events at or after this
    /// session-relative time (seconds).
    #[arg(long)]
    since: Option<f64>,
    /// `--analyze-session` filter: restrict to events at or before this
    /// session-relative time (seconds).
    #[arg(long)]
    until: Option<f64>,
    /// Torture/cut overrides for a `--prim` subject (for testing the prim
    /// system). `--shear x,z` · `--twist rad` · `--taper x,z` ·
    /// `--taperbottom x,z` · `--bulge x,z` · `--pathcut a,b` ·
    /// `--profilecut a,b` · `--hollow h`.
    #[arg(long)]
    shear: Option<String>,
    #[arg(long)]
    twist: Option<f32>,
    #[arg(long)]
    taper: Option<String>,
    #[arg(long)]
    taperbottom: Option<String>,
    #[arg(long)]
    bulge: Option<String>,
    #[arg(long)]
    pathcut: Option<String>,
    #[arg(long)]
    profilecut: Option<String>,
    #[arg(long)]
    hollow: Option<f32>,
    /// Camera elevation in degrees above the subject's centre. The sheet
    /// cameras' default orbit sits low (roughly 13°), which is the right
    /// eye-line for judging a facade or a silhouette but cannot see into
    /// anything open-topped - a brazier, a well, a crate, a bowl. Pass e.g.
    /// `--elev 45` to look down into it. For `--world` the default is 28°,
    /// the login backdrop's aerial angle.
    #[arg(long)]
    elev: Option<f32>,
    /// Clip only: the elevation the camera ends the clip at (a dolly; default
    /// `--elev`).
    #[arg(long)]
    elev_end: Option<f32>,
    /// Single-camera shots: what the rig orbits - `origin` (the spawn square;
    /// the `--world` default), `landing` (the gateway forecourt), `walker`
    /// (follow the `--walker` body), `subject` (the framed bounds; the
    /// turntable default), or a point `x,z` on the ground / `x,y,z`.
    #[arg(long)]
    focus: Option<String>,
    /// Single-camera shots: camera distance from the focus in metres
    /// (default: 150 for `--world`, the framed distance otherwise).
    #[arg(long)]
    dist: Option<f32>,
    /// Clip only: the distance the camera ends the clip at (a dolly; default
    /// `--dist`).
    #[arg(long)]
    dist_end: Option<f32>,
    /// Single-camera shots: metres above the focus point the camera looks at
    /// (default: 8 for a world point, 1 for the walker, 0 for a subject).
    #[arg(long)]
    lift: Option<f32>,
    /// Single-camera shots: camera yaw in degrees at the start (180 is the
    /// sheet cameras' "front"). For `--focus walker` it is measured from
    /// directly behind the body, so 0 follows it and 150 sees its face.
    #[arg(long)]
    yaw: Option<f32>,
    /// Clip only: degrees the yaw turns over the clip (default: 360 for a
    /// turntable, 30 for a world, 0 when following the walker).
    #[arg(long)]
    sweep: Option<f32>,
    /// Turntables: divide the auto-framed distance - 1.5 sits a third closer
    /// than the sheet cameras' fit, which is conservative (it fits the
    /// bounding sphere) and leaves a wide building small in a 16:9 frame.
    /// An explicit `--dist` is absolute and ignores this.
    #[arg(long, default_value_t = 1.0)]
    zoom: f32,
    /// Frames in the clip (default 1 - a still). Above 1 the single camera
    /// is moved along the rig one frame at a time and the result is a GIF;
    /// the clock advances exactly `1 / --fps` seconds a frame, so wind,
    /// clouds, water, particles and the walker's gait play at the rate the
    /// GIF does.
    #[arg(long, default_value_t = 1)]
    frames: u32,
    /// Clip frame rate (default 12.5). GIF holds delays in centiseconds, so
    /// 10, 12.5, 20 and 25 land exactly; others round.
    #[arg(long, default_value_t = DEFAULT_FPS)]
    fps: f32,
    /// Clip only: also write every frame as `<out>-frames/frame-NNN.png`.
    #[arg(long, default_value_t = false)]
    keep_frames: bool,
    /// GIF encoding (clips and `--stitch`): the ordered-dither amplitude in
    /// 8-bit steps (default 6). Higher smooths gradients and costs bytes -
    /// a dither pattern is what LZW cannot fold; 0 is a plain nearest-colour
    /// map and the smallest file.
    #[arg(long, default_value_t = gif::DEFAULT_DITHER)]
    dither: f32,
    /// Describe seeded rooms without rendering and exit: a u64 seed, a DID,
    /// or a range `a..b` - the scene roll (landform, biome, theme,
    /// prosperity, escalation), the atmosphere numbers that decide whether a
    /// shot can see anything (fog visibility, sun height, cloud cover), the
    /// water line, the landing, and the placement counts. The survey to run
    /// before `--world`, so a foggy seed is known to be foggy before a
    /// minute of compile says so with a green rectangle. A no-render mode.
    #[arg(long)]
    describe: Option<String>,
    /// Single-camera shots: frame width in pixels (default 896, or 1920 for
    /// `--play-view`; forced to a multiple of 64 so the GPU readback needs
    /// no row padding).
    #[arg(long)]
    width: Option<u32>,
    /// Single-camera shots: frame height in pixels (default 504, or 1080 for
    /// `--play-view`).
    #[arg(long)]
    height: Option<u32>,
    /// Sheets: per-tile pixel side. Forced to a multiple of 64 (no GPU row
    /// padding).
    #[arg(long, default_value_t = 512)]
    size: u32,
    /// Output path (defaults to `/tmp/avatar-render/<label>.png`, or
    /// `.gif` for a clip).
    #[arg(long)]
    out: Option<String>,
}

/// CLI entry point (called by the `render` bin).
pub fn run() {
    let args = Args::parse();

    // `--family-seeds <fam>`: print the first N seeds mapping to a chassis
    // family and exit - a survey aid, never renders.
    if let Some(fam) = &args.family_seeds {
        print_family_seeds(fam, args.family_count, args.craft.as_deref());
        return;
    }

    // `--outfit <seed|did>`: print one avatar's resolved outfit and exit.
    if let Some(subject) = &args.outfit {
        print_outfit(subject);
        return;
    }

    // `--find-part <slug>`: scan for seeds that roll a styled part and exit.
    if let Some(slug) = &args.find_part {
        find_part(slug, args.family_count);
        return;
    }

    // `--settlement-drop <seeds>`: measure real footprint drops and exit.
    if let Some(seeds) = args.settlement_drop {
        print_settlement_drop(seeds);
        return;
    }

    // `--foundation-audit [all]`: print the plinth-depth audit and exit.
    if let Some(mode) = &args.foundation_audit {
        print_foundation_audit(mode);
        return;
    }

    // `--gateway-fit <slug|all>`: print the veil-fit report and exit.
    if let Some(slug) = &args.gateway_fit {
        print_gateway_fit(slug);
        return;
    }

    // `--road-dump <seed|did>`: print the room's road-graph diagnostics and
    // exit - a no-render topology/geometry-risk dump for the road-filtering work.
    if let Some(room) = &args.road_dump {
        dump_road_graph(room);
        return;
    }

    // `--room-census <n>`: print seeded rooms' analytic entity estimates and
    // exit - the #810 density survey, never renders.
    if let Some(n) = args.room_census {
        room_census(n);
        return;
    }

    // `--scatter-census <n>`: replay the real sampling loop over seeded rooms
    // and print placement yield + arrangement - the #912 naturalness survey.
    if let Some(n) = args.scatter_census {
        scatter_census(n);
        return;
    }

    // `--scatter-plot <seed>`: write the plan-view PNG that shows what the
    // census's clustering number means.
    if let Some(room) = &args.scatter_plot {
        scatter_plot(
            room,
            std::path::Path::new(args.out.as_deref().unwrap_or("scatter-plot.png")),
        );
        return;
    }

    // `--describe <seed|did|a..b>`: print what a seeded room is before any
    // render app stands up.
    if let Some(what) = &args.describe {
        describe_rooms(what);
        return;
    }

    // `--stitch <dir,dir,...>`: PNG frame directories → one GIF and exit.
    if let Some(dirs) = &args.stitch {
        let dirs: Vec<String> = dirs.split(',').map(|d| d.trim().to_string()).collect();
        let out = args
            .out
            .clone()
            .unwrap_or_else(|| format!("{OUT_DIR}/stitch.gif"));
        if let Err(e) = gif::stitch(
            &dirs,
            &out,
            rig::delay_cs(args.fps),
            args.dither,
            args.crossfade,
        ) {
            eprintln!("--stitch failed: {e}");
            std::process::exit(1);
        }
        return;
    }

    // `--analyze-session <path>`: read a captured NDJSON session log, replay the
    // anomaly rules over it, and print an agent-facing post-mortem - a no-render
    // analysis, the offline counterpart to the live diagnostic engine.
    if let Some(path) = &args.analyze_session {
        analyze_session(&args, path);
        return;
    }

    // `--diff-sessions <a> <b>`: read two captured logs and print a before/after
    // delta report - the fix-validation counterpart to `--analyze-session`.
    if let Some(pair) = &args.diff_sessions {
        diff_sessions(&pair[0], &pair[1]);
        return;
    }

    // `--dump`: serialize the subject's generator to stdout (a valid
    // `--generator` seed) and exit before standing up the render app. Supports
    // a catalogue slug, a primitive tag (with the `--cut`/`--hollow`/…
    // overrides applied, #663), or an avatar seed/DID so any of them can
    // drive the fast no-recompile geometry loop.
    if args.dump {
        let g = if let Some(slug) = args.catalogue.as_deref() {
            crate::catalogue::by_slug(slug)
                .unwrap_or_else(|| panic!("unknown catalogue slug {slug:?}"))
                .build("did:render:tool")
        } else if let Some(tag) = args.prim.as_deref() {
            // Same construction as resolve_subject's --prim arm, so the
            // dumped JSON is exactly what a render of the same flags spawns.
            let mut kind =
                primitive_for_tag(tag).unwrap_or_else(|| panic!("unknown primitive tag {tag:?}"));
            apply_prim_overrides(&mut kind, &args);
            Generator::from_kind(kind)
        } else if let Some(avatar) = args.avatar.as_deref() {
            let body = match avatar.parse::<u64>() {
                Ok(seed) => build_in_livery(seed, args.livery).0,
                Err(_) => build_for_did_in_livery(avatar, args.livery).0,
            };
            generator_body(body, avatar)
        } else {
            panic!(
                "--dump requires --catalogue <slug>, --prim <tag>, or --avatar <seed|did> \
                 (--room/--world/--generator subjects are file/derived records - dump not supported)"
            );
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&g).expect("generator serialize")
        );
        return;
    }

    let Resolved {
        subject,
        label,
        ride,
    } = resolve_subject(&args);
    let (subject, label) = match &args.ages {
        Some(ages) => age_sweep(subject, &label, ages),
        None => (subject, label),
    };
    let frames = args.frames.max(1);
    let is_world = matches!(subject, Subject::World(_));
    let is_lineup = matches!(subject, Subject::Lineup(_));
    // Which subjects there is one camera to move along a rig: everything a
    // clip can be made of.
    let one_camera_subject = is_world
        || matches!(subject, Subject::Single(_) | Subject::Room(_))
        || (is_lineup && args.play_view);
    assert!(
        frames == 1 || one_camera_subject,
        "--frames needs a single-camera subject (--world, a --play-view line-up, or a \
         --generator/--prim/--catalogue/--avatar/--room turntable); --terrain, --wear \
         and --ages sheets have no one camera to move"
    );
    assert!(
        !args.play_view || matches!(subject, Subject::Single(_) | Subject::Lineup(_)),
        "--play-view frames a subject at the chase camera's range; --world, --terrain, \
         --room and --wear are not subjects it can stand on a ground plane"
    );
    assert!(
        args.ride_height.is_none() || args.play_view,
        "--ride-height only means anything under --play-view, which is the mode that \
         stands a subject on the ground"
    );
    let rig = build_rig(&args, is_world);
    let walker = (!args.walker.is_empty()).then(|| WalkerSpec {
        seeds: args.walker.clone(),
        pace: args.walker_pace,
        from: args.walk_from.as_deref().map(parse_xz),
        to: args.walk_to.as_deref().map(parse_xz),
        wear: args
            .walker_wear
            .as_deref()
            .map(|w| w.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default(),
        lead: args.walker_lead,
        outfits: args
            .walker_outfit
            .iter()
            .map(|o| parse_outfit(o).unwrap_or_else(|e| panic!("{e}")))
            .collect(),
        spread: args.walker_spread,
    });
    assert!(
        walker.is_none() || is_world,
        "--walker needs --world: the body walks the compiled terrain"
    );
    let single_camera = is_world || frames > 1 || args.play_view;
    let frame = if args.play_view {
        PLAY_FRAME
    } else {
        DEFAULT_FRAME
    };
    let tile = if single_camera {
        (
            (args.width.unwrap_or(frame.0) / 64).max(1) * 64,
            args.height.unwrap_or(frame.1).max(1),
        )
    } else {
        let side = (args.size / 64).max(1) * 64;
        (side, side)
    };
    // `--play-view`: one ride height per line-up slot - the derived one where
    // the subject had a locomotion record to read, and `--ride-height` where
    // it did not (or where it is being overridden). The frame is the one
    // actually rendered, rounding included, because the pixels-per-metre the
    // log reports is read off it.
    let play = args.play_view.then(|| {
        let slots = match &subject {
            Subject::Lineup(v) => v.len(),
            _ => 1,
        };
        let mut derived = ride;
        derived.resize(slots, None);
        PlayView {
            ride: resolve_rides(&derived, args.ride_height.as_deref()),
            frame: tile,
        }
    });
    let ext = if frames > 1 { "gif" } else { "png" };
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| format!("{OUT_DIR}/{label}.{ext}"));
    assert!(args.fps > 0.0, "--fps must be positive");

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>(),
    )
    .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO));
    // Resources + texture/material plugins the real spawn path reads.
    crate::world_builder::register_headless_spawn(&mut app);
    // `--terrain` (#994) drives the game's own terrain pipeline; everything it
    // needs beyond the spawn path lives here so the other modes pay nothing.
    if let Subject::Terrain { record, .. } = &subject {
        crate::terrain::register_headless_terrain(&mut app);
        app.init_resource::<crate::state::LocalSettings>()
            .init_resource::<crate::diagnostics::MetricsRegistry>()
            .insert_resource(crate::state::LiveRoomRecord((**record).clone()));
        // The capture signal is `terrain::SplatApplied`. Waiting a fixed
        // number of frames instead would race an async heightmap and four
        // texture bakes, and the frame it would catch is the flat placeholder
        // colour the material wears until the splat pass resolves - a render
        // that looks like a finished one and shows no ground texture at all.
    }
    // `--world`: the whole game pipeline, and the walker's spec.
    assert!(
        !args.editor || is_world,
        "--editor needs --world: the editor edits a compiled world"
    );
    assert!(
        args.editor
            || (args.editor_tab.is_none()
                && args.editor_select.is_none()
                && args.editor_ui_scale.is_none()
                && args.editor_window.is_empty()
                && args.editor_script.is_none()
                && !args.editor_avatar),
        "--editor-tab, --editor-select, --editor-ui-scale, --editor-window, --editor-script and \
         --editor-avatar need --editor"
    );
    assert!(
        !args.editor_avatar || args.editor_tab.is_none(),
        "--editor-avatar opens the AVATAR editor; --editor-tab names a World Editor tab"
    );
    assert!(
        args.downscale == 1
            || (single_camera
                && args.downscale > 1
                && tile.0.is_multiple_of(args.downscale)
                && tile.1.is_multiple_of(args.downscale)),
        "--downscale {} needs a single-camera shot whose --width and --height divide by it \
         (got {}×{})",
        args.downscale,
        tile.0,
        tile.1
    );
    if let Subject::World(spec) = &subject {
        world::register(&mut app, spec, walker);
        // `--editor` (#1353): the game's own editing surfaces over it.
        if args.editor {
            let opening = editor::EditorOpening {
                tab: args
                    .editor_tab
                    .as_deref()
                    .map(editor::parse_tab)
                    .transpose()
                    .unwrap_or_else(|e| panic!("{e}"))
                    .unwrap_or_default(),
                select: args.editor_select.clone(),
                ui_scale: args.editor_ui_scale,
                windows: args
                    .editor_window
                    .iter()
                    .map(|w| editor::parse_window(w).unwrap_or_else(|e| panic!("{e}")))
                    .collect(),
                avatar: args.editor_avatar,
            };
            let script = args.editor_script.as_deref().map(|path| {
                let source = std::fs::read_to_string(path)
                    .unwrap_or_else(|e| panic!("--editor-script {path:?}: {e}"));
                let script = editor::script::EditorScript::parse(&source)
                    .unwrap_or_else(|e| panic!("--editor-script {path:?}: {e}"));
                assert!(
                    frames > 1 || script.start == script.steps.len(),
                    "--editor-script {path:?}: a still captures one frame, so every step \
                     belongs before `start`"
                );
                script
            });
            editor::register(&mut app, &spec.record, &spec.did, opening, script);
        }
    }
    // Rigged bodies for `--wear` (#1088) and `--walker`: the engine's
    // spawn/pose/drive plugin (stateless, no game dependencies) and the
    // one-shot dressing system that parents worn props once the joints exist.
    app.add_plugins(bevy_symbios_avatar::AvatarPlugin);
    // The game's hair switch, so a `--walker` still is what the player sees at
    // that distance rather than what the adapter's default would show (#1358).
    // After the plugin, which `init_resource`s its own; `--hair-switch`
    // overrides it, which is how the two candidate switches were compared.
    app.insert_resource(bevy_symbios_avatar::HairLod {
        switch: args
            .hair_switch
            .unwrap_or(crate::config::camera::HAIR_SWITCH),
        margin: crate::config::camera::HAIR_MARGIN,
    });
    app.add_systems(Update, headless::dress_wear_bodies);
    let [br, bg, bb] = args
        .backdrop
        .as_deref()
        .map(|b| parse_hex_colour(b).unwrap_or_else(|e| panic!("{e}")))
        .unwrap_or(DEFAULT_BACKDROP);
    app.insert_resource(ClearColor(Color::srgb_u8(br, bg, bb)))
        .insert_resource(RenderJob {
            subject,
            play,
            out,
            tile,
            elev: args.elev,
            rig,
            frames,
            fps: args.fps,
            keep_frames: args.keep_frames,
            dither: args.dither,
            downscale: args.downscale,
        })
        .insert_resource(Clock {
            step: 1.0 / args.fps,
            run: true,
            once: false,
            stepped: false,
            elapsed: 0.0,
        })
        .init_resource::<Capture>()
        // The hand-driven clock (see `headless`): `Time<Virtual>` is
        // paused, and `tick_clock` advances it right after Bevy's own time
        // system has published the paused (zero-delta) frame.
        .add_systems(Startup, (setup, headless::pause_virtual_time))
        .add_systems(First, headless::tick_clock.after(TimeSystems))
        // The walker moves, then the camera follows it, and both land before
        // the avatar plugin poses the body for this frame.
        .add_systems(
            Update,
            (world::step_walkers, drive)
                .chain()
                .before(AvatarSystems::Animate),
        )
        .add_systems(
            Update,
            world::spawn_walker.run_if(
                resource_exists::<WalkerSpec>.and_then(resource_exists::<headless::ClipTiming>),
            ),
        )
        // Particle emitters spawn through the shared dispatch arm, but the
        // systems that make them *emit* are registered by `WorldBuilderPlugin`
        // behind `in_state(AppState::InGame)` - a state the render app never
        // enters. Without these an FX-bearing prop renders as its bare
        // geometry, which is exactly the detail an FX review needs to see.
        // The particle integrator reads avian's gravity vector and takes a
        // `SpatialQuery`, and the render app runs no physics plugin to
        // supply either. An empty collider tree is the honest state here -
        // catalogue FX all run with `collide_colliders: false`, so nothing
        // queries it.
        .insert_resource(avian3d::prelude::Gravity::default())
        .init_resource::<avian3d::collider_tree::ColliderTrees>()
        .add_systems(
            Update,
            (
                crate::world_builder::particles::update_emitter_motion,
                crate::world_builder::particles::tick_emitter_spawn,
                crate::world_builder::particles::tick_particles,
            )
                .chain(),
        )
        .add_observer(headless::on_capture)
        .run();
}

/// The camera rig from the `--focus` / `--dist` / `--elev` / `--yaw` /
/// `--sweep` / `--lift` flags, with the mode's defaults filled in: a world
/// orbits its spawn from 150 m at 28° and drifts 30° over a clip; a
/// turntable orbits its subject at the framed distance and turns once;
/// following the walker holds the angle.
fn build_rig(args: &Args, is_world: bool) -> CameraRig {
    let focus = match &args.focus {
        Some(f) => Focus::parse(f).unwrap_or_else(|e| panic!("{e}")),
        None if is_world => Focus::Origin,
        None => Focus::Subject,
    };
    // `--play-view` leaves distance and elevation unset so the framing can
    // hand the rig the game's own pair (`rig::PLAY_DIST` / `play_elev_deg`),
    // and holds the angle: it is a shot of a craft standing still at the
    // range the player sees it, not a turntable.
    if args.play_view {
        return CameraRig {
            focus,
            lift: args.lift.unwrap_or(0.0),
            dist: args.dist.map(|d| (d, args.dist_end.unwrap_or(d))),
            elev: args.elev.map(|e| (e, args.elev_end.unwrap_or(e))),
            // The sheet's own three-quarter angle, not its head-on "front":
            // a craft seen straight on shows neither its sheer nor its
            // length, which between them are most of what is being judged,
            // and the studio sun is on this side.
            yaw: args.yaw.unwrap_or(ANGLES[1]),
            sweep: args.sweep.unwrap_or(0.0),
            zoom: args.zoom.max(0.01),
        };
    }
    // A vista focus (the spawn, the landing, the settlement) looks at the
    // built-up band, 8 m up; a point on the ground and the walker are
    // deliberate ground-level shots, and a subject is framed on its centre.
    let lift = args.lift.unwrap_or(match focus {
        Focus::Walker | Focus::Point { .. } => 1.0,
        Focus::Subject => 0.0,
        _ if is_world => 8.0,
        _ => 0.0,
    });
    let dist = args
        .dist
        .map(|d| (d, args.dist_end.unwrap_or(d)))
        .or(is_world.then_some((150.0, 150.0)));
    let elev = args
        .elev
        .map(|e| (e, args.elev_end.unwrap_or(e)))
        .or(is_world.then_some((28.0, 28.0)));
    let yaw = args.yaw.unwrap_or(match focus {
        Focus::Walker => 30.0,
        _ if is_world => 0.0,
        _ => 180.0,
    });
    // A vista drifts; a deliberate shot (the walker, a named point) holds.
    let sweep = args.sweep.unwrap_or(match focus {
        Focus::Walker | Focus::Point { .. } => 0.0,
        _ if is_world => 30.0,
        _ => 360.0,
    });
    CameraRig {
        focus,
        lift,
        dist,
        elev,
        yaw,
        sweep,
        zoom: args.zoom.max(0.01),
    }
}

/// The studio clear colour behind a single subject: the blue-grey the tool
/// has always used, as `--backdrop` would spell it (`#8592b3`).
const DEFAULT_BACKDROP: [u8; 3] = [0x85, 0x92, 0xb3];

/// Parse `--backdrop`: `#rrggbb` or `rrggbb`.
fn parse_hex_colour(s: &str) -> Result<[u8; 3], String> {
    let hex = s.trim().trim_start_matches('#');
    let bad = || format!("--backdrop {s:?}: expected a hex colour, #rrggbb or rrggbb");
    if hex.len() != 6 {
        return Err(bad());
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
    Ok([channel(0)?, channel(2)?, channel(4)?])
}

/// Parse `--walker-outfit`: four comma-separated axes in `0..=1`, in the
/// avatar editor's order - top hue, top shade, leg hue, leg shade.
fn parse_outfit(s: &str) -> Result<[f32; 4], String> {
    let bad = || {
        format!("--walker-outfit {s:?}: expected top_hue,top_shade,leg_hue,leg_shade, each 0..1")
    };
    let v: Vec<f32> = s
        .split(',')
        .map(|p| p.trim().parse::<f32>().map_err(|_| bad()))
        .collect::<Result<_, _>>()?;
    match v.as_slice() {
        [a, b, c, d] if v.iter().all(|x| (0.0..=1.0).contains(x)) => Ok([*a, *b, *c, *d]),
        _ => Err(bad()),
    }
}

/// Where each play-view slot's origin goes: the height derived from the
/// subject's own locomotion record, overridden by `--ride-height`.
///
/// One bare value in the spec sets every slot (and a bare `auto` derives
/// every slot, which is what passing nothing already does); a comma list
/// sets them one for one, with `auto` (or an empty entry) leaving a slot's
/// derived height alone. The list has to match the line-up exactly - a short list would
/// silently leave the last subject standing somewhere nobody asked for, and
/// the whole point of the view is that nothing in it is accidental.
fn resolve_rides(derived: &[Option<f32>], spec: Option<&str>) -> Vec<Ride> {
    let told = |entry: &str| {
        entry
            .parse::<f32>()
            .unwrap_or_else(|e| panic!("--ride-height {entry:?}: {e}"))
    };
    let auto = |h: Option<f32>| h.map_or(Ride::Bounds, Ride::Derived);
    let Some(spec) = spec else {
        return derived.iter().copied().map(auto).collect();
    };
    let entries: Vec<&str> = spec.split(',').map(str::trim).collect();
    if let [only] = entries.as_slice() {
        // A single `auto` is "derive every slot", which is what passing no
        // flag at all already means; a single number sets them all.
        return match *only {
            "auto" | "" => derived.iter().copied().map(auto).collect(),
            v => {
                let h = told(v);
                derived.iter().map(|_| Ride::Told(h)).collect()
            }
        };
    }
    assert_eq!(
        entries.len(),
        derived.len(),
        "--ride-height {spec:?}: {} values for {} line-up slot(s) - pass one value for \
         every slot, `auto` for the ones that keep their derived height, or a single \
         value for all of them",
        entries.len(),
        derived.len()
    );
    derived
        .iter()
        .zip(entries)
        .map(|(&h, entry)| match entry {
            "auto" | "" => auto(h),
            v => Ride::Told(told(v)),
        })
        .collect()
}

/// Parse an `x,z` ground point.
fn parse_xz(s: &str) -> [f32; 2] {
    let v: Vec<f32> = s
        .split(',')
        .map(|p| {
            p.trim()
                .parse::<f32>()
                .unwrap_or_else(|e| panic!("bad x,z component {p:?}: {e}"))
        })
        .collect();
    match v.as_slice() {
        [x, z] => [*x, *z],
        _ => panic!("expected x,z - got {s:?}"),
    }
}

/// What a subject resolved to: the thing to draw, the filename label, and -
/// for `--play-view` - where the game rests each line-up slot's origin above
/// flat ground.
struct Resolved {
    subject: Subject,
    label: String,
    /// One entry per line-up slot (one for a single subject). `None` means
    /// there is no locomotion record to read a ride height off, so the play
    /// view stands that slot on its own drawn bounds.
    ride: Vec<Option<f32>>,
}

impl Resolved {
    /// A subject with no ride height to derive - everything but an avatar.
    fn plain(subject: Subject, label: String) -> Self {
        Self {
            subject,
            label,
            ride: vec![None],
        }
    }
}

/// One line-up slot's tree, its derived ride height and its label.
struct Slot {
    generator: Generator,
    ride: Option<f32>,
    label: String,
}

/// Resolve a `--lineup` entry: a `u64` seed or a DID resolves to that seeded
/// avatar through [`seeded_slot`], and a path to a readable file is a
/// `--generator` JSON, which carries no locomotion record at all and so has
/// no ride height to derive (`--ride-height` is how a prototype is told).
///
/// A seed is tried first, then a file, then a DID. Something that *looks*
/// like a path - it has a separator or a `.json` tail - but is not readable
/// is an error rather than a DID, because the alternative is a confusing
/// "no such DID" for a mistyped filename.
fn lineup_slot(spec: &str, livery: Option<usize>) -> Slot {
    if spec.parse::<u64>().is_err() {
        let path = std::path::Path::new(spec);
        if path.is_file() {
            let json = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("read generator {spec:?}: {e}"));
            let generator: Generator = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("parse generator {spec:?}: {e}"));
            let label = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("generator")
                .to_string();
            return Slot {
                generator,
                ride: None,
                label: format!("gen-{label}"),
            };
        }
        assert!(
            !(spec.contains(['/', '\\']) || spec.ends_with(".json")),
            "--lineup {spec:?}: looks like a file path, but nothing readable is there"
        );
    }
    seeded_slot(spec, livery)
}

/// A seeded avatar - `u64` seed or DID - as a line-up slot, with the ride
/// height read off the locomotion the same build produced. Vehicle seeds
/// only: a rigged humanoid is refused by [`generator_body`], which is why
/// `--reference-figure` exists.
fn seeded_slot(spec: &str, livery: Option<usize>) -> Slot {
    let (body, loco, label) = match spec.parse::<u64>() {
        Ok(seed) => {
            let (body, loco) = build_in_livery(seed, livery);
            (body, loco, format!("seed-{seed}"))
        }
        Err(_) => {
            let (body, loco) = build_for_did_in_livery(spec, livery);
            (body, loco, spec.replace([':', '/'], "_"))
        }
    };
    Slot {
        generator: generator_body(body, spec),
        ride: crate::pds::avatar::default_visuals::ground_ride_height(&loco),
        label,
    }
}

/// Build the subject + a filename label from the CLI args.
///
/// Precedence: `--lineup` → `--generator` → `--world` → `--terrain` →
/// `--room` → `--prim` → `--wear` → `--catalogue` → `--avatar` → seed 7.
/// Pinned by `tests::the_subject_precedence_is_the_one_the_docs_claim`,
/// because this order is stated in four places and three of them had drifted
/// (#1162).
fn resolve_subject(args: &Args) -> Resolved {
    if let Some(entries) = &args.lineup {
        let slots: Vec<Slot> = entries
            .split(',')
            .map(|e| lineup_slot(e.trim(), args.livery))
            .chain(args.reference_figure.then(|| Slot {
                generator: figure::reference_figure(),
                ride: None,
                label: format!("figure-{:.2}m", figure::HEIGHT),
            }))
            .collect();
        assert!(!slots.is_empty(), "--lineup needs at least one subject");
        println!(
            "line-up, left→right: {}",
            slots
                .iter()
                .map(|s| s.label.as_str())
                .collect::<Vec<_>>()
                .join(" | ")
        );
        let label = format!("lineup-{}", slots.len());
        let ride = slots.iter().map(|s| s.ride).collect();
        return Resolved {
            subject: Subject::Lineup(slots.into_iter().map(|s| s.generator).collect()),
            label,
            ride,
        };
    }
    if let Some(path) = &args.generator {
        let json = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read generator {path:?}: {e}"));
        let generator: Generator =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("parse generator {path:?}: {e}"));
        let label = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("generator")
            .to_string();
        return Resolved::plain(Subject::Single(Box::new(generator)), format!("gen-{label}"));
    }
    if let Some(world) = &args.world {
        let (record, did) = match world.parse::<u64>() {
            Ok(seed) => {
                let did = format!("did:render:{seed}");
                (RoomRecord::default_for_seed(seed, &did), did)
            }
            Err(_) => (RoomRecord::default_for_did(world), world.clone()),
        };
        let label = format!("world-{}", world.replace([':', '/'], "_"));
        return Resolved::plain(Subject::World(Box::new(WorldSpec { record, did })), label);
    }
    if let Some(terrain) = &args.terrain {
        let record = match terrain.parse::<u64>() {
            Ok(seed) => RoomRecord::default_for_seed(seed, &format!("did:render:{seed}")),
            Err(_) => RoomRecord::default_for_did(terrain),
        };
        let label = format!(
            "terrain-{}-{:.0}m",
            terrain.replace([':', '/'], "_"),
            args.view
        );
        return Resolved::plain(
            Subject::Terrain {
                record: Box::new(record),
                view_m: args.view.max(10.0),
            },
            label,
        );
    }
    if let Some(room) = &args.room {
        let record = match room.parse::<u64>() {
            Ok(seed) => RoomRecord::default_for_seed(seed, &format!("did:render:{seed}")),
            Err(_) => RoomRecord::default_for_did(room),
        };
        let label = format!("room-{}", room.replace([':', '/'], "_"));
        return Resolved::plain(Subject::Room(Box::new(record)), label);
    }
    if let Some(tag) = &args.prim {
        let mut kind =
            primitive_for_tag(tag).unwrap_or_else(|| panic!("unknown primitive tag {tag:?}"));
        apply_prim_overrides(&mut kind, args);
        return Resolved::plain(
            Subject::Single(Box::new(Generator::from_kind(kind))),
            format!("prim-{}", tag.to_lowercase()),
        );
    }
    if let Some(slug) = &args.wear {
        let entry = crate::catalogue::by_slug(slug)
            .unwrap_or_else(|| panic!("--wear {slug:?}: no catalogue entry with that slug"));
        let socket = match &args.wear_socket {
            Some(name) => symbios_avatar::Socket::from_name(name)
                .unwrap_or_else(|| panic!("--wear-socket {name:?}: not an engine socket name")),
            None => entry.wear_socket().unwrap_or_else(|| {
                panic!(
                    "--wear {slug:?}: entry is not wearable (no wear_socket()) - \
                     pass --wear-socket to force one"
                )
            }),
        };
        assert!(args.wear_bodies > 0, "--wear-bodies must be at least 1");
        let seeds = (0..args.wear_bodies as u64).collect();
        return Resolved::plain(
            Subject::Wear {
                seeds,
                item: Box::new(entry.build("did:render:wear")),
                socket,
                fit: entry.wear_fit(),
            },
            format!("wear-{slug}-{}", socket.name()),
        );
    }
    if let Some(slug) = &args.catalogue {
        let entry = crate::catalogue::by_slug(slug)
            .unwrap_or_else(|| panic!("unknown catalogue slug {slug:?}"));
        let mut generator = entry.build("did:render:tool");
        let mut label = format!("cat-{slug}");
        if let Some(variant) = &args.variant {
            if variant == "list" {
                println!("{slug} variants:");
                for v in entry.variants() {
                    println!("  {:<16} {}", v.name, v.label);
                }
                if entry.variants().is_empty() {
                    println!("  (none - this entry has no material re-skins)");
                }
                std::process::exit(0);
            }
            if let GeneratorKind::LSystem { materials, .. } = &mut generator.kind {
                crate::catalogue::items::plants::variant::apply_named(
                    entry.variants(),
                    variant,
                    materials,
                );
            }
            label.push_str(&format!("-{variant}"));
        }
        return Resolved::plain(Subject::Single(Box::new(generator)), label);
    }
    let avatar = args.avatar.clone().unwrap_or_else(|| "7".to_string());
    let slot = seeded_slot(&avatar, args.livery);
    // `--reference-figure` without `--lineup`: the subject plus the ruler is
    // a two-slot line-up, which is the same picture with fewer flags.
    if args.reference_figure {
        println!(
            "line-up, left→right: {} | figure-{:.2}m",
            slot.label,
            figure::HEIGHT
        );
        let label = format!("{}-figure", slot.label);
        return Resolved {
            subject: Subject::Lineup(vec![slot.generator, figure::reference_figure()]),
            label,
            ride: vec![slot.ride, None],
        };
    }
    Resolved {
        subject: Subject::Single(Box::new(slot.generator)),
        label: slot.label,
        ride: vec![slot.ride],
    }
}

/// The generator tree behind a seeded avatar, or a clear refusal.
///
/// This tool draws `Generator` geometry through the real spawn path; a
/// rigged body (#1060 - every humanoid seed) is a skinned
/// `symbios-avatar` build with no tree to walk, so it is turned away by
/// name rather than rendered as an empty sheet. The sibling
/// `bevy_symbios_avatar` viewer is that body's instrument, and it has its
/// own `--shot` capture.
fn generator_body(body: AvatarBody, subject: &str) -> Generator {
    match body {
        AvatarBody::Generator(body) => body.visuals,
        _ => panic!(
            "avatar {subject:?} is a rigged body - this tool renders generator trees. \n\
             Vehicle seeds (boat / airship / skiff) still render here; for a rigged \n\
             body use the bevy_symbios_avatar viewer's --shot capture."
        ),
    }
}

/// Expand a single-generator subject into the `--ages` lineup: one clone per
/// iteration count, ready for the grid contact sheet (rows top→bottom follow
/// the argument order). Panics on `--room` subjects and on generator trees
/// without an L-system node - an age sweep of those is meaningless.
fn age_sweep(subject: Subject, label: &str, ages: &str) -> (Subject, String) {
    let Subject::Single(base) = subject else {
        panic!(
            "--ages needs a single-generator subject \
             (--generator/--prim/--catalogue/--avatar); --room, --world, \
             --terrain and --wear resolve to whole scenes and have no single \
             tree to age"
        );
    };
    let ages: Vec<u32> = ages
        .split(',')
        .map(|a| {
            a.trim()
                .parse::<u32>()
                .unwrap_or_else(|e| panic!("bad --ages entry {a:?}: {e}"))
        })
        .collect();
    assert!(
        !ages.is_empty(),
        "--ages needs at least one iteration count"
    );
    let variants: Vec<Generator> = ages
        .iter()
        .map(|&n| {
            let mut g = (*base).clone();
            assert!(
                override_lsystem_iterations(&mut g, n),
                "--ages: subject has no L-system node to sweep"
            );
            g
        })
        .collect();
    println!("age sweep rows, top→bottom: {ages:?} iterations");
    (Subject::Lineup(variants), format!("{label}-ages"))
}

/// Set `iterations` on every L-system node in the tree; returns whether any
/// node was hit.
fn override_lsystem_iterations(g: &mut Generator, iterations: u32) -> bool {
    let mut hit = false;
    if let GeneratorKind::LSystem { iterations: it, .. } = &mut g.kind {
        *it = iterations;
        hit = true;
    }
    for child in &mut g.children {
        hit |= override_lsystem_iterations(child, iterations);
    }
    hit
}

/// Parse a `"a,b"` pair into `[f32; 2]` (missing components default to 0).
fn parse2(s: &str) -> [f32; 2] {
    let mut it = s.split(',').map(|x| x.trim().parse::<f32>().unwrap_or(0.0));
    [it.next().unwrap_or(0.0), it.next().unwrap_or(0.0)]
}

/// Apply the CLI torture/cut overrides to a `--prim` subject for testing.
fn apply_prim_overrides(kind: &mut GeneratorKind, args: &Args) {
    let Some(t) = kind.torture_mut() else {
        return;
    };
    if let Some(s) = &args.shear {
        t.shear = Fp2(parse2(s));
    }
    if let Some(v) = args.twist {
        t.twist = Fp(v);
    }
    if let Some(s) = &args.taper {
        t.taper = Fp2(parse2(s));
    }
    if let Some(s) = &args.taperbottom {
        t.taper_bottom = Fp2(parse2(s));
    }
    if let Some(s) = &args.bulge {
        t.bulge = Fp2(parse2(s));
    }
    if let Some(s) = &args.pathcut {
        t.path_cut = Fp2(parse2(s));
    }
    if let Some(s) = &args.profilecut {
        t.profile_cut = Fp2(parse2(s));
    }
    if let Some(h) = args.hollow {
        t.hollow = Fp(h);
    }
}

/// Resolve a primitive tag (case-insensitive) to a default kind. Wraps
/// [`GeneratorKind::default_primitive_for_tag`], which is title-cased.
fn primitive_for_tag(tag: &str) -> Option<GeneratorKind> {
    let mut chars = tag.chars();
    let titled: String = {
        let first = chars.next()?;
        first.to_uppercase().collect::<String>() + chars.as_str()
    };
    GeneratorKind::default_primitive_for_tag(&titled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backdrop_is_a_hex_colour_with_or_without_the_hash() {
        assert_eq!(parse_hex_colour("#8592b3").unwrap(), DEFAULT_BACKDROP);
        assert_eq!(parse_hex_colour(" DCD6C8 ").unwrap(), [0xdc, 0xd6, 0xc8]);
        for bad in ["#fff", "8592b3ff", "#85g2b3", "grey"] {
            let err = parse_hex_colour(bad).unwrap_err();
            assert!(err.contains("--backdrop"), "{bad}: {err}");
        }
    }

    #[test]
    fn a_walker_outfit_is_four_unit_axes_in_the_editor_order() {
        assert_eq!(
            parse_outfit("0.6, 0.45,0.1,0.25").unwrap(),
            [0.6, 0.45, 0.1, 0.25]
        );
        for bad in ["0.6,0.45,0.1", "0.6,0.45,0.1,1.5", "blue,0.4,0.1,0.2", ""] {
            let err = parse_outfit(bad).unwrap_err();
            assert!(err.contains("--walker-outfit"), "{bad:?}: {err}");
        }
    }

    /// #1162. The subject precedence is stated in four places - this module's
    /// header, [`resolve_subject`]'s own doc, the `generator` arg doc and
    /// docs/building.md - and three of them omitted `--wear` from the day it
    /// shipped (#1088), so `--wear satchel --catalogue villa` did the thing
    /// the documentation said could not happen.
    ///
    /// This does not fail against the old behaviour, because the old
    /// behaviour was right and the prose was wrong. What it does is make the
    /// order a fact rather than a claim: reorder the chain and this fails by
    /// name, pointing at the four sentences that then need re-reading. A
    /// 0-warning doc gate cannot do that - it checks that links resolve, not
    /// that sentences are true.
    #[test]
    fn the_subject_precedence_is_the_one_the_docs_claim() {
        fn label_for(argv: &[&str]) -> String {
            let args = Args::parse_from(std::iter::once("render").chain(argv.iter().copied()));
            resolve_subject(&args).label
        }

        // A wearable to argue over, taken from the catalogue rather than
        // hardcoded, so retiring one entry does not silently gut this test.
        let worn = crate::catalogue::items::ENTRIES
            .iter()
            .find(|e| e.wear_socket().is_some())
            .expect("the catalogue ships at least one wearable");
        let wear = worn.slug();

        assert!(
            label_for(&["--wear", wear]).starts_with("wear-"),
            "--wear alone renders the wear sheet"
        );
        // The loser has to be a real, reachable arm, or "wear wins" would be
        // true of a slug that resolves to nothing.
        assert_eq!(
            label_for(&["--catalogue", "villa"]),
            "cat-villa",
            "the catalogue arm this test outranks must itself resolve"
        );
        assert!(
            label_for(&["--wear", wear, "--catalogue", "villa"]).starts_with("wear-"),
            "--wear outranks --catalogue"
        );
        assert!(
            label_for(&["--prim", "cuboid", "--wear", wear]).starts_with("prim-"),
            "--prim outranks --wear"
        );
        assert!(
            label_for(&["--room", "3", "--prim", "cuboid", "--wear", wear]).starts_with("room-"),
            "--room outranks both"
        );
        // `--terrain` above `--room` (#994): the two take the same argument
        // and build the same record, and the one that renders the ground has
        // to be reachable when both are given.
        assert!(
            label_for(&["--terrain", "3", "--room", "3", "--prim", "cuboid"])
                .starts_with("terrain-"),
            "--terrain outranks --room"
        );
        assert_eq!(
            label_for(&["--terrain", "3", "--view", "250"]),
            "terrain-3-250m",
            "a terrain label carries its view distance, so two views do not \
             overwrite one file"
        );
        // `--world` above `--terrain`: same argument, same record, and the
        // one that compiles the whole room has to win when both are given.
        assert!(
            label_for(&["--world", "3", "--terrain", "3", "--room", "3"]).starts_with("world-"),
            "--world outranks --terrain"
        );
        // `--lineup` above everything (#1360): it is the only flag that
        // names several subjects, so a subject flag beside it is what the
        // line-up is being compared *against*, not a competing request.
        assert_eq!(
            label_for(&[
                "--lineup",
                "12,40,7",
                "--world",
                "3",
                "--catalogue",
                "villa"
            ]),
            "lineup-3",
            "--lineup outranks --world"
        );
        assert_eq!(
            label_for(&["--lineup", "12", "--reference-figure"]),
            "lineup-2",
            "--reference-figure joins the line-up as its last slot"
        );
        assert_eq!(
            label_for(&["--avatar", "40", "--reference-figure"]),
            "seed-40-figure",
            "--reference-figure alone makes a two-slot line-up of the subject and the ruler"
        );
    }

    /// #1360. `--play-view` is a preset, so the only thing worth pinning
    /// here is that it *is* one: the distance and elevation stay unset so
    /// the framing can hand over the game's own pair, and the shot holds its
    /// angle instead of turning like a turntable. The numbers themselves are
    /// pinned against `config::camera` in `rig`.
    #[test]
    fn the_play_view_preset_leaves_the_game_numbers_to_the_framing() {
        let parse =
            |argv: &[&str]| Args::parse_from(std::iter::once("render").chain(argv.iter().copied()));
        let play = build_rig(&parse(&["--avatar", "40", "--play-view"]), false);
        assert_eq!(play.focus, Focus::Subject);
        assert_eq!(play.dist, None, "the framing supplies the game's distance");
        assert_eq!(play.elev, None, "and the game's pitch");
        assert_eq!(play.sweep, 0.0, "a play view holds its angle");
        assert_eq!(play.lift, 0.0, "the framing looks at the line-up's middle");
        assert_eq!(play.yaw, ANGLES[1], "the sheet's three-quarter angle");
        // ... and every one of them is still overridable, because the view
        // is also the most convenient studio the tool has.
        let nudged = build_rig(
            &parse(&[
                "--avatar",
                "40",
                "--play-view",
                "--yaw",
                "200",
                "--dist",
                "6",
            ]),
            false,
        );
        assert_eq!(nudged.yaw, 200.0);
        assert_eq!(nudged.dist, Some((6.0, 6.0)));
    }

    /// #1360. `--ride-height` over the derived heights: one value for all,
    /// a list one for one, `auto` to keep what was derived - and a list that
    /// does not match the line-up is refused rather than padded, because a
    /// padded one would stand the last subject somewhere nobody asked for.
    #[test]
    fn a_told_ride_height_overrides_a_derived_one_slot_for_slot() {
        let derived = [Some(1.17), None, Some(0.83)];
        let heights = |spec: Option<&str>| -> Vec<Option<f32>> {
            resolve_rides(&derived, spec)
                .iter()
                .map(Ride::height)
                .collect()
        };
        assert_eq!(heights(None), vec![Some(1.17), None, Some(0.83)]);
        assert_eq!(heights(Some("0.4")), vec![Some(0.4); 3]);
        assert_eq!(
            heights(Some("auto, 0.35 ,auto")),
            vec![Some(1.17), Some(0.35), Some(0.83)]
        );
        // `auto` alone is not a single-value override; it is "derive them
        // all", which is what no flag already means.
        assert_eq!(heights(Some("auto")), heights(None));
        // Provenance survives the override, so the log can say whose answer
        // a subject is standing on.
        let rides = resolve_rides(&derived, Some("auto,0.35,auto"));
        assert!(matches!(rides[0], Ride::Derived(_)));
        assert!(matches!(rides[1], Ride::Told(_)));
    }

    #[test]
    #[should_panic(expected = "2 values for 3 line-up slot(s)")]
    fn a_short_ride_height_list_is_refused_rather_than_padded() {
        resolve_rides(&[Some(1.0), None, None], Some("0.4,0.5"));
    }

    /// #1360. A line-up entry is a seed, a readable file, or a DID - and a
    /// mistyped path is told it is a mistyped path rather than sent off to
    /// resolve as a DID.
    #[test]
    fn a_lineup_entry_is_a_seed_a_file_or_a_did() {
        assert_eq!(lineup_slot("40", None).label, "seed-40");
        assert!(
            lineup_slot("40", None).ride.is_some(),
            "a seeded boat knows its own ride height"
        );
        let did = "did:render:lineup-test";
        // Not every DID rolls a vehicle - walk until one does, since a
        // rigged humanoid is refused by name.
        let vehicle = (0..64)
            .map(|i| format!("{did}-{i}"))
            .find(|d| {
                crate::seeded_defaults::ChassisFamily::for_did(d)
                    != crate::seeded_defaults::ChassisFamily::Humanoid
            })
            .expect("one of 64 DIDs rolls a vehicle");
        assert_eq!(
            lineup_slot(&vehicle, None).label,
            vehicle.replace([':', '/'], "_")
        );
    }

    #[test]
    #[should_panic(expected = "looks like a file path")]
    fn a_mistyped_lineup_path_is_not_taken_for_a_did() {
        lineup_slot("target/dump/no-such-prototype.json", None);
    }

    /// The rig defaults per mode, so a bare `--world` and a bare turntable
    /// each land on a sensible orbit without flags.
    #[test]
    fn the_rig_defaults_follow_the_mode() {
        let parse =
            |argv: &[&str]| Args::parse_from(std::iter::once("render").chain(argv.iter().copied()));
        let world = build_rig(&parse(&["--world", "3"]), true);
        assert_eq!(world.focus, Focus::Origin);
        assert_eq!(world.dist, Some((150.0, 150.0)));
        assert_eq!(world.elev, Some((28.0, 28.0)));
        assert_eq!(world.sweep, 30.0);
        assert_eq!(world.lift, 8.0);

        let turntable = build_rig(&parse(&["--catalogue", "villa", "--frames", "12"]), false);
        assert_eq!(turntable.focus, Focus::Subject);
        assert_eq!(turntable.dist, None, "the framing supplies the distance");
        assert_eq!(turntable.elev, None);
        assert_eq!(turntable.yaw, 180.0, "starts on the front tile's angle");
        assert_eq!(turntable.sweep, 360.0);
        assert_eq!(turntable.zoom, 1.0);

        let follow = build_rig(
            &parse(&[
                "--world",
                "3",
                "--walker",
                "7",
                "--focus",
                "walker",
                "--elev",
                "12",
                "--elev-end",
                "20",
            ]),
            true,
        );
        assert_eq!(follow.focus, Focus::Walker);
        assert_eq!(follow.sweep, 0.0);
        assert_eq!(follow.lift, 1.0);
        assert_eq!(follow.elev, Some((12.0, 20.0)), "a dolly runs start→end");
        assert_eq!(parse_xz(" 3.5, -2 "), [3.5, -2.0]);

        // A ground point is a deliberate shot: eye height, no drift.
        let point = build_rig(&parse(&["--world", "3", "--focus", "9.7,13.6"]), true);
        assert_eq!(point.lift, 1.0);
        assert_eq!(point.sweep, 0.0);
    }
}
