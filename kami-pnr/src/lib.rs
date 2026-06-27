pub mod cts;
/// KAMI Place and Route — physical design backend for VLSI/ASIC layout.
///
/// Provides floorplanning, cell placement, clock tree synthesis (CTS),
/// maze routing, and GDSII stream export.
pub mod floorplan;
pub mod gdsii;
pub mod placement;
pub mod routing;

pub use cts::{ClockTree, CtsBuffer, CtsLevel, CtsSpec, CtsStats, CtsWire};
pub use floorplan::{BlockType, Floorplan, FloorplanBlock, IoPin, PinSide};
pub use gdsii::{GdsiiElement, GdsiiStream, GdsiiStructure};
pub use placement::{Orientation, PlacedCell, Placement, PlacementRow, PlacementStats};
pub use routing::{RouteSegment, RouteVia, RoutedNet, Router, RoutingGrid, RoutingStats};
