//! Desktop NetCDF storage backend reading local files via `libnetcdf`.

use crate::data::blocks::BlockStoreError;

/// NetCDF storage backend reading local files via `libnetcdf`.
#[derive(Debug, Clone)]
pub struct NetCdfBlockStore {
    pub(crate) file_path: String,
}

impl NetCdfBlockStore {
    /// Opens a NetCDF or HDF5 dataset from a local filesystem path.
    pub fn open_local(path: &str) -> Result<Self, BlockStoreError> {
        let clean_path = path
            .strip_prefix("file://")
            .or_else(|| path.strip_prefix("netcdf://"))
            .or_else(|| path.strip_prefix("hdf5://"))
            .unwrap_or(path)
            .trim()
            .trim_matches('\'')
            .trim_matches('"');
        let p = crate::utils::expand_tilde(clean_path);
        if !p.exists() {
            return Err(format!("File not found: {}", p.display()).into());
        }
        if p.is_dir() {
            return Err(format!(
                "'{}' is a directory. Please specify a .nc / .h5 / .hdf5 / .cdf file path.",
                p.display()
            )
            .into());
        }

        // Verify that the file can be opened and parsed by netcdf
        let _file = netcdf::open(&p)
            .map_err(|e| format!("Failed to open NetCDF file '{}': {e}", p.display()))?;

        Ok(Self {
            file_path: p.to_string_lossy().to_string(),
        })
    }

    pub fn file_path(&self) -> &str {
        &self.file_path
    }
}
