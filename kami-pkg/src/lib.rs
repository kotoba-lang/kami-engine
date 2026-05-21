/// KAMI Packaging — IC package modeling, wire bond / flip chip bonding,
/// and thermal analysis.

pub mod package;
pub mod bonding;
pub mod thermal;

pub use package::{PackageType, Package};
pub use bonding::{BondType, Bond, BondDiagram};
pub use thermal::{ThermalResult, ThermalSpec};
