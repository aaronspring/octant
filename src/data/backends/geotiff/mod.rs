//! GeoTIFF and TIFF storage backend implementation.

pub mod blit;
pub mod coords;
pub mod decode;
pub mod inspect;
pub mod palette;
pub mod reader;
pub mod slice;
pub mod store;
#[cfg(test)]
pub mod test_utils;
#[cfg(test)]
pub mod tests;

pub use store::GeoTiffBlockStore;
