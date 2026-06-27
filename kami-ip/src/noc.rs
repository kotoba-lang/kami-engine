/// Network-on-Chip topology synthesis and router generation.
use serde::{Deserialize, Serialize};

/// NoC topology type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NocTopology {
    /// 2D mesh with given rows and columns.
    Mesh { rows: u32, cols: u32 },
    /// Ring with given node count.
    Ring { nodes: u32 },
    /// Full crossbar with given port count.
    Crossbar { ports: u32 },
    /// Fat tree with given levels.
    Tree { levels: u32 },
}

/// Router port direction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PortDirection {
    North,
    South,
    East,
    West,
    Local,
}

/// A single port on a NoC router.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NocPort {
    /// Port direction.
    pub direction: PortDirection,
    /// Bandwidth in Gbps.
    pub bandwidth_gbps: f64,
    /// Latency in clock cycles.
    pub latency_cycles: u32,
}

/// Routing algorithm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoutingAlgorithm {
    XY,
    WestFirst,
    OddEven,
}

/// NoC configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NocConfig {
    /// Topology type.
    pub topology: NocTopology,
    /// Data width in bits.
    pub data_width: u32,
    /// Flit size in bits.
    pub flit_size: u32,
    /// Routing algorithm.
    pub routing: RoutingAlgorithm,
}

/// A router in the NoC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NocRouter {
    /// Router ID.
    pub id: u32,
    /// X coordinate in the topology.
    pub x: u32,
    /// Y coordinate in the topology.
    pub y: u32,
    /// Ports on this router.
    pub ports: Vec<NocPort>,
}

/// Link between two routers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NocLink {
    /// Source router ID.
    pub src_id: u32,
    /// Destination router ID.
    pub dst_id: u32,
    /// Bandwidth in Gbps.
    pub bandwidth_gbps: f64,
}

/// Complete NoC design.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NocDesign {
    /// All routers.
    pub routers: Vec<NocRouter>,
    /// All inter-router links.
    pub links: Vec<NocLink>,
    /// Estimated total area in um^2.
    pub total_area_um2: f64,
    /// Estimated worst-case latency in cycles.
    pub estimated_latency_cycles: u32,
}

