//! GeoTIFF and TIFF storage backend for `BlockStore`.

pub mod coords;
pub mod inspect;
pub mod reader;
pub mod slice;
pub mod store;
#[cfg(test)]
pub mod tests;

pub use coords::GeoSpatialBounds;
pub use inspect::inspect_tiff;
pub use reader::{MemoryTiffReader, create_async_reader};
pub use slice::fetch_geotiff_block;
pub use store::GeoTiffBlockStore;
