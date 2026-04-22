//! Symbios Overlands — binary entry point.
//!
//! All engine wiring lives in the library crate ([`symbios_overlands`]); this
//! binary is a one-line shim that hands control to [`symbios_overlands::run`].
//! Keeping the binary thin lets integration tests in `tests/` reuse the
//! library's public API directly.

fn main() {
    symbios_overlands::run();
}
