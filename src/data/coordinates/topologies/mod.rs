//! Concrete grid topology implementations.

pub mod cartesian;
pub mod curvilinear;
pub mod healpix;
pub mod irregular_1d;

pub use cartesian::CartesianTopology;
pub use curvilinear::CurvilinearTopology;
pub use healpix::HealpixTopology;
pub use irregular_1d::Irregular1DTopology;
