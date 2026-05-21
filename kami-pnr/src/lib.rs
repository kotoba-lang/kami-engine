/// KAMI Place and Route — physical design backend for VLSI/ASIC layout.
///
/// Provides floorplanning, cell placement, clock tree synthesis (CTS),
/// maze routing, and GDSII stream export.

pub mod floorplan;
pub mod placement;
pub mod cts;
pub mod routing;
pub mod gdsii;

pub use floorplan::{Floorplan, FloorplanBlock, BlockType, IoPin, PinSide};
pub use placement::{Placement, PlacedCell, PlacementRow, Orientation, PlacementStats};
pub use cts::{ClockTree, CtsLevel, CtsBuffer, CtsWire, CtsSpec, CtsStats};
pub use routing::{RoutingGrid, RoutedNet, RouteSegment, RouteVia, Router, RoutingStats};
pub use gdsii::{GdsiiStream, GdsiiStructure, GdsiiElement};
