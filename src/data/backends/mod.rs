pub mod coord_bounds;
pub mod generic_zarr;
pub mod icechunk;
#[cfg(not(target_arch = "wasm32"))]
pub mod icechunk_storage;
pub mod netcdf;
pub mod procedural;
pub mod wasm_icechunk;
pub mod wasm_zarr;
pub mod wasm_zstd_shim;
pub mod zarr;
pub mod zarr_block;
pub mod zarr_slice;
pub mod zarr_storage;

pub use coord_bounds::{
    fetch_all_dimension_coordinates, fetch_all_dimension_coordinates_for_variables,
    get_cached_coord_bounds, get_cached_coord_bounds_scoped, get_cached_coord_bounds_with_rank,
    read_coord_bounds, read_coord_bounds_scoped, read_coord_bounds_with_rank,
};
pub use generic_zarr::GenericZarrBlockStore;
pub use netcdf::NetCdfBlockStore;
pub use procedural::ProceduralBlockStore;
pub use wasm_zarr::WasmZarrBlockStore;
