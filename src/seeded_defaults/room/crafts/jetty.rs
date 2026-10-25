//! The jetty (#1782): out from the shore near the arrival over the water, a
//! boat or two moored at its head.
//!
//! The probe it is sited on misreads a shore by metres up and down, by
//! tens of them along a gentle one (#1777), so it is sited loosely - a bank
//! near the probe's water line, square to the shore as the probe reads it,
//! the sea out past its length - and the compile walks it along its own
//! axis to the water's edge the full ground draws
//! (`world_builder::compile::pad::meet_the_shore`) - and draws it only
//! where its head then stands in water deep enough for its boats
//! (`world_builder::compile::pad::place_stands`). It is placed at its
//! deck's height over the water, unsnapped, and its boats are part of it
//! (`catalogue::items::crafts::jetty::moored`), so they go where it goes.

use super::{FARTHEST, Ground, MARGIN, Place, Siting, Stands, Wiring, facing, placed, unit};
use crate::catalogue::items::crafts::jetty as kit;
use crate::seeded_defaults::room::siting::TerrainProbe;
use crate::world_builder::compile::scatter::JETTY;

/// How long a seeded jetty is (m): the kit's, which the compile reads its
/// head's depth at.
const LENGTH: f32 = kit::SEEDED_LENGTH;
/// How high over the probe's water the bank a jetty is sited on stands, at
/// the least and the most (m): near its water line, the compile finding
/// the true one.
const BANK: [f32; 2] = [-1.0, 3.0];
/// How deep the probe reads the water past its head, at the least (m): the
/// sea lies that way.
const SEA: f32 = 0.3;
/// How far round its bank the water is read for which way the shore faces
/// (m).
const SHORE_READ: f32 = 30.0;
/// How far along its axis the compile's walk may take it, either way, for
/// the ground it keeps (m): the compile's own reach
/// (`world_builder::compile::pad::meet_the_shore`), a quarter of the walks
/// in #1782's fleet past 20 m and the longest 47 m.
const WALK: f32 = 50.0;
/// How far round the jetty's bank end the scatters keep off (m): its deck
/// runs [`kit::BACK`] onto the bank.
const BANK_KEEP: f32 = kit::BACK + 1.5;

/// A jetty: the bank it is sited on, and which way it runs out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Jetty {
    pub land: [f32; 2],
    pub out: [f32; 2],
}

/// The point `s` metres from `from` along `dir`.
fn along(from: [f32; 2], dir: [f32; 2], s: f32) -> [f32; 2] {
    [from[0] + dir[0] * s, from[1] + dir[1] * s]
}

/// Which way the water lies round `at`: toward every cell at or under the
/// water line within [`SHORE_READ`], the nearer the more - square off the
/// shore, where a cell's own nearest wet one may lie along it.
fn seaward(probe: &TerrainProbe, at: [f32; 2]) -> Option<[f32; 2]> {
    let cell = probe.cell_size();
    let steps = (SHORE_READ / cell).ceil() as i32;
    let water = probe.water_y();
    let mut pull = [0.0f32; 2];
    for j in -steps..=steps {
        for i in -steps..=steps {
            let (dx, dz) = (i as f32 * cell, j as f32 * cell);
            let d2 = dx * dx + dz * dz;
            if d2 <= 0.0 || d2 > SHORE_READ * SHORE_READ {
                continue;
            }
            if probe.height_between([at[0] + dx, at[1] + dz]) <= water {
                pull[0] += dx / d2;
                pull[1] += dz / d2;
            }
        }
    }
    unit(pull)
}

impl Place for Jetty {
    /// The bank nearest the landing near the probe's water line, square to
    /// its shore, the sea past its length, clear of what is kept along all
    /// the way the compile may walk it.
    fn site(siting: &Siting) -> Option<Self> {
        let probe = siting.probe;
        let water = probe.water_y();
        siting
            .lattice(1.0, FARTHEST, kit::BACK + WALK)
            .into_iter()
            .find_map(|(land, _)| {
                let bank = probe.height_between(land) - water;
                if !(BANK[0]..=BANK[1]).contains(&bank) {
                    return None;
                }
                let out = seaward(probe, land)?;
                let past = water - probe.height_between(along(land, out, LENGTH + 2.0));
                let jetty = Jetty { land, out };
                (past >= SEA && siting.kept.clear(&jetty.ground()[0], MARGIN)).then_some(jetty)
            })
    }

    /// From the bank behind it to its head, its boats beside, and as far
    /// either way along its axis as the compile's walk may take it.
    fn ground(&self) -> Vec<Ground> {
        vec![Ground {
            a: along(self.land, self.out, -(kit::BACK + WALK)),
            b: along(self.land, self.out, LENGTH + WALK),
            r: kit::BERTH_OFF + 1.2,
        }]
    }

    /// The jetty and its boats at its deck's height over the water, running
    /// out along its front: one boat lying out to sea, or a second beside
    /// it lying back to the land.
    fn wire(&self, wiring: &mut Wiring) {
        let berths: Vec<(f32, bool)> = [(1.0, true), (-1.0, false)]
            .into_iter()
            .take(wiring.boats)
            .collect();
        wiring
            .generators
            .push((JETTY.to_string(), kit::moored(LENGTH, &berths)));
        wiring.placements.push(placed(
            JETTY,
            self.land,
            wiring.water_y + kit::FREEBOARD,
            facing(self.out),
            Stands::Placed(BANK_KEEP),
        ));
    }
}
