pub mod bus_protocol;
pub mod cdc;
/// KAMI IP Management — IP-XACT component catalog, bus protocol generation,
/// NoC topology synthesis, and CDC analysis.
pub mod ip_xact;
pub mod noc;

pub use bus_protocol::{ApbConfig, AxiConfig};
pub use cdc::{
    CdcCrossing, CdcReport, CdcViolation, CdcViolationKind, CrossingType, SynchronizerType,
};
pub use ip_xact::{BusInterface, BusType, IpCatalog, IpParam, IpPort, IpXactComponent};
pub use noc::{NocConfig, NocDesign, NocPort, NocRouter, NocTopology};
