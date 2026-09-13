# Octant Agent Rules

When developing and reviewing code in this repository:

1. **Rust Quality & Checks**:
   - Adhere to idiomatic Rust (Rust 2024 edition).
   - Enforce borrow-over-clone (`&[T]` over `&Vec<T>`, `&str` over `&String`).
   - Forbid `unwrap()` in production code (use `?`, `let Some(...) = ... else`, or `f32::total_cmp`).
   - Use poison-resilient lock handling (`if let Ok(guard) = ...` or `.unwrap_or_else(|p| p.into_inner())`).
   - Always use checked arithmetic when computing multi-dimensional tensor shape volumes (`shape.iter().try_fold(...)`).
   - Run `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`.
   - For C-library/FFI test suites (e.g. NetCDF), serialize file creation with static mutex test locks to prevent concurrent non-reentrant IO collisions.

2. **Modular Architecture & Subsystem Layout**:
   - Follow the Open-Closed Principle (OCP): decompose monolithic modules into single-purpose submodules (< 250 lines per file, < 50 lines per function).
   - **Data Engine Subsystems (`src/data/`)**:
     - **Blocks Engine (`src/data/blocks/`)**: `cache.rs` (LRU memory cache), `key.rs` (`BlockKey`), `loader.rs` (async background workers), `prefetch.rs` (`BlockPrefetcher`), `request.rs` (`SliceRequest`), `store.rs` (`BlockStore` trait), `summary.rs` (`BlockSummary`), `tests.rs`, `mod.rs`.
     - **Coordinates (`src/data/coordinates/`)**: `naming.rs` (dimension role inference & ASCII case-insensitive searches), `detection.rs`, `healpix.rs`, `regular.rs`, `topology.rs`.
     - **Backends (`src/data/backends/`)**: `coord_bounds.rs` (unified coordinate range bounds extraction), `generic_zarr.rs`, `icechunk.rs`, `netcdf.rs`, `procedural.rs`, `wasm_zarr.rs`.
     - **Data Slicing (`src/data/slicing/`)**: `common.rs` (math & range clamping), `coords.rs` (sliced coordinate extraction), `copy.rs` (strided & contiguous copy routines), `slice_1d.rs` (1D/0D slabs), `slice_2d.rs` (2D hyperslabs), `slice_3d.rs` (3D volumetric slabs).
     - Store tensor values in `Arc<[f32]>` for $O(1)$ zero-copy sharing between cache and render pipelines.
   - **WGPU Renderers (`src/plots/`)**:
     - All concrete plot renderers (`HeatmapRenderer`, `LineRenderer`, `Mesh3DRenderer`, `VolumeRenderer`, `PointCloudRenderer`) must implement the polymorphic `crate::plots::traits::PlotRenderer` trait.
     - Uniform parameter structs (such as `Mesh3DUniformParams`) must be zero-allocation `Copy` structs; never clone large coordinate vectors or data structures inside paint loops.
     - For all structured, discrete global (HEALPix, ICON, Cubed-Sphere), and unstructured grid formats (UGRID, MPAS), use zero-allocation GPU instancing or GPU vertex pulling. Reserve CPU mesh generation strictly for non-grid geometry (GIS vector polygons, streamlines, Marching Cubes CAD export, UI labels and annotations).
   - **Export Engine (`src/export/`)**:
     - Submodules: `raster.rs` (PNG, JPEG, WebP, Display P3 chunk injection), `vector.rs` (SVG, PDF), `clipboard.rs` (native file manager reveal & clipboard).
   - **UI Subsystems (`src/ui/`)**:
     - `src/ui/variables_overlay.rs`: Floating modal dialog for fast variable searching and active selection.
     - `src/ui/variables_panel/`: Docked sidebar inspector (`info.rs`, `mod.rs`) and dimension slider controls (`dimension_slider/`: `defaults.rs`, `double_slider.rs`, `metrics.rs`, `roles.rs`, `slice_req.rs`, `slider_row.rs`, `mod.rs`).
     - `src/ui/settings/`: Dedicated settings submodules (`clipping.rs`, `coastline.rs`, `export.rs`, `plot_2d.rs`, `plot_3d.rs`, `plot_options.rs`, `resampling.rs`, `mod.rs`).
     - `src/ui/colorbar/`: `ticks.rs` (scientific tick generation), `handles.rs` (input boxes and clip triangles), `mod.rs` (overlay coordinator).
     - `src/ui/hover/`: `callout.rs`, `camera.rs`, `enrich.rs`, `entries.rs`, `entries_1d.rs`, `entries_2d.rs`, `entries_3d.rs`, `format.rs`, `overlay.rs`, `raycast_sphere.rs`, `raycast_surface.rs`, `raycast_volume.rs`, `sample_1d.rs`, `sample_2d.rs`, `mod.rs`.
     - `src/app/pipeline/`: `aspect.rs`, `camera.rs`, `paint.rs`, `profile.rs`, `mod.rs`.

3. **Zero-Allocation UI & Render Loop Rules**:
   - In immediate-mode UI loops, prefer zero-allocation tuple salts `("salt", id)` over heap-allocating `format!(...)`.
   - Never allocate `String`s or dynamic `Vec`s per frame for tick labels; use stack buffers (`[u8; 32]`) and stack arrays (e.g. `[TickMark; 7]`).
   - Use zero-allocation case-insensitive ASCII searches (`contains_ascii_case_insensitive`) from `crate::data::coordinates::naming` for dimension and coordinate parsing.
   - All custom canvas overlays and floating toolbars must dynamically adapt to dark and light visual themes (`ui.visuals().dark_mode`).
   - Transient UI overlays (crop handles, grids, tooltips) must be suppressed during export capture passes (`if self.pending_export.is_none()`).
   - **No raw emojis or unicode glyphs**: Do not use font-dependent emojis or unicode symbols in UI widgets, buttons, labels, logs, or status messages. Use procedural vector icons from `crate::ui::icons::Icon` via `UiIconExt` (`icon`, `icon_button`, `icon_label`, `icon_colored`, or `Icon::paint`). If a new visual symbol is needed, define it as a procedural vector icon in `src/ui/icons/` with dark/light theme support.

4. **Skills Reference**:
   - `rust-skills`: 265 detailed Rust best practices across 26 categories (install locally via `git clone --depth 1 https://github.com/leonardomso/rust-skills.git .agents/skills/rust-skills && rm -rf .agents/skills/rust-skills/.git`).
   - `rust-workflows`: Cargo build, test, lint, clippy, WASM, and logging commands.
   - `octant-data-engine`: Data loading, caching, prefetching, and hyperslab slicing.
   - `octant-rendering-wgpu`: WGPU render pipelines, uniform buffer alignment, and WGSL shaders.
   - `octant-ui-egui`: egui immediate-mode GUI components and event dispatch.
