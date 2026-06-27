/// Scan chain insertion — distributes flip-flops into scan chains for
/// manufacturing test access.
use serde::{Deserialize, Serialize};

/// Configuration for scan chain insertion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanChainConfig {
    pub chain_count: usize,
    pub max_length: usize,
    pub clock_name: String,
    pub scan_enable: String,
    pub scan_in_prefix: String,
    pub scan_out_prefix: String,
}

/// A single flip-flop converted to a scan cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanCell {
    pub ff_name: String,
    pub scan_in: String,
    pub scan_out: String,
    pub chain_id: usize,
    pub position: usize,
}

/// A complete scan chain with its cells and port names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanChain {
    pub id: usize,
    pub cells: Vec<ScanCell>,
    pub length: usize,
    pub scan_in_port: String,
    pub scan_out_port: String,
}

/// Statistics for the inserted scan chains.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanStats {
    pub num_chains: usize,
    pub total_ffs: usize,
    pub max_length: usize,
    pub min_length: usize,
}

impl ScanChain {
    /// Compute scan chain statistics for a set of chains.
    pub fn scan_chain_stats(chains: &[ScanChain]) -> ScanStats {
        let num_chains = chains.len();
        let total_ffs: usize = chains.iter().map(|c| c.length).sum();
        let max_length = chains.iter().map(|c| c.length).max().unwrap_or(0);
        let min_length = chains.iter().map(|c| c.length).min().unwrap_or(0);
        ScanStats {
            num_chains,
            total_ffs,
            max_length,
            min_length,
        }
    }
}

/// Insert scan chains by distributing flip-flops evenly across the configured
/// number of chains, creating scan_in/scan_out connections between consecutive
/// cells in each chain.
pub fn insert_scan_chains(flip_flops: Vec<String>, config: &ScanChainConfig) -> Vec<ScanChain> {
    if config.chain_count == 0 || flip_flops.is_empty() {
        return Vec::new();
    }

    let chain_count = config.chain_count;
    let mut chains: Vec<Vec<String>> = vec![Vec::new(); chain_count];

    // Round-robin distribution for even balancing
    for (i, ff) in flip_flops.iter().enumerate() {
        chains[i % chain_count].push(ff.clone());
    }

    chains
        .into_iter()
        .enumerate()
        .map(|(chain_id, ffs)| {
            let scan_in_port = format!("{}{}", config.scan_in_prefix, chain_id);
            let scan_out_port = format!("{}{}", config.scan_out_prefix, chain_id);
            let length = ffs.len();

            let cells: Vec<ScanCell> = ffs
                .iter()
                .enumerate()
                .map(|(pos, ff_name)| {
                    let scan_in = if pos == 0 {
                        scan_in_port.clone()
                    } else {
                        format!("{}_so", ffs[pos - 1])
                    };
                    let scan_out = if pos == length - 1 {
                        scan_out_port.clone()
                    } else {
                        format!("{ff_name}_so")
                    };

                    ScanCell {
                        ff_name: ff_name.clone(),
                        scan_in,
                        scan_out,
                        chain_id,
                        position: pos,
                    }
                })
                .collect();

            ScanChain {
                id: chain_id,
                cells,
                length,
                scan_in_port,
                scan_out_port,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config(chain_count: usize) -> ScanChainConfig {
        ScanChainConfig {
            chain_count,
            max_length: 100,
            clock_name: "clk".into(),
            scan_enable: "SE".into(),
            scan_in_prefix: "SI".into(),
            scan_out_prefix: "SO".into(),
        }
    }

    #[test]
    fn even_distribution() {
        let ffs: Vec<String> = (0..12).map(|i| format!("ff_{i}")).collect();
        let chains = insert_scan_chains(ffs, &default_config(3));
        assert_eq!(chains.len(), 3);
        // Each chain should have exactly 4 FFs (12 / 3)
        for chain in &chains {
            assert_eq!(chain.length, 4);
        }
    }

    #[test]
    fn uneven_distribution() {
        let ffs: Vec<String> = (0..10).map(|i| format!("ff_{i}")).collect();
        let chains = insert_scan_chains(ffs, &default_config(3));
        let stats = ScanChain::scan_chain_stats(&chains);
        assert_eq!(stats.num_chains, 3);
        assert_eq!(stats.total_ffs, 10);
        // 10 / 3 → chains of length 4, 3, 3
        assert_eq!(stats.max_length, 4);
        assert_eq!(stats.min_length, 3);
    }

    #[test]
    fn scan_connections() {
        let ffs: Vec<String> = (0..4).map(|i| format!("ff_{i}")).collect();
        let chains = insert_scan_chains(ffs, &default_config(2));
        // Chain 0: ff_0, ff_2 (round-robin)
        let c0 = &chains[0];
        assert_eq!(c0.cells[0].scan_in, "SI0");
        assert_eq!(c0.cells.last().unwrap().scan_out, "SO0");
    }
}
