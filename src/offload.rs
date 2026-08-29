//! Platform-routed CPU-generation offload.
//!
//! [`offload`] takes a self-contained [`GenJob`] and returns a
//! `bevy::tasks::Task<GenResult>` that the caller polls each frame — the same
//! API on every target. On **native** the job runs on the multithreaded
//! `AsyncComputeTaskPool` (Bevy's async-executor task pool), giving real
//! parallelism off the main schedule.
//!
//! On **wasm** Bevy's task pools collapse to a single cooperative thread, so a
//! job run inline would stall the render frame. Instead the wasm backend
//! dispatches the job to a **pooled** Web Worker (the `gen_worker` crate,
//! spawned via `gloo-worker` and kept warm between jobs — see the `worker`
//! submodule for the pool, #802), which runs it on a real worker thread —
//! matching native's off-the-frame progressive loading. The worker links only
//! the Bevy-free [`gen_jobs`] crate, never the engine.
//!
//! **It is no longer small.** Adding `GenJob::AvatarBuild` (#1061) linked the
//! whole avatar engine into it: measured at 839 KB gzipped (3.9 MB raw)
//! against ~16 KB before. That is one lazy fetch, cached, and it buys the
//! browser build the only off-main-thread body build available to it — but it
//! is a real boot cost for a session that never renders a rigged avatar, and
//! splitting the roster across two worker artifacts is filed as #1063.
//!
//! ## The determinism claim, stated accurately (#1132)
//!
//! The shared [`gen_jobs::GenJob::run`] means native and worker execution run
//! the same SOURCE. That used to be written here as a guarantee of
//! byte-identical results, and it was not one: the same source compiled for
//! `x86_64-unknown-linux-gnu` and for `wasm32-unknown-unknown` links different
//! implementations of `f32::sin`, `cos`, `powf` and `exp` — glibc's and
//! `compiler-builtins`', respectively — and Rust documents those results as
//! platform-dependent. A native owner and a browser guest could therefore
//! derive measurably different terrain from one record, which is the opposite
//! of what a thin client relying on this is entitled to assume.
//!
//! So the derivation path no longer calls them. Every transcendental in it
//! routes through the pure-Rust [`libm`] crate here and in `symbios-ground`,
//! which computes the same operations in the same order on every target.
//! `sqrt` is left alone deliberately: IEEE-754 requires it correctly rounded,
//! so it is already identical everywhere.
//!
//! The claim this file may now make is: **same source, same arithmetic, same
//! bits — on any target.** What is verified rather than argued is the
//! `symbios-ground` half, whose `tests/determinism.rs` pins a bake and a splat
//! scoring against committed hashes and was checked to move when the call
//! sites revert. No gate in this repo executes a wasm32 test binary, so the
//! browser side of the equality rests on `libm` being pure Rust, not on a
//! measurement — and [`crate::world_digest`] exists so that a real pair of
//! peers can settle it in the field.

use bevy::tasks::Task;
pub use gen_jobs::{GenJob, GenResult};

/// Dispatch a generation job and return a task to poll for its [`GenResult`].
///
/// Polled with the usual `futures_lite::future::{block_on, poll_once}` idiom,
/// identically on native and wasm.
#[cfg(not(target_arch = "wasm32"))]
pub fn offload(job: GenJob) -> Task<GenResult> {
    // The ticket rides inside the future, so it is dropped both when the job
    // finishes and when the caller cancels the `Task` (#1143).
    let ticket = census::start(census::label(&job));
    bevy::tasks::AsyncComputeTaskPool::get().spawn(async move {
        let _ticket = ticket;
        job.run()
    })
}

/// Wasm dispatch: run the job on a Web Worker (off the render thread) and
/// resolve the task when it returns. `spawn_local` drives the worker round-trip
/// on the JS event loop; a oneshot channel bridges the result back into a Bevy
/// `Task` so callers poll it exactly as on native.
#[cfg(target_arch = "wasm32")]
pub fn offload(job: GenJob) -> Task<GenResult> {
    let (tx, rx) = futures_channel::oneshot::channel::<GenResult>();
    // The ticket belongs to the `spawn_local` future, not to the `Task` below:
    // dropping a wasm `Task` does not cancel the work behind it, and it is the
    // worker round-trip that either answers or does not. A `gen-worker.js`
    // that 404s leaves this future parked forever, which is precisely the
    // in-flight entry the watchdog needs to see (#1143).
    let ticket = census::start(census::label(&job));
    wasm_bindgen_futures::spawn_local(async move {
        let _ticket = ticket;
        let result = worker::run_on_worker(job).await;
        // The receiver is dropped only if the terrain task was cancelled; then
        // there is simply nothing to deliver the result to.
        let _ = tx.send(result);
    });
    bevy::tasks::IoTaskPool::get().spawn(async move {
        rx.await
            .expect("gen-worker dropped before returning a result")
    })
}