/// Generate a NoC design from configuration.
///
/// Creates routers and links according to the topology, then estimates
/// area and latency from data width, flit size, and topology diameter.
pub fn generate_noc(config: &NocConfig) -> NocDesign {
    let link_bw = config.data_width as f64 * 1.0; // Gbps at 1 GHz
    let link_latency = 1_u32;

    match &config.topology {
        NocTopology::Mesh { rows, cols } => {
            let mut routers = Vec::with_capacity((*rows * *cols) as usize);
            let mut links = Vec::new();

            for r in 0..*rows {
                for c in 0..*cols {
                    let id = r * cols + c;
                    let mut ports = vec![NocPort {
                        direction: PortDirection::Local,
                        bandwidth_gbps: link_bw,
                        latency_cycles: 0,
                    }];

                    if r > 0 {
                        ports.push(NocPort {
                            direction: PortDirection::North,
                            bandwidth_gbps: link_bw,
                            latency_cycles: link_latency,
                        });
                        links.push(NocLink {
                            src_id: id,
                            dst_id: (r - 1) * cols + c,
                            bandwidth_gbps: link_bw,
                        });
                    }
                    if r < rows - 1 {
                        ports.push(NocPort {
                            direction: PortDirection::South,
                            bandwidth_gbps: link_bw,
                            latency_cycles: link_latency,
                        });
                    }
                    if c > 0 {
                        ports.push(NocPort {
                            direction: PortDirection::West,
                            bandwidth_gbps: link_bw,
                            latency_cycles: link_latency,
                        });
                        links.push(NocLink {
                            src_id: id,
                            dst_id: r * cols + c - 1,
                            bandwidth_gbps: link_bw,
                        });
                    }
                    if c < cols - 1 {
                        ports.push(NocPort {
                            direction: PortDirection::East,
                            bandwidth_gbps: link_bw,
                            latency_cycles: link_latency,
                        });
                    }

                    routers.push(NocRouter {
                        id,
                        x: c,
                        y: r,
                        ports,
                    });
                }
            }

            // Area estimate: ~5000 um^2 per router port * total ports.
            let total_ports: usize = routers.iter().map(|r| r.ports.len()).sum();
            let area = total_ports as f64 * 5000.0 * (config.data_width as f64 / 32.0);
            // Worst-case latency: mesh diameter.
            let diameter = (rows - 1) + (cols - 1);

            NocDesign {
                routers,
                links,
                total_area_um2: area,
                estimated_latency_cycles: diameter * link_latency + 1,
            }
        }
        NocTopology::Ring { nodes } => {
            let mut routers = Vec::with_capacity(*nodes as usize);
            let mut links = Vec::new();

            for i in 0..*nodes {
                let ports = vec![
                    NocPort {
                        direction: PortDirection::Local,
                        bandwidth_gbps: link_bw,
                        latency_cycles: 0,
                    },
                    NocPort {
                        direction: PortDirection::East,
                        bandwidth_gbps: link_bw,
                        latency_cycles: link_latency,
                    },
                    NocPort {
                        direction: PortDirection::West,
                        bandwidth_gbps: link_bw,
                        latency_cycles: link_latency,
                    },
                ];
                routers.push(NocRouter {
                    id: i,
                    x: i,
                    y: 0,
                    ports,
                });
                links.push(NocLink {
                    src_id: i,
                    dst_id: (i + 1) % nodes,
                    bandwidth_gbps: link_bw,
                });
            }

            let area = *nodes as f64 * 3.0 * 5000.0 * (config.data_width as f64 / 32.0);
            let diameter = nodes / 2;
            NocDesign {
                routers,
                links,
                total_area_um2: area,
                estimated_latency_cycles: diameter * link_latency + 1,
            }
        }
        NocTopology::Crossbar { ports } => {
            let mut routers = Vec::with_capacity(*ports as usize);
            let mut links = Vec::new();

            for i in 0..*ports {
                let r_ports = vec![NocPort {
                    direction: PortDirection::Local,
                    bandwidth_gbps: link_bw,
                    latency_cycles: 0,
                }];
                routers.push(NocRouter {
                    id: i,
                    x: i,
                    y: 0,
                    ports: r_ports,
                });
                for j in 0..*ports {
                    if i != j {
                        links.push(NocLink {
                            src_id: i,
                            dst_id: j,
                            bandwidth_gbps: link_bw,
                        });
                    }
                }
            }

            let area = (*ports as f64).powi(2) * 3000.0 * (config.data_width as f64 / 32.0);
            NocDesign {
                routers,
                links,
                total_area_um2: area,
                estimated_latency_cycles: 2,
            }
        }
        NocTopology::Tree { levels } => {
            let total_nodes = (1_u32 << levels) - 1;
            let mut routers = Vec::with_capacity(total_nodes as usize);
            let mut links = Vec::new();

            for i in 0..total_nodes {
                let level = (i + 1).ilog2();
                let ports = vec![
                    NocPort {
                        direction: PortDirection::Local,
                        bandwidth_gbps: link_bw,
                        latency_cycles: 0,
                    },
                    NocPort {
                        direction: PortDirection::North,
                        bandwidth_gbps: link_bw,
                        latency_cycles: link_latency,
                    },
                ];
                routers.push(NocRouter {
                    id: i,
                    x: i,
                    y: level,
                    ports,
                });
                if i > 0 {
                    let parent = (i - 1) / 2;
                    links.push(NocLink {
                        src_id: i,
                        dst_id: parent,
                        bandwidth_gbps: link_bw,
                    });
                }
            }

            let area = total_nodes as f64 * 2.0 * 5000.0 * (config.data_width as f64 / 32.0);
            NocDesign {
                routers,
                links,
                total_area_um2: area,
                estimated_latency_cycles: 2 * levels,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_router_count() {
        let config = NocConfig {
            topology: NocTopology::Mesh { rows: 4, cols: 4 },
            data_width: 64,
            flit_size: 128,
            routing: RoutingAlgorithm::XY,
        };
        let design = generate_noc(&config);
        assert_eq!(design.routers.len(), 16, "4x4 mesh should have 16 routers");
    }

    #[test]
    fn ring_has_n_routers() {
        let config = NocConfig {
            topology: NocTopology::Ring { nodes: 8 },
            data_width: 32,
            flit_size: 64,
            routing: RoutingAlgorithm::XY,
        };
        let design = generate_noc(&config);
        assert_eq!(design.routers.len(), 8);
        assert_eq!(design.links.len(), 8, "Ring should have N links");
    }
}
