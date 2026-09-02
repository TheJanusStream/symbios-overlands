//! Default-eliding serialization (Stage 1 of the single-record-boundary
//! plan, #695).
//!
//! Record weight is dominated by config structs whose fields mostly hold
//! their `Default` values — a catalogue prop repeats an identity
//! [`TortureParams`](super::generator::TortureParams), a near-default
//! material and an identity transform on every one of its dozens of child
//! prims. [`impl_default_eliding_serialize!`] replaces a struct's derived
//! `Serialize` with one that emits only the fields that *differ* from the
//! struct's `Default` instance, which shrinks records severalfold without
//! any schema change.
//!
//! The contract that keeps round-trips exact: every struct using this macro
//! MUST deserialize missing fields from the same `Default` the serializer
//! compared against — i.e. carry a container-level `#[serde(default)]` (or
//! equivalent per-field defaults). The macro destructures `Self`, so a new
//! field is a compile error here rather than a silently-always-serialized
//! (or worse, silently-dropped) one. Renamed fields cannot use the macro
//! as-is (`serialize_field` uses the Rust identifier); none of the current
//! users rename.
//!
//! Reader compatibility: elision only changes what is *written*. Existing
//! full records decode unchanged, and clients built before a struct adopted
//! the macro can decode elided output only if they already tolerated the
//! missing fields — the same forward-compat rule (`#[serde(default)]`,
//! no `deny_unknown_fields`) every record type here follows.

/// Implement a default-eliding `serde::Serialize` for a struct: fields equal
/// to their value in `Self::default()` are omitted from the output. List
/// every field; the destructuring pattern makes an omission or a rename a
/// compile error.
///
/// A field that uses a `#[serde(with = "module")]` custom wire format
/// declares it as `name via module(FieldType)` so the eliding impl routes
/// through the same `module::serialize` the old derive used (e.g. the
/// `u64_as_string` seeds) instead of silently changing the wire shape.
///
/// A field marked `name (always)` is written unconditionally. Use this when
/// an *absent* key already has a legacy meaning that differs from the
/// struct's default — e.g. `ParticleParams::procedural_texture`, where a
/// missing key means "pre-sprite record, plain quads" while the struct
/// default is the soft-disc sprite. Eliding such a field would silently
/// rewrite the legacy meaning onto every round-trip.
macro_rules! impl_default_eliding_serialize {
    ($ty:ident { $( $field:ident $( ($mode:ident) )? $( via $with:ident ($fty:ty) )? ),+ $(,)? }) => {
        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeStruct;
                // Exhaustive destructure: adding a field to the struct
                // without listing it here fails to compile.
                let Self { $($field),+ } = self;
                let default = Self::default();
                let mut len = 0usize;
                $(
                    if crate::pds::serde_util::impl_default_eliding_serialize!(
                        @keep default, $field $( ($mode) )?
                    ) {
                        len += 1;
                    }
                )+
                let mut state = serializer.serialize_struct(stringify!($ty), len)?;
                $(
                    if crate::pds::serde_util::impl_default_eliding_serialize!(
                        @keep default, $field $( ($mode) )?
                    ) {
                        crate::pds::serde_util::impl_default_eliding_serialize!(
                            @field state, $field $( via $with($fty) )?
                        );
                    }
                )+
                state.end()
            }
        }
    };
    (@keep $default:ident, $field:ident) => {
        *$field != $default.$field
    };
    (@keep $default:ident, $field:ident (always)) => {
        true
    };
    (@field $state:ident, $field:ident) => {
        $state.serialize_field(stringify!($field), $field)?;
    };
    (@field $state:ident, $field:ident via $with:ident ($fty:ty)) => {{
        struct Adapter<'a>(&'a $fty);
        impl serde::Serialize for Adapter<'_> {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                $with::serialize(self.0, s)
            }
        }
        $state.serialize_field(stringify!($field), &Adapter($field))?;
    }};
}

pub(crate) use impl_default_eliding_serialize;