#[cfg(target_arch = "wasm32")]
mod worker;

/// Process-global census of in-flight offloaded jobs (#1143).
///
/// The problem this exists to solve is that a job which never answers is
/// invisible. On wasm `offload` awaits a oneshot whose sender lives inside a
/// `spawn_local` future that only completes when the worker replies, and
/// `spawn_worker` has no error path at all: a 404 or a CSP refusal on
/// `./gen-worker.js`, or a worker-side OOM, simply never resolves. Nothing
/// observed that — `OffloadJobStarted` had no emit site anywhere in the crate,
/// so even the offline replay rule that exists for this
/// (`offload.task_never_resolves`) had nothing to fold.
///
/// The census is armed inside [`offload`] rather than at the six call sites,
/// which is the whole point: the avatar build (#1061) is the newest and
/// heaviest job in the roster and was the one nobody instrumented. A ledger the
/// dispatch function itself keeps cannot be forgotten by the next caller.
///
/// It is a plain global with no clock of its own beyond a monotonic `Instant`
/// because `offload` has no `World` — the same constraint that shapes
/// [`crate::diagnostics::panic`]. The Bevy side ([`sample_offload_census`])
/// drains it each frame and turns transitions into session events.
///
/// [`sample_offload_census`]: crate::diagnostics::offload_watch::sample_offload_census
pub mod census {
    use std::sync::Mutex;

    use bevy::platform::time::Instant;

    use super::GenJob;

