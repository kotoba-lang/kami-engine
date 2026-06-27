/// Bus protocol signal definitions and RTL generation (AXI4, APB).
use serde::{Deserialize, Serialize};

/// AXI4 bus configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxiConfig {
    /// Address bus width in bits.
    pub addr_width: u32,
    /// Data bus width in bits.
    pub data_width: u32,
    /// Transaction ID width in bits.
    pub id_width: u32,
    /// User signal width in bits (0 to disable).
    pub user_width: u32,
}

/// APB bus configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApbConfig {
    /// Address bus width in bits.
    pub addr_width: u32,
    /// Data bus width in bits.
    pub data_width: u32,
}

/// AXI4 signal names grouped by channel.
pub const AXI4_SIGNALS: &[&str] = &[
    // Write address channel
    "AWVALID", "AWREADY", "AWADDR", "AWLEN", "AWSIZE", "AWBURST", "AWID", "AWLOCK", "AWCACHE",
    "AWPROT", "AWQOS", // Write data channel
    "WVALID", "WREADY", "WDATA", "WSTRB", "WLAST", // Write response channel
    "BVALID", "BREADY", "BRESP", "BID", // Read address channel
    "ARVALID", "ARREADY", "ARADDR", "ARLEN", "ARSIZE", "ARBURST", "ARID", "ARLOCK", "ARCACHE",
    "ARPROT", "ARQOS", // Read data channel
    "RVALID", "RREADY", "RDATA", "RRESP", "RLAST", "RID",
];

/// Generate a Verilog AXI4 master port list.
pub fn generate_axi4_master(config: &AxiConfig) -> String {
    let strb_width = config.data_width / 8;
    let mut v = String::with_capacity(2048);
    v.push_str(&format!(
        "// AXI4 Master — addr={}, data={}, id={}\n",
        config.addr_width, config.data_width, config.id_width
    ));
    v.push_str("module axi4_master (\n");
    v.push_str("  input  wire        ACLK,\n");
    v.push_str("  input  wire        ARESETn,\n");
    // Write address
    v.push_str(&format!(
        "  output wire [{w}:0] AWADDR,\n",
        w = config.addr_width - 1
    ));
    v.push_str(&format!(
        "  output wire [{w}:0] AWID,\n",
        w = config.id_width - 1
    ));
    v.push_str("  output wire [7:0]  AWLEN,\n");
    v.push_str("  output wire [2:0]  AWSIZE,\n");
    v.push_str("  output wire [1:0]  AWBURST,\n");
    v.push_str("  output wire        AWVALID,\n");
    v.push_str("  input  wire        AWREADY,\n");
    // Write data
    v.push_str(&format!(
        "  output wire [{w}:0] WDATA,\n",
        w = config.data_width - 1
    ));
    v.push_str(&format!(
        "  output wire [{w}:0] WSTRB,\n",
        w = strb_width - 1
    ));
    v.push_str("  output wire        WLAST,\n");
    v.push_str("  output wire        WVALID,\n");
    v.push_str("  input  wire        WREADY,\n");
    // Write response
    v.push_str("  input  wire [1:0]  BRESP,\n");
    v.push_str(&format!(
        "  input  wire [{w}:0] BID,\n",
        w = config.id_width - 1
    ));
    v.push_str("  input  wire        BVALID,\n");
    v.push_str("  output wire        BREADY,\n");
    // Read address
    v.push_str(&format!(
        "  output wire [{w}:0] ARADDR,\n",
        w = config.addr_width - 1
    ));
    v.push_str(&format!(
        "  output wire [{w}:0] ARID,\n",
        w = config.id_width - 1
    ));
    v.push_str("  output wire [7:0]  ARLEN,\n");
    v.push_str("  output wire [2:0]  ARSIZE,\n");
    v.push_str("  output wire [1:0]  ARBURST,\n");
    v.push_str("  output wire        ARVALID,\n");
    v.push_str("  input  wire        ARREADY,\n");
    // Read data
    v.push_str(&format!(
        "  input  wire [{w}:0] RDATA,\n",
        w = config.data_width - 1
    ));
    v.push_str("  input  wire [1:0]  RRESP,\n");
    v.push_str("  input  wire        RLAST,\n");
    v.push_str(&format!(
        "  input  wire [{w}:0] RID,\n",
        w = config.id_width - 1
    ));
    v.push_str("  input  wire        RVALID,\n");
    v.push_str("  output wire        RREADY\n");
    v.push_str(");\n");
    v.push_str("  // Master logic placeholder\n");
    v.push_str("endmodule\n");
    v
}

