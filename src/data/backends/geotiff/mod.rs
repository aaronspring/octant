//! GeoTIFF and TIFF storage backend for `BlockStore`.

pub mod blit;
pub mod coords;
pub mod decode;
pub mod inspect;
pub mod palette;
pub mod reader;
pub mod slice;
pub mod store;
pub mod tasks;
#[cfg(test)]
pub mod tests;

pub use blit::ReadWindow;
pub use coords::GeoSpatialBounds;
pub use inspect::inspect_tiff;
pub use reader::{MemoryTiffReader, create_async_reader};
pub use slice::fetch_geotiff_block;
pub use store::GeoTiffBlockStore;
