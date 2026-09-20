//! Dataset & data source session management subsystem.

pub mod factory;
pub mod handle;
pub mod item;
pub mod manager;
pub mod source;

pub use factory::SourceFactory;
pub use handle::StoreHandle;
pub use item::Dataset;
pub use manager::DatasetManager;
pub use source::{DataSource, DataSourceKind};