/// Generate a Verilog AXI4 slave port list.
pub fn generate_axi4_slave(config: &AxiConfig) -> String {
    let strb_width = config.data_width / 8;
    let mut v = String::with_capacity(2048);
    v.push_str(&format!(
        "// AXI4 Slave — addr={}, data={}, id={}\n",
        config.addr_width, config.data_width, config.id_width
    ));
    v.push_str("module axi4_slave (\n");
    v.push_str("  input  wire        ACLK,\n");
    v.push_str("  input  wire        ARESETn,\n");
    // Write address (inputs for slave)
    v.push_str(&format!(
        "  input  wire [{w}:0] AWADDR,\n",
        w = config.addr_width - 1
    ));
    v.push_str(&format!(
        "  input  wire [{w}:0] AWID,\n",
        w = config.id_width - 1
    ));
    v.push_str("  input  wire [7:0]  AWLEN,\n");
    v.push_str("  input  wire [2:0]  AWSIZE,\n");
    v.push_str("  input  wire [1:0]  AWBURST,\n");
    v.push_str("  input  wire        AWVALID,\n");
    v.push_str("  output wire        AWREADY,\n");
    // Write data
    v.push_str(&format!(
        "  input  wire [{w}:0] WDATA,\n",
        w = config.data_width - 1
    ));
    v.push_str(&format!(
        "  input  wire [{w}:0] WSTRB,\n",
        w = strb_width - 1
    ));
    v.push_str("  input  wire        WLAST,\n");
    v.push_str("  input  wire        WVALID,\n");
    v.push_str("  output wire        WREADY,\n");
    // Write response
    v.push_str("  output wire [1:0]  BRESP,\n");
    v.push_str(&format!(
        "  output wire [{w}:0] BID,\n",
        w = config.id_width - 1
    ));
    v.push_str("  output wire        BVALID,\n");
    v.push_str("  input  wire        BREADY,\n");
    // Read address
    v.push_str(&format!(
        "  input  wire [{w}:0] ARADDR,\n",
        w = config.addr_width - 1
    ));
    v.push_str(&format!(
        "  input  wire [{w}:0] ARID,\n",
        w = config.id_width - 1
    ));
    v.push_str("  input  wire [7:0]  ARLEN,\n");
    v.push_str("  input  wire [2:0]  ARSIZE,\n");
    v.push_str("  input  wire [1:0]  ARBURST,\n");
    v.push_str("  input  wire        ARVALID,\n");
    v.push_str("  output wire        ARREADY,\n");
    // Read data
    v.push_str(&format!(
        "  output wire [{w}:0] RDATA,\n",
        w = config.data_width - 1
    ));
    v.push_str("  output wire [1:0]  RRESP,\n");
    v.push_str("  output wire        RLAST,\n");
    v.push_str(&format!(
        "  output wire [{w}:0] RID,\n",
        w = config.id_width - 1
    ));
    v.push_str("  output wire        RVALID,\n");
    v.push_str("  input  wire        RREADY\n");
    v.push_str(");\n");
    v.push_str("  // Slave logic placeholder\n");
    v.push_str("endmodule\n");
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axi4_signal_count() {
        // AXI4 has 37 signal groups (some multi-bit).
        assert!(
            AXI4_SIGNALS.len() >= 35,
            "AXI4 should have 35+ signals, got {}",
            AXI4_SIGNALS.len()
        );
    }

    #[test]
    fn axi4_master_contains_signals() {
        let config = AxiConfig {
            addr_width: 32,
            data_width: 64,
            id_width: 4,
            user_width: 0,
        };
        let rtl = generate_axi4_master(&config);
        assert!(rtl.contains("AWVALID"));
        assert!(rtl.contains("AWREADY"));
        assert!(rtl.contains("[31:0] AWADDR"));
        assert!(rtl.contains("[63:0] WDATA"));
    }
}
