//! In-memory procedural and known-truth synthetic block store.

pub mod healpix;
pub mod healpix_meta;
pub mod inspect;
pub mod slice_2d;
pub mod slice_3d;
pub mod store;

#[cfg(test)]
mod tests;

use crate::data::blocks::BlockStoreError;

pub struct ProceduralBlockStore {
    pub uri: String,
}

impl ProceduralBlockStore {
    pub fn open(uri: &str) -> Result<Self, BlockStoreError> {
        Ok(Self {
            uri: uri.to_string(),
        })
    }
}
