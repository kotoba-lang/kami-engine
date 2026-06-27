/// Clock Domain Crossing (CDC) analysis and violation detection.
use serde::{Deserialize, Serialize};

/// Type of CDC crossing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CrossingType {
    /// Single-bit signal crossing.
    SingleBit,
    /// Multi-bit bus crossing.
    MultiBit,
    /// Handshake protocol crossing.
    Handshake,
    /// Asynchronous FIFO crossing.
    FifoAsync,
}

/// Synchronizer type applied to a crossing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SynchronizerType {
    /// Two flip-flop synchronizer.
    TwoFF,
    /// Three flip-flop synchronizer (for high reliability).
    ThreeFF,
    /// Gray code encoding (for multi-bit).
    GrayCode,
    /// Mux-based synchronizer.
    MuxSync,
}

/// A signal crossing between two clock domains.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdcCrossing {
    /// Signal name.
    pub signal_name: String,
    /// Source clock domain.
    pub from_clock: String,
    /// Destination clock domain.
    pub to_clock: String,
    /// Crossing type.
    pub crossing_type: CrossingType,
    /// Synchronizer applied (None if missing).
    pub synchronizer: Option<SynchronizerType>,
}

/// Kind of CDC violation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CdcViolationKind {
    /// No synchronizer on a clock domain crossing.
    MissingSynchronizer,
    /// Multiple signals converging after separate synchronizers.
    ConvergenceIssue,
    /// Signal reconverges after being synchronized differently.
    ReconvergenceIssue,
    /// Multi-bit crossing without proper encoding (glitch-prone).
    GlitchProne,
}

/// A detected CDC violation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdcViolation {
    /// Signal name.
    pub signal: String,
    /// Type of issue.
    pub issue: CdcViolationKind,
}

/// CDC analysis report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdcReport {
    /// All detected crossings.
    pub crossings: Vec<CdcCrossing>,
    /// All detected violations.
    pub violations: Vec<CdcViolation>,
}

/// Clock domain definition.
#[derive(Debug, Clone)]
pub struct ClockDomain {
    /// Clock name.
    pub name: String,
    /// Frequency in MHz.
    pub freq_mhz: f64,
}

/// Signal with clock domain assignment.
#[derive(Debug, Clone)]
pub struct CdcSignal {
    /// Signal name.
    pub name: String,
    /// Source clock domain name.
    pub source_clock: String,
    /// Destination clock domain name.
    pub dest_clock: String,
    /// Bit width.
    pub width: u32,
    /// Whether a synchronizer is present.
    pub has_synchronizer: bool,
    /// Synchronizer type if present.
    pub synchronizer: Option<SynchronizerType>,
}

/// Analyze signals for CDC issues.
///
/// Checks each signal that crosses clock domains for missing synchronizers,
/// multi-bit glitch-prone crossings, and structural issues.
pub fn analyze_cdc(signals: &[CdcSignal], clocks: &[ClockDomain]) -> CdcReport {
    let mut crossings = Vec::new();
    let mut violations = Vec::new();

    let clock_names: Vec<&str> = clocks.iter().map(|c| c.name.as_str()).collect();

    for sig in signals {
        // Only analyze signals crossing between known clock domains.
        if sig.source_clock == sig.dest_clock {
            continue;
        }
        if !clock_names.contains(&sig.source_clock.as_str())
            || !clock_names.contains(&sig.dest_clock.as_str())
        {
            continue;
        }

        let crossing_type = if sig.width == 1 {
            CrossingType::SingleBit
        } else {
            CrossingType::MultiBit
        };

        crossings.push(CdcCrossing {
            signal_name: sig.name.clone(),
            from_clock: sig.source_clock.clone(),
            to_clock: sig.dest_clock.clone(),
            crossing_type: crossing_type.clone(),
            synchronizer: sig.synchronizer.clone(),
        });

        // Check for missing synchronizer.
        if !sig.has_synchronizer {
            violations.push(CdcViolation {
                signal: sig.name.clone(),
                issue: CdcViolationKind::MissingSynchronizer,
            });
        }

        // Check for multi-bit without Gray code.
        if sig.width > 1 {
            let has_gray = matches!(sig.synchronizer, Some(SynchronizerType::GrayCode));
            let has_fifo = matches!(crossing_type, CrossingType::FifoAsync);
            if !has_gray && !has_fifo && sig.has_synchronizer {
                violations.push(CdcViolation {
                    signal: sig.name.clone(),
                    issue: CdcViolationKind::GlitchProne,
                });
            }
        }
    }

    CdcReport {
        crossings,
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_clocks() -> Vec<ClockDomain> {
        vec![
            ClockDomain {
                name: "clk_100".to_string(),
                freq_mhz: 100.0,
            },
            ClockDomain {
                name: "clk_200".to_string(),
                freq_mhz: 200.0,
            },
        ]
    }

    #[test]
    fn missing_synchronizer_detected() {
        let signals = vec![CdcSignal {
            name: "req".to_string(),
            source_clock: "clk_100".to_string(),
            dest_clock: "clk_200".to_string(),
            width: 1,
            has_synchronizer: false,
            synchronizer: None,
        }];
        let report = analyze_cdc(&signals, &test_clocks());
        assert_eq!(report.crossings.len(), 1);
        assert_eq!(report.violations.len(), 1);
        assert_eq!(
            report.violations[0].issue,
            CdcViolationKind::MissingSynchronizer
        );
    }

    #[test]
    fn multibit_without_gray_is_glitch_prone() {
        let signals = vec![CdcSignal {
            name: "data_bus".to_string(),
            source_clock: "clk_100".to_string(),
            dest_clock: "clk_200".to_string(),
            width: 8,
            has_synchronizer: true,
            synchronizer: Some(SynchronizerType::TwoFF),
        }];
        let report = analyze_cdc(&signals, &test_clocks());
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].issue, CdcViolationKind::GlitchProne);
    }

    #[test]
    fn same_clock_not_flagged() {
        let signals = vec![CdcSignal {
            name: "internal".to_string(),
            source_clock: "clk_100".to_string(),
            dest_clock: "clk_100".to_string(),
            width: 1,
            has_synchronizer: false,
            synchronizer: None,
        }];
        let report = analyze_cdc(&signals, &test_clocks());
        assert!(report.crossings.is_empty());
        assert!(report.violations.is_empty());
    }
}