    /// A short, stable name per job kind — the string that appears in
    /// `OffloadJobStarted { job }` and in the watchdog's message. Deliberately
    /// coarse: the census answers "is something stuck", and per-instance
    /// identity is the `id` beside it.
    pub fn label(job: &GenJob) -> &'static str {
        match job {
            GenJob::Heightmap(_) => "heightmap",
            GenJob::AudioBake(_) => "audio_bake",
            GenJob::TextureBake { .. } => "texture_bake",
            GenJob::AvatarBuild { .. } => "avatar_build",
        }
    }

    /// One in-flight job.
    struct Pending {
        id: u64,
        job: &'static str,
        started: Instant,
    }

    /// A start or an end, queued for the next drain by the Bevy sampler.
    pub enum Transition {
        Started {
            id: u64,
            job: &'static str,
        },
        Finished {
            id: u64,
            job: &'static str,
            elapsed_secs: f64,
        },
    }

    struct Census {
        next_id: u64,
        pending: Vec<Pending>,
        transitions: Vec<Transition>,
    }

    static CENSUS: Mutex<Census> = Mutex::new(Census {
        next_id: 0,
        pending: Vec::new(),
        transitions: Vec::new(),
    });

    /// Held for the lifetime of one offloaded job; retires it on drop.
    ///
    /// A guard rather than an explicit `finish()` call because the job's future
    /// has two endings, not one: it completes, or the caller drops the `Task`
    /// and cancels it. Only a `Drop` covers both — and a cancelled job left in
    /// the ledger would have the watchdog reporting a stall that nobody is
    /// waiting on.
    pub struct Ticket {
        id: u64,
    }

    impl Drop for Ticket {
        fn drop(&mut self) {
            let Ok(mut census) = CENSUS.lock() else {
                return;
            };
            let Some(index) = census.pending.iter().position(|p| p.id == self.id) else {
                return;
            };
            let entry = census.pending.remove(index);
            let elapsed_secs = entry.started.elapsed().as_secs_f64();
            census.transitions.push(Transition::Finished {
                id: entry.id,
                job: entry.job,
                elapsed_secs,
            });
        }
    }

    /// Register a job as in flight. The returned [`Ticket`] must be moved into
    /// the future that runs it.
    pub fn start(job: &'static str) -> Ticket {
        let Ok(mut census) = CENSUS.lock() else {
            return Ticket { id: u64::MAX };
        };
        let id = census.next_id;
        census.next_id += 1;
        census.pending.push(Pending {
            id,
            job,
            started: Instant::now(),
        });
        census.transitions.push(Transition::Started { id, job });
        Ticket { id }
    }

    /// Take every transition since the last call.
    pub fn drain_transitions() -> Vec<Transition> {
        match CENSUS.lock() {
            Ok(mut census) => std::mem::take(&mut census.transitions),
            Err(_) => Vec::new(),
        }
    }

    /// Every job still in flight, as `(id, job, age_secs)`.
    ///
    /// Returns all of them, not just the oldest: a stale gen-worker fails every
    /// job, and reporting one of six would understate a broken deploy as a
    /// single slow bake.
    pub fn pending_snapshot() -> Vec<(u64, &'static str, f64)> {
        match CENSUS.lock() {
            Ok(census) => census
                .pending
                .iter()
                .map(|p| (p.id, p.job, p.started.elapsed().as_secs_f64()))
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod determinism_goldens {
    //! Goldens for the claim this module's docs make: every peer derives the
    //! same world from the same record, bit for bit, on any target (#1132).
    //!
    //! ## Why a golden and not a round-trip
    //!
    //! A round-trip test — derive twice, assert equal — passes on every build
    //! in existence, because each build agrees with itself. The failure this
    //! guards is two DIFFERENT builds disagreeing, and the only way to test
    //! that from one of them is to compare against a number the other one
    //! could also compute. That is what these constants are.
    //!
    //! ## What they prove, and what they do not
    //!
    //! They prove the derivation is pinned: reverting a call site from `libm`
    //! back to the `f32` method moves the hash and fails the assertion. That
    //! was verified by doing it, not assumed — see the sibling
    //! `symbios-ground/tests/determinism.rs`, whose bake parameters had to be
    //! chosen deliberately, because glibc and `libm` agree on most inputs and
    //! disagree on roughly one in ten.
    //!
    //! They do NOT prove a wasm32 build computes the same numbers: no gate in
    //! this repo executes a wasm test binary. The argument that it does is the
    //! `libm` crate's own — pure Rust, no platform dispatch, the same
    //! operations in the same order everywhere. These constants are what make
    //! that argument settleable rather than merely plausible, and
    //! [`crate::world_digest`] is how a real pair of peers settles it in a
    //! live room.

    use super::{GenJob, GenResult};
    use crate::pds::SovereignTerrainConfig;

    /// Hash raw `f32` bits, NOT [`crate::world_digest::heightmap_digest`].
    ///
    /// The world digest is quantised to a millimetre on purpose — it reports
    /// divergence a player could see and stays quiet about last-bit noise,
    /// which is what makes it usable as a live cross-peer instrument. That is
    /// exactly the wrong sensitivity here: this module's claim is bit-identity,
    /// so it has to see the bits.
    fn hash_bits(samples: &[f32]) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for s in samples {
            for b in s.to_le_bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }

    fn bake(cfg: &SovereignTerrainConfig) -> Vec<f32> {
        match GenJob::Heightmap(crate::terrain::heightmap_params(cfg)).run() {
            GenResult::Heightmap(d) => d.data,
            _ => panic!("a heightmap job yielded something that is not a heightmap"),
        }
    }

    /// The terrain a seeded region gets by default, on a 129-square grid so the
    /// test costs milliseconds. Generator, seed, octaves and erosion settings
    /// are all the shipped defaults, so this bakes terrain the app actually
    /// produces rather than a contrivance.
    fn default_bake() -> Vec<f32> {
        bake(&SovereignTerrainConfig {
            grid_size: 129,
            ..SovereignTerrainConfig::default()
        })
    }

    /// A Diamond-Square bake at roughness `0.768`, and both choices are
    /// deliberate.
    ///
    /// The default config above is Voronoi terracing plus hydraulic erosion,
    /// and neither reaches an argument where implementations disagree: Voronoi
    /// uses only `sqrt` (which IEEE-754 requires correctly rounded, so it is
    /// identical everywhere) and the erosion kernel's `exp` arguments are a
    /// small set of `-(dx²+dz²)/r` on which glibc and `libm` happen to agree.
    /// Measured: [`GOLDEN_DEFAULT_HEIGHTMAP`] did NOT move when `symbios-ground`
    /// 0.3.2 rerouted those calls — which is real news for existing regions
    /// (their ground did not shift) and useless as a regression guard.
    ///
    /// Diamond-Square's per-octave amplitude is `powf(0.5, 1.5 - roughness)`,
    /// where the two disagree on about one roughness in ten. 0.768 is one of
    /// them, and it is large enough that the perturbed amplitude survives
    /// rounding into the grid instead of being absorbed. This bake is
    /// therefore the one that would actually catch an upstream regression
    /// reaching this repo.
    fn divergence_sensitive_bake() -> Vec<f32> {
        bake(&SovereignTerrainConfig {
            grid_size: 129,
            generator_kind: crate::pds::SovereignGeneratorKind::DiamondSquare,
            ds_roughness: crate::pds::Fp(0.768),
            ..SovereignTerrainConfig::default()
        })
    }

    /// Raw-bit hash of [`default_bake`], taken on x86-64 with every
    /// transcendental in the derivation routed through `libm`.
    ///
    /// A failure means the seeded terrain moved. Sometimes that is intended (a
    /// generator changed, or `symbios-ground` cut a release that shifts
    /// output) — in which case re-take this constant DELIBERATELY and say so,
    /// because every existing region's ground just changed shape. Sometimes it
    /// means a transcendental went back to an `f32` method, which is the
    /// regression this exists to catch.
    const GOLDEN_DEFAULT_HEIGHTMAP: u64 = 17_463_291_289_401_008_650;

    /// Raw-bit hash of [`divergence_sensitive_bake`]. This is the constant with
    /// teeth — see that function for why the default config has none.
    ///
    /// Verified rather than assumed: resolving `symbios-ground` back to 0.3.1
    /// (the release before it routed its transcendentals through `libm`) makes
    /// this assertion fail and leaves [`GOLDEN_DEFAULT_HEIGHTMAP`] passing,
    /// which is precisely the split those two constants exist to express.
    const GOLDEN_SENSITIVE_HEIGHTMAP: u64 = 8_569_161_101_078_850_543;

    #[test]
    fn the_default_seeded_terrain_hashes_to_its_golden() {
        assert_eq!(
            hash_bits(&default_bake()),
            GOLDEN_DEFAULT_HEIGHTMAP,
            "the default seeded heightmap moved — read this module's docs before re-cutting"
        );
    }

    #[test]
    fn a_terrain_that_reaches_a_divergent_transcendental_hashes_to_its_golden() {
        assert_eq!(
            hash_bits(&divergence_sensitive_bake()),
            GOLDEN_SENSITIVE_HEIGHTMAP,
            "the seeded heightmap moved at parameters that reach a transcendental \
             where implementations disagree — this is the regression the default \
             bake above cannot see"
        );
    }

    /// The job that crosses the worker boundary must be deterministic in the
    /// ordinary sense before the golden above means anything. The control: if
    /// this fails, the golden is noise and the problem is not the toolchain.
    #[test]
    fn the_same_params_bake_the_same_heightmap_twice() {
        assert_eq!(hash_bits(&default_bake()), hash_bits(&default_bake()));
    }

    /// 46.5°, and the odd value is the point.
    ///
    /// `slope_cutoff` is `1 - cos(θ)`, and glibc and `libm` agree on most
    /// angles: at 15°, 30°, 45° and 60° they return identical bits, so a test
    /// at any round angle would pass against the pre-fix code and prove
    /// nothing. Sweeping 0-90° at a tenth of a degree finds them disagreeing on
    /// about one value in eighty; 46.5° is the first.
    const DIVERGENT_SLOPE_DEG: f32 = 46.5;

    /// `slope_cutoff(46.5°)` as `libm` computes it.
    ///
    /// Pinned as raw bits rather than compared within a tolerance, because a
    /// tolerance would pass on exactly the difference this watches for: the
    /// cutoff is COMPARED against a sampled slope to accept or reject each
    /// scatter instance, so a sample within a ULP of it gets a tree on one peer
    /// and bare ground on the other. The finding is that the last bit decides
    /// something.
    const GOLDEN_SLOPE_CUTOFF_BITS: u32 = 1_050_644_478;

    #[test]
    fn the_slope_cutoff_is_pinned_at_an_angle_where_implementations_disagree() {
        use crate::pds::{Fp, ScatterNaturalness};
        let naturalness = ScatterNaturalness {
            max_slope_deg: Some(Fp(DIVERGENT_SLOPE_DEG)),
            ..ScatterNaturalness::default()
        };
        let cutoff = crate::world_builder::compile::scatter::slope_cutoff(&naturalness)
            .expect("a max slope was set");
        assert_eq!(
            cutoff.to_bits(),
            GOLDEN_SLOPE_CUTOFF_BITS,
            "the slope cutoff moved: {cutoff} — every borderline scatter sample \
             either side of this value changes its answer"
        );
    }

    /// The digest instrument (#1146) must agree with the goldens above about
    /// what it is looking at. This is the join between the two: a peer
    /// comparing digests in a live room is comparing the same derivation these
    /// constants pin.
    #[test]
    fn the_world_digest_of_the_pinned_bake_is_itself_stable() {
        let a = crate::world_digest::heightmap_digest(129, 129, &default_bake());
        assert_eq!(
            a,
            crate::world_digest::heightmap_digest(129, 129, &default_bake())
        );

        // And it is a digest of something, not a constant: a different seed is
        // a different terrain and must read as one.
        let cfg = SovereignTerrainConfig {
            grid_size: 129,
            seed: SovereignTerrainConfig::default().seed.wrapping_add(1),
            ..SovereignTerrainConfig::default()
        };
        let GenResult::Heightmap(other) =
            GenJob::Heightmap(crate::terrain::heightmap_params(&cfg)).run()
        else {
            panic!("a heightmap job yielded something that is not a heightmap");
        };
        assert_ne!(
            a,
            crate::world_digest::heightmap_digest(129, 129, &other.data)
        );
    }
}
