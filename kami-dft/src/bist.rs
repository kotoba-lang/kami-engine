/// Built-In Self-Test — memory BIST (MBIST) and logic BIST (LBIST) generation.

use serde::{Deserialize, Serialize};

/// Type of BIST controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BistType {
    MemoryBist,
    LogicBist,
}

/// March algorithm variant for memory testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarchAlgorithm {
    MarchC,
    MarchCMinus,
    MarchB,
    MarchA,
    Checkerboard,
}

impl MarchAlgorithm {
    /// Number of march elements (read/write operations per address) for each algorithm.
    pub fn march_elements(&self) -> usize {
        match self {
            MarchAlgorithm::MarchC => 10,       // {w0}; {r0,w1}; {r1,w0}; {r0,w1}; {r1,w0}; {r0}
            MarchAlgorithm::MarchCMinus => 10,
            MarchAlgorithm::MarchB => 17,
            MarchAlgorithm::MarchA => 15,
            MarchAlgorithm::Checkerboard => 4,  // write pattern, read pattern, write complement, read complement
        }
    }
}

/// Configuration for memory BIST.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MbistConfig {
    pub memory_name: String,
    pub algorithm: MarchAlgorithm,
    pub data_width: u32,
    pub addr_width: u32,
}

/// Generated MBIST controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MbistController {
    pub config: MbistConfig,
    pub state_count: usize,
    pub test_time_cycles: u64,
}

/// Configuration for logic BIST.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LbistConfig {
    pub seed: u64,
    pub polynomial: u64,
    pub scan_chain_count: usize,
}

/// Generated LBIST controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LbistController {
    pub config: LbistConfig,
    pub test_cycle_count: u64,
}

/// Create a memory BIST controller for the given configuration.
///
/// Calculates state machine complexity and test time based on
/// the march algorithm and memory dimensions.
pub fn create_mbist(config: MbistConfig) -> MbistController {
    let num_addresses = 1u64 << config.addr_width;
    let march_ops = config.algorithm.march_elements() as u64;

    // Each march element visits every address once, plus overhead for FSM transitions
    let test_time_cycles = num_addresses * march_ops + 10; // 10 cycles init/done overhead

    // State count: init + one state per march element + compare + done
    let state_count = config.algorithm.march_elements() + 3;

    MbistController {
        config,
        state_count,
        test_time_cycles,
    }
}

/// Create a logic BIST controller for the given configuration.
///
/// Uses LFSR-based pseudo-random pattern generation with MISR compaction.
pub fn create_lbist(config: LbistConfig) -> LbistController {
    // Test cycles: 1024 patterns per scan chain (standard LBIST depth)
    let test_cycle_count = 1024 * config.scan_chain_count as u64;

    LbistController {
        config,
        test_cycle_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mbist_march_c_cycle_count() {
        let config = MbistConfig {
            memory_name: "sram_4k".into(),
            algorithm: MarchAlgorithm::MarchC,
            data_width: 32,
            addr_width: 12, // 4096 addresses
        };
        let ctrl = create_mbist(config);
        // MarchC: 10 elements * 4096 addresses + 10 overhead = 40970
        assert_eq!(ctrl.test_time_cycles, 4096 * 10 + 10);
        assert_eq!(ctrl.state_count, 13); // 10 + 3
    }

    #[test]
    fn lbist_test_cycles() {
        let config = LbistConfig {
            seed: 0xDEAD_BEEF,
            polynomial: 0x8005,
            scan_chain_count: 4,
        };
        let ctrl = create_lbist(config);
        assert_eq!(ctrl.test_cycle_count, 1024 * 4);
    }
}
