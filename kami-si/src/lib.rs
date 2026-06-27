pub mod crosstalk;
pub mod eye_diagram;
pub mod s_param;
/// KAMI Signal Integrity — transmission line analysis, eye diagram generation,
/// crosstalk analysis, and S-parameter extraction.
pub mod transmission_line;

pub use crosstalk::{CouplingType, CrosstalkResult};
pub use eye_diagram::{EyeDiagramData, EyeMetrics};
pub use s_param::{SParamMetrics, SParameter};
pub use transmission_line::{TLineParams, TLineType};
