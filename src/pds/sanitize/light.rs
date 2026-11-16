//! [`LightSource`] sanitiser (#1921): every field held to what the drawing
//! can show and WebGL2 can hold, and rounded onto the wire's grid as a peer
//! reads it back, so a sanitised light equals itself saved and reloaded.
//!
//! The reach is the bound that matters on the web: a light's place in each
//! screen cluster it touches comes out of one fixed list there, and a long
//! light touches many (see [`limits::MAX_LIGHT_RANGE_M`]). The brightness
//! is held after it, by the light's power ([`max_light_brightness`]): a
//! shorter reach may be brighter, but no light is stronger than the
//! brightest of the longest reach. A spot's aim is
//! a direction and only its direction counts, so it is clamped, never
//! renormalised; a zero aim points down, as the drawing reads it. Unknown
//! arms are kept: a kind from a newer engine refuses this build's save
//! (#1111) rather than vanishing from the owner's record.

use super::Sanitize;
use super::common::clamp_finite;
use super::limits;
use crate::pds::light::{
    DEFAULT_RANGE_M, DEFAULT_SIZE_M, DEFAULT_SPOT_CONE_DEG, FLICKER_FINE_RATE, FLICKER_FINE_SHARE,
    LightFlicker, LightShape, LightSource,
};
use crate::pds::types::{Fp, Fp3};

/// `v` held in `[lo, hi]` (a non-finite one `default`) and put on the grid.
fn bounded(v: Fp, lo: f32, hi: f32, default: f32) -> Fp {
    Fp(clamp_finite(v.0, lo, hi, default)).on_wire()
}

impl Sanitize for LightSource {
    fn sanitize(&mut self) {
        self.shape.sanitize();
        self.color = Fp3(self.color.0.map(|c| clamp_finite(c, 0.0, 1.0, 1.0))).on_wire();
        self.range = bounded(
            self.range,
            limits::MIN_LIGHT_RANGE_M,
            limits::MAX_LIGHT_RANGE_M,
            DEFAULT_RANGE_M,
        );
        // After the reach, which sets how bright it may be.
        let brightest = max_light_brightness(self.range.0);
        self.brightness = bounded(self.brightness, 0.0, brightest, 1.0_f32.min(brightest));
        self.size = bounded(self.size, 0.0, limits::MAX_LIGHT_SIZE_M, DEFAULT_SIZE_M);
        let reach = limits::MAX_LIGHT_OFFSET_M;
        self.offset = Fp3(self.offset.0.map(|v| clamp_finite(v, -reach, reach, 0.0))).on_wire();
        if let Some(flicker) = self.flicker.as_mut() {
            flicker.sanitize();
        }
    }
}

impl Sanitize for LightShape {
    fn sanitize(&mut self) {
        if let LightShape::Spot { aim, inner, outer } = self {
            *aim = Fp3(aim.0.map(|v| clamp_finite(v, -1.0, 1.0, 0.0))).on_wire();
            *outer = bounded(
                *outer,
                limits::MIN_SPOT_ANGLE_DEG,
                limits::MAX_SPOT_ANGLE_DEG,
                DEFAULT_SPOT_CONE_DEG[1],
            );
            // The full cone sits inside the fading one. Both are on the
            // grid, so the inner one clamped to the outer stays there.
            *inner = bounded(*inner, 0.0, outer.0, DEFAULT_SPOT_CONE_DEG[0].min(outer.0));
        }
    }
}

/// The brightest a light of `range` (m, on the grid and inside its bounds)
/// may be, on the wire's grid: [`limits::MAX_LIGHT_BRIGHTNESS`], and no more
/// than holds its power - which grows with the brightness and the square of
/// the reach (`world_builder::light::lumens`) - to that of
/// [`limits::MAX_LIGHT_BRIGHTNESS_AT_MAX_RANGE`] at
/// [`limits::MAX_LIGHT_RANGE_M`]: 32 at a reach of 10 m and less, 22.2222
/// at 12, 8 at 20.
pub fn max_light_brightness(range: f32) -> f32 {
    let share = limits::MAX_LIGHT_RANGE_M / range.max(limits::MIN_LIGHT_RANGE_M);
    let brightest = (limits::MAX_LIGHT_BRIGHTNESS_AT_MAX_RANGE * share * share)
        .min(limits::MAX_LIGHT_BRIGHTNESS);
    // Down onto the grid, so the bound a peer reads back is still under it.
    Fp((brightest * 10_000.0).floor() / 10_000.0).on_wire().0
}