/// Declare a `SovereignXxx` mirror of an upstream config struct — the
/// struct, its `Default`, `to_native()` and `from_native()` — from one
/// field list, so the four cannot drift apart (#1160).
///
/// Each field is declared by its *kind* followed by `: name = default`;
/// the kind selects the wire-format wrapper and the conversion rule:
///
/// | kind            | mirror type | native type   | conversion                  |
/// |-----------------|-------------|---------------|-----------------------------|
/// | `fp`            | `Fp`        | `f32`         | wrap / unwrap               |
/// | `fp3`           | `Fp3`       | `[f32; 3]`    | wrap / unwrap               |
/// | `fp64`          | `Fp64`      | `f64`         | wrap / unwrap               |
/// | `u32`           | `u32`       | `u32`         | copy                        |
/// | `usize`         | `u32`       | `usize`       | `as` cast each way          |
/// | `bool`          | `bool`      | `bool`        | copy                        |
/// | `enum(T)`       | `T`         | `T`           | clone — one shared type     |
/// | `nested(S)`     | `S`         | `S::native`   | `S::to_native(&)` / `from_native(&)` |
/// | `mirror(S)`     | `S`         | `S::native`   | `S::to_native(self)` / `from_native(by value)` — `Copy` mirror enums |
///
/// The first token picks the wire discipline, and it is the one thing the
/// two families of mirror disagree on:
///
/// * `eliding` — the texture mirrors (#695): a container-level
///   `#[serde(default)]` on read and
///   [`impl_default_eliding_serialize!`] on write, so a default-valued
///   config collapses to `{}` on the wire.
/// * `verbatim` — the audio mirrors: a plain derived `Serialize` /
///   `Deserialize`, every field written every time in declaration order.
///   Generators carrying an audio patch are content-addressed over those
///   bytes, and the audio mirrors have always written the full form, so
///   switching them to elision would re-address every such child record
///   on its next publish. Per-field `#[serde(...)]` attributes pass
///   through, which is how a field added after a record shape shipped
///   keeps its own decode default.
///
/// Attributes and doc comments before the struct name apply to the
/// struct; those before a field kind apply to that field.
macro_rules! define_sovereign_mirror {
    (
        eliding
        $(#[$meta:meta])*
        $sov:ident => $native:path {
            $( $(#[$fmeta:meta])* $kind:ident $( ( $sub:ty ) )? : $field:ident = $default:expr ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(serde::Deserialize, Clone, Debug, PartialEq)]
        #[serde(default)]
        pub struct $sov {
            $( $(#[$fmeta])* pub $field: $crate::pds::serde_util::define_sovereign_mirror!(@ty $kind $(($sub))?), )+
        }

        $crate::pds::serde_util::impl_default_eliding_serialize!($sov {
            $( $field ),+
        });

        $crate::pds::serde_util::define_sovereign_mirror!(@impls $sov => $native {
            $( $kind $(($sub))? : $field = $default ),+
        });
    };
    (
        verbatim
        $(#[$meta:meta])*
        $sov:ident => $native:path {
            $( $(#[$fmeta:meta])* $kind:ident $( ( $sub:ty ) )? : $field:ident = $default:expr ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
        pub struct $sov {
            $( $(#[$fmeta])* pub $field: $crate::pds::serde_util::define_sovereign_mirror!(@ty $kind $(($sub))?), )+
        }

        $crate::pds::serde_util::define_sovereign_mirror!(@impls $sov => $native {
            $( $kind $(($sub))? : $field = $default ),+
        });
    };

    (@impls $sov:ident => $native:path {
        $( $kind:ident $( ( $sub:ty ) )? : $field:ident = $default:expr ),+ $(,)?
    }) => {
        impl Default for $sov {
            fn default() -> Self {
                Self {
                    $( $field: $crate::pds::serde_util::define_sovereign_mirror!(@default $kind $(($sub))?, $default), )+
                }
            }
        }

        impl $sov {
            pub fn to_native(&self) -> $native {
                $native {
                    $( $field: $crate::pds::serde_util::define_sovereign_mirror!(@to_native $kind $(($sub))?, self.$field), )+
                }
            }

            pub fn from_native(native: &$native) -> Self {
                Self {
                    $( $field: $crate::pds::serde_util::define_sovereign_mirror!(@from_native $kind $(($sub))?, native.$field), )+
                }
            }
        }
    };

    (@ty fp)          => { $crate::pds::types::Fp };
    (@ty fp3)         => { $crate::pds::types::Fp3 };
    (@ty fp64)        => { $crate::pds::types::Fp64 };
    (@ty u32)         => { u32 };
    (@ty usize)       => { u32 };
    (@ty bool)        => { bool };
    (@ty enum ($e:ty))   => { $e };
    (@ty nested ($t:ty)) => { $t };
    (@ty mirror ($t:ty)) => { $t };

    (@default fp, $v:expr)            => { $crate::pds::types::Fp($v) };
    (@default fp3, $v:expr)           => { $crate::pds::types::Fp3($v) };
    (@default fp64, $v:expr)          => { $crate::pds::types::Fp64($v) };
    (@default u32, $v:expr)           => { $v };
    (@default usize, $v:expr)         => { $v };
    (@default bool, $v:expr)          => { $v };
    (@default enum ($e:ty), $v:expr)    => { $v };
    (@default nested ($t:ty), $v:expr)  => { $v };
    (@default mirror ($t:ty), $v:expr)  => { $v };

    (@to_native fp, $v:expr)          => { $v.0 };
    (@to_native fp3, $v:expr)         => { $v.0 };
    (@to_native fp64, $v:expr)        => { $v.0 };
    (@to_native u32, $v:expr)         => { $v };
    (@to_native usize, $v:expr)       => { $v as usize };
    (@to_native bool, $v:expr)        => { $v };
    (@to_native enum ($e:ty), $v:expr)   => { $v.clone() };
    (@to_native nested ($t:ty), $v:expr) => { $v.to_native() };
    (@to_native mirror ($t:ty), $v:expr) => { $v.to_native() };

    (@from_native fp, $v:expr)        => { $crate::pds::types::Fp($v) };
    (@from_native fp3, $v:expr)       => { $crate::pds::types::Fp3($v) };
    (@from_native fp64, $v:expr)      => { $crate::pds::types::Fp64($v) };
    (@from_native u32, $v:expr)       => { $v };
    (@from_native usize, $v:expr)     => { $v as u32 };
    (@from_native bool, $v:expr)      => { $v };
    (@from_native enum ($e:ty), $v:expr)   => { ($v).clone() };
    (@from_native nested ($t:ty), $v:expr) => { <$t>::from_native(&$v) };
    (@from_native mirror ($t:ty), $v:expr) => { <$t>::from_native($v) };
}

pub(crate) use define_sovereign_mirror;
