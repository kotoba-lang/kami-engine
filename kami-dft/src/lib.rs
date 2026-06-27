pub mod atpg;
pub mod bist;
pub mod jtag;
/// KAMI Design for Test — scan chain insertion, BIST generation,
/// ATPG pattern generation, and JTAG/BSDL support.
pub mod scan;

pub use atpg::{AtpgResult, Fault, FaultType, TestPattern};
pub use bist::{
    BistType, LbistConfig, LbistController, MarchAlgorithm, MbistConfig, MbistController,
};
pub use jtag::{BoundaryScanCell, BsdlDevice, CellType, JtagInstruction};
pub use scan::{ScanCell, ScanChain, ScanChainConfig, ScanStats};