/// The fastest `rate` a flicker (`pulse` false) or pulse of `amount` may
/// move at, on the wire's grid: [`limits::MAX_LIGHT_FLICKER_RATE_HZ`] for
/// one too shallow to flash, else what holds it to
/// [`limits::MAX_LIGHT_FLASHES_HZ`] - a pulse's period, a flicker's broad
/// waver and, where its share of the dip flashes too, its quicker fine
/// shimmer.
pub fn max_flicker_rate_hz(pulse: bool, amount: f32) -> f32 {
    let flashes = |depth: f32| depth > limits::LIGHT_FLASH_DEPTH;
    let fastest = if pulse {
        if flashes(amount) {
            limits::MAX_LIGHT_FLASHES_HZ
        } else {
            limits::MAX_LIGHT_FLICKER_RATE_HZ
        }
    } else if flashes(amount * FLICKER_FINE_SHARE) {
        limits::MAX_LIGHT_FLASHES_HZ / FLICKER_FINE_RATE
    } else if flashes(amount * (1.0 - FLICKER_FINE_SHARE)) {
        limits::MAX_LIGHT_FLASHES_HZ
    } else {
        limits::MAX_LIGHT_FLICKER_RATE_HZ
    };
    // Down onto the grid, so the bound a peer reads back is still under it.
    Fp((fastest * 10_000.0).floor() / 10_000.0).on_wire().0
}

