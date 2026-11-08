# transform-gizmo-bevy (Overlands' copy)

The Bevy integration of [transform-gizmo](https://github.com/urholaukkarinen/transform-gizmo),
carried in this workspace so Overlands can move to a new Bevy release without
waiting for upstream to publish one (#1634). The World Editor's in-world
gizmo (`src/editor_gizmo/`), the wearables' placement gizmo and the render
tool's editor host all build on it.

- **Taken from:** `transform-gizmo-bevy` 0.11.0 on crates.io, upstream commit
  `07a951ec17ebff2de4608b711ee0544ff2c8063c` (`crates/transform-gizmo-bevy`).
- **Left out:** upstream's two examples, which need the whole `bevy` crate as
  a dev-dependency.
- **Not copied:** the engine-agnostic core, `transform-gizmo`, which holds the
  handle geometry, hit tests and drag maths. It has no Bevy dependency, so
  it stays a crates.io dependency.
- **Changed here:** nothing yet beyond formatting. Each change made in this
  copy is listed below, so it can be offered upstream.

When upstream publishes a release for the Bevy version Overlands is on, this
copy can go: point `transform-gizmo-bevy` in the root `Cargo.toml` back at
crates.io and delete this folder, after moving any change listed here
upstream.

## Changes from upstream

(none)

## License

Upstream's terms, MIT OR Apache-2.0: the MIT text with upstream's notice is
in `LICENSE-MIT`, and the Apache-2.0 text is the repository's own `LICENSE`.
