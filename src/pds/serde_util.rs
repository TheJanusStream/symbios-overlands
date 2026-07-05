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