impl Sanitize for LightFlicker {
    fn sanitize(&mut self) {
        let pulse = matches!(self, LightFlicker::Pulse { .. });
        if let LightFlicker::Flicker { amount, rate } | LightFlicker::Pulse { amount, rate } = self
        {
            *amount = bounded(*amount, 0.0, 1.0, 0.0);
            let fastest = max_flicker_rate_hz(pulse, amount.0);
            *rate = bounded(
                *rate,
                limits::MIN_LIGHT_FLICKER_RATE_HZ,
                fastest,
                1.0_f32.min(fastest),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sanitized(mut light: LightSource) -> LightSource {
        light.sanitize();
        light
    }

    /// A hostile or mistaken light is held drawable: every field into its
    /// bounds, a non-finite one to its rest, and every value onto the grid.
    #[test]
    fn every_field_is_held_inside_its_bounds() {
        let light = sanitized(LightSource {
            shape: LightShape::Spot {
                aim: Fp3([f32::NAN, -3.0, 0.123_456]),
                inner: Fp(170.0),
                outer: Fp(400.0),
            },
            color: Fp3([1.5, -0.2, f32::INFINITY]),
            brightness: Fp(1e9),
            range: Fp(1e6),
            size: Fp(-1.0),
            offset: Fp3([1e9, f32::NAN, -0.000_04]),
            dusk_to_dawn: false,
            flicker: Some(LightFlicker::Pulse {
                amount: Fp(4.0),
                rate: Fp(1e4),
            }),
        });
        assert_eq!(
            light.shape,
            LightShape::Spot {
                aim: Fp3([0.0, -1.0, 0.1235]),
                inner: Fp(limits::MAX_SPOT_ANGLE_DEG),
                outer: Fp(limits::MAX_SPOT_ANGLE_DEG),
            }
        );
        assert_eq!(light.color, Fp3([1.0, 0.0, 1.0]));
        assert_eq!(
            light.brightness,
            Fp(limits::MAX_LIGHT_BRIGHTNESS_AT_MAX_RANGE),
            "the brightest a light of the longest reach may be"
        );
        assert_eq!(light.range, Fp(limits::MAX_LIGHT_RANGE_M));
        assert_eq!(light.size, Fp(0.0));
        assert_eq!(light.offset, Fp3([limits::MAX_LIGHT_OFFSET_M, 0.0, 0.0]));
        assert!(!light.dusk_to_dawn, "a flag is the owner's to set");
        assert_eq!(
            light.flicker,
            Some(LightFlicker::Pulse {
                amount: Fp(1.0),
                rate: Fp(limits::MAX_LIGHT_FLASHES_HZ),
            }),
            "a full-depth pulse flashes no faster than lightning"
        );

        let low = sanitized(LightSource {
            shape: LightShape::Spot {
                aim: Fp3([0.0, -1.0, 0.0]),
                inner: Fp(f32::NAN),
                outer: Fp(0.0),
            },
            brightness: Fp(f32::NAN),
            range: Fp(0.0),
            size: Fp(f32::NAN),
            flicker: Some(LightFlicker::Flicker {
                amount: Fp(-1.0),
                rate: Fp(0.0),
            }),
            ..LightSource::default()
        });
        assert_eq!(
            low.shape,
            LightShape::Spot {
                aim: Fp3([0.0, -1.0, 0.0]),
                inner: Fp(limits::MIN_SPOT_ANGLE_DEG),
                outer: Fp(limits::MIN_SPOT_ANGLE_DEG),
            },
            "a NaN inner cone takes the fresh one, held inside the outer"
        );
        assert_eq!(low.brightness, Fp(1.0), "a NaN brightness is the lamp's");
        assert_eq!(low.range, Fp(limits::MIN_LIGHT_RANGE_M));
        assert_eq!(low.size, Fp(DEFAULT_SIZE_M));
        assert_eq!(
            low.flicker,
            Some(LightFlicker::Flicker {
                amount: Fp(0.0),
                rate: Fp(limits::MIN_LIGHT_FLICKER_RATE_HZ),
            })
        );
    }

    /// Off the grid in, on the grid out: a value between two steps lands on
    /// the nearer, so the light in memory is the one every peer reads back.
    #[test]
    fn every_value_lands_on_the_wire_grid() {
        let light = sanitized(LightSource {
            color: Fp3([0.123_44, 0.5, 0.987_66]),
            brightness: Fp(1.000_04),
            range: Fp(7.333_333),
            size: Fp(0.050_06),
            offset: Fp3([0.1, 2.000_06, -0.333_33]),
            ..LightSource::default()
        });
        assert_eq!(light.color, Fp3([0.1234, 0.5, 0.9877]));
        assert_eq!(light.brightness, Fp(1.0));
        assert_eq!(light.range, Fp(7.3333));
        assert_eq!(light.size, Fp(0.0501));
        assert_eq!(light.offset, Fp3([0.1, 2.0001, -0.3333]));
    }

    /// A sanitised light is unchanged by a second pass and by a save and
    /// reload, which saves the same bytes - for every flicker mode, a spot
    /// and values off the grid.
    #[test]
    fn a_sanitised_light_is_stable_through_a_second_pass_and_the_wire() {
        for kind in LightFlicker::KINDS {
            let once = sanitized(LightSource {
                shape: LightShape::Spot {
                    aim: Fp3([0.6, -0.48, 0.640_04]),
                    inner: Fp(12.345_67),
                    outer: Fp(33.333_33),
                },
                color: Fp3([0.333_33, 0.666_67, 0.1]),
                brightness: Fp(2.468_13),
                range: Fp(13.131_31),
                size: Fp(0.123_45),
                offset: Fp3([1.111_11, -2.222_22, 3.333_33]),
                dusk_to_dawn: false,
                flicker: LightFlicker::fresh(kind),
            });
            assert_eq!(sanitized(once.clone()), once);
            let saved = serde_json::to_string(&once).expect("serialise");
            let reloaded: LightSource = serde_json::from_str(&saved).expect("decode");
            assert_eq!(reloaded, once, "a peer reads back the light in memory");
            let resaved = serde_json::to_string(&sanitized(reloaded)).expect("serialise");
            assert_eq!(resaved, saved, "a reload saves the same bytes");
        }
    }

    /// A fresh light and a fresh spot are already sanitised, so a light the
    /// editor adds is not moved by its first save.
    #[test]
    fn a_fresh_light_is_left_as_it_is() {
        assert_eq!(sanitized(LightSource::default()), LightSource::default());
        assert_eq!(sanitized(LightSource::spot()), LightSource::spot());
        let unknown = sanitized(LightSource {
            shape: LightShape::Unknown,
            flicker: Some(LightFlicker::Unknown),
            ..LightSource::default()
        });
        assert_eq!(unknown.shape, LightShape::Unknown, "a newer kind is kept");
        assert_eq!(unknown.flicker, Some(LightFlicker::Unknown));
    }

    /// A light's power is bounded, not its brightness alone (#1922): a lamp
    /// of a modest reach may be as bright as a tall post needs - 32 at 10 m,
    /// 22.2222 at 12 - but no light, at any reach, is stronger than the
    /// brightest of the longest reach (8 at 20 m), and each bound is the
    /// most that holds it, on the wire's grid.
    #[test]
    fn a_lights_power_is_bounded_whatever_its_reach() {
        use crate::world_builder::light::lumens;
        let strongest = lumens(
            limits::MAX_LIGHT_BRIGHTNESS_AT_MAX_RANGE,
            limits::MAX_LIGHT_RANGE_M,
        );
        let held = |brightness: f32, range: f32| {
            let light = sanitized(LightSource {
                brightness: Fp(brightness),
                range: Fp(range),
                ..LightSource::default()
            });
            (light.brightness.0, light.range.0)
        };
        assert_eq!(held(1e9, 10.0), (limits::MAX_LIGHT_BRIGHTNESS, 10.0));
        assert_eq!(held(32.0, 12.0), (22.2222, 12.0), "a tall post's lamp");
        assert_eq!(held(16.0, 12.0), (16.0, 12.0), "under the bound, as asked");
        assert_eq!(
            held(32.0, limits::MAX_LIGHT_RANGE_M),
            (
                limits::MAX_LIGHT_BRIGHTNESS_AT_MAX_RANGE,
                limits::MAX_LIGHT_RANGE_M
            )
        );
        assert_eq!(held(32.0, 1e6).0, limits::MAX_LIGHT_BRIGHTNESS_AT_MAX_RANGE);
        // Every reach on a 1 cm sweep: the bound is on the grid, holds the
        // power, and a step more would not (where 32 does not cap it).
        for cm in 50..=2000 {
            let range = Fp(cm as f32 / 100.0).on_wire().0;
            let top = max_light_brightness(range);
            assert_eq!(top, Fp(top).on_wire().0, "{range}: {top} on the grid");
            assert!(top <= limits::MAX_LIGHT_BRIGHTNESS, "{range}: {top}");
            assert!(
                lumens(top, range) <= strongest * (1.0 + 1e-6),
                "{range}: {top} is {} lm, over {strongest}",
                lumens(top, range)
            );
            if top < limits::MAX_LIGHT_BRIGHTNESS {
                assert!(
                    lumens(top + 0.0002, range) > strongest,
                    "{range}: {top} is not the most"
                );
            }
            assert_eq!(held(1e9, range), (top, range));
        }
    }

    /// No light flashes faster than lightning, at any setting (#1921): a
    /// pulse or flicker dipping past a tenth of its light is held to two
    /// flashes a second - a deep flicker's fine shimmer, 2.7 times its rate,
    /// included - while a shallow one may move as fast as the ceiling; and
    /// every bound sits on the wire's grid, at or under the flash cap.
    #[test]
    fn no_light_flashes_faster_than_lightning() {
        let fast = |kind: &str, amount: f32| {
            let mut flicker = match kind {
                "pulse" => LightFlicker::Pulse {
                    amount: Fp(amount),
                    rate: Fp(1e4),
                },
                _ => LightFlicker::Flicker {
                    amount: Fp(amount),
                    rate: Fp(1e4),
                },
            };
            flicker.sanitize();
            match flicker {
                LightFlicker::Pulse { rate, .. } | LightFlicker::Flicker { rate, .. } => rate.0,
                LightFlicker::Unknown => unreachable!(),
            }
        };
        let flashes = limits::MAX_LIGHT_FLASHES_HZ;
        assert_eq!(fast("pulse", 1.0), flashes);
        assert_eq!(fast("pulse", 0.11), flashes);
        assert_eq!(fast("pulse", 0.1), limits::MAX_LIGHT_FLICKER_RATE_HZ);
        // A full flicker's fine shimmer (a third of the dip) flashes too.
        let fine = fast("flicker", 1.0);
        assert!(fine * FLICKER_FINE_RATE <= flashes, "{fine}");
        assert!(fine * FLICKER_FINE_RATE > flashes - 0.001, "{fine}");
        assert_eq!(fine, Fp(fine).on_wire().0, "on the grid");
        // A middling one: only its broad waver flashes.
        assert_eq!(fast("flicker", 0.25), flashes);
        // A shallow one may be quick.
        assert_eq!(fast("flicker", 0.12), limits::MAX_LIGHT_FLICKER_RATE_HZ);
        // A fresh flicker and pulse are already within it.
        for kind in LightFlicker::KINDS {
            let fresh = LightFlicker::fresh(kind).expect("a kind");
            let mut again = fresh.clone();
            again.sanitize();
            assert_eq!(again, fresh, "{kind}");
        }
    }

    /// A node's light is sanitised wherever the node is: in a room's tree,
    /// at its root and below it, and in an avatar's visuals - where slice
    /// D's vehicles carry theirs (#1924) - bounded and on the grid alike.
    #[test]
    fn a_nodes_light_is_sanitised_in_a_room_and_on_an_avatar() {
        use crate::pds::Generator;
        let wild = || LightSource {
            range: Fp(1e6),
            brightness: Fp(f32::NAN),
            offset: Fp3([0.123_456, 0.0, 0.0]),
            ..LightSource::default()
        };
        let tame = sanitized(wild());
        assert_eq!(tame.range, Fp(limits::MAX_LIGHT_RANGE_M));
        let tree = || {
            let mut root = Generator::default_cuboid();
            root.light = Some(wild());
            let mut child = Generator::default_cuboid();
            child.light = Some(wild());
            root.children.push(child);
            root
        };
        let mut room = tree();
        crate::pds::sanitize_generator(&mut room);
        let mut body = tree();
        crate::pds::sanitize_avatar_visuals(&mut body);
        for node in [&room, &room.children[0], &body, &body.children[0]] {
            assert_eq!(node.light.as_ref(), Some(&tame));
        }
    }
}
