# Rust & Octant Guidelines

## Core Principles

1. **Ownership & Borrowing**:
   - Prefer borrowing (`&T`, `&str`, `&[T]`) over `.clone()`.
   - Never clone collections or large structs solely to pass them to read-only functions.
   - Use `Arc<T>` (e.g. `Arc<[f32]>`) for zero-copy sharing across threads/pipelines; prefer references within local scopes.

2. **Error Handling & Arithmetic Safety**:
   - Never use `unwrap()` in production code paths (`err-no-unwrap-prod`). Use `?`, `match`, `if let`, or `expect()` with a detailed invariant message.
   - Use `f32::total_cmp` for deterministic float sorting.
   - Always use checked arithmetic (`checked_mul`, `try_fold`) when computing multi-dimensional tensor volumes.

3. **Performance & Allocation**:
   - Pre-allocate collections with `Vec::with_capacity` when the size is known.
   - Avoid `format!()` in hot UI loops or render passes; use tuple salts `("salt", id)` and stack buffers (`[u8; 32]`).
   - Zero-copy tensor slicing: verify tensor strides (`stride_x == 1`, `stride_y == nx`, `stride_z == nx * ny`) before copying.

4. **Modular Architecture**:
   - Maintain strict file line bounds (< 250 lines/file) and function line bounds (< 50 lines/fn).
   - Reusable domain logic belongs in modular subsystems (`src/data/blocks/`, `src/data/coordinates/`, `src/data/slicing/`, `src/ui/settings/`, `src/ui/hover/`, `src/ui/variables_panel/dimension_slider/`).

5. **WGPU & Shaders**:
   - Maintain 16-byte std140/std430 alignment on uniform structs with `#[repr(C)]` and `bytemuck::Pod`/`Zeroable`.
   - Always run `cargo clippy --all-targets -- -D warnings` and `cargo fmt --all -- --check`.

6. **Antigravity Skills**:
   - Access comprehensive Rust best practices and 265 specialized rules via the `rust-skills` skill.
   - Access Octant architecture and domain runbooks via `rust-workflows`, `octant-data-engine`, `octant-rendering-wgpu`, and `octant-ui-egui`.
