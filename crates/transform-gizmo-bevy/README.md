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
- **Changed here:** the port to Bevy 0.20 (#1634). Each change made in this
  copy is listed below, so it can be offered upstream.

When upstream publishes a release for the Bevy version Overlands is on, this
copy can go: point `transform-gizmo-bevy` in the root `Cargo.toml` back at
crates.io and delete this folder, after moving any change listed here
upstream.

## Changes from upstream

The port to Bevy 0.20 (0.20.1), against upstream 0.11.0. `lib.rs`,
`mouse_interact.rs`, `picking.rs`, `prelude.rs` and `gizmo.wgsl` are
unchanged; everything else is below.

1. **`Cargo.toml`:** every `bevy_*` dependency moves from `0.19` to
   `0.20.1`, still with `default-features = false`.
2. **`Cargo.toml`: a new dependency, `bevy_extract`** (`0.20.1`,
   `default-features = false`). Bevy 0.20 moved extraction out of
   `bevy_render` into the new `bevy_extract` crate, and the
   `ExtractComponent` derive's expansion names `bevy_extract` directly, so a
   crate deriving it must depend on it (`bevy_render::extract_component`
   still re-exports the trait and the derive).
3. **`render.rs`: `#[extract_app(RenderApp)]` on `GizmoDrawDataHandle`.**
   Extraction is generic over the target sub-app in 0.20 (migration guide,
   "Extract Extract"), and the `ExtractComponent` derive refuses to expand
   without the attribute naming it. Behaviour is unchanged: as upstream,
   no `ExtractComponentPlugin` is added for this type - the handles reach
   the render world through `extract_gizmo_data`.
4. **`render.rs`: `TemporaryRenderEntity::default()`** in
   `extract_gizmo_data`, where upstream spawns the unit value
   `TemporaryRenderEntity`. In 0.20 the name is an alias of
   `bevy_extract`'s `TemporaryEntity<RenderApp>`, a generic tuple struct
   over a `PhantomData`, so it has no unit value to spawn (same guide).
5. **`render.rs`: `constants: Vec::new()`** in the pipeline's
   `VertexState` and `FragmentState`. Both gained a `constants` field in
   0.20 (values for pipeline-overridable `override` constants); the gizmo
   shader declares none.

`gizmo.wgsl` needed nothing: Bevy 0.20 replaced naga_oil with WESL, and a
plain WGSL file with no preprocessor directives - which this one is - keeps
working as it is (migration guide, "WESL Shaders"). The `PERSPECTIVE` shader
def the pipeline pushes is read by no line of the shader, as upstream.

## License

Upstream's terms, MIT OR Apache-2.0: the MIT text with upstream's notice is
in `LICENSE-MIT`, and the Apache-2.0 text is the repository's own `LICENSE`.
