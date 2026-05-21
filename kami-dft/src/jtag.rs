/// JTAG / Boundary Scan — IEEE 1149.1 TAP controller modeling and BSDL generation.

use serde::{Deserialize, Serialize};

/// Standard JTAG instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JtagInstruction {
    Bypass,
    Extest,
    SamplePreload,
    Idcode,
    BoundaryScan,
    UserDefined(u8),
}

impl JtagInstruction {
    /// Instruction name for BSDL output.
    pub fn name(&self) -> String {
        match self {
            JtagInstruction::Bypass => "BYPASS".into(),
            JtagInstruction::Extest => "EXTEST".into(),
            JtagInstruction::SamplePreload => "SAMPLE".into(),
            JtagInstruction::Idcode => "IDCODE".into(),
            JtagInstruction::BoundaryScan => "BOUNDARY_SCAN".into(),
            JtagInstruction::UserDefined(n) => format!("USER_{n}"),
        }
    }

    /// Instruction opcode (simplified: sequential assignment).
    pub fn opcode(&self, ir_len: u8) -> String {
        let code = match self {
            JtagInstruction::Bypass => (1u32 << ir_len) - 1, // All 1s
            JtagInstruction::Extest => 0,
            JtagInstruction::SamplePreload => 1,
            JtagInstruction::Idcode => 2,
            JtagInstruction::BoundaryScan => 3,
            JtagInstruction::UserDefined(n) => *n as u32 + 4,
        };
        format!("{:0>width$b}", code, width = ir_len as usize)
    }
}

/// Boundary scan cell type per IEEE 1149.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CellType {
    BC1,
    BC2,
    BC4,
    BC7,
}

impl CellType {
    pub fn bsdl_name(&self) -> &str {
        match self {
            CellType::BC1 => "BC_1",
            CellType::BC2 => "BC_2",
            CellType::BC4 => "BC_4",
            CellType::BC7 => "BC_7",
        }
    }
}

/// A single boundary scan register cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryScanCell {
    pub pin_name: String,
    pub cell_type: CellType,
    pub control_cell: Option<usize>,
}

/// A BSDL device description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BsdlDevice {
    pub name: String,
    pub instruction_length: u8,
    pub instructions: Vec<JtagInstruction>,
    pub boundary_register: Vec<BoundaryScanCell>,
    pub idcode: u32,
}

/// Generate a BSDL (Boundary Scan Description Language) file for the device.
pub fn generate_bsdl(device: &BsdlDevice) -> String {
    let mut out = String::new();

    out.push_str(&format!("-- BSDL file for {}\n", device.name));
    out.push_str(&format!("entity {} is\n\n", device.name));

    // Generic parameter
    out.push_str("  generic (PHYSICAL_PIN_MAP : string := \"DEFAULT\");\n\n");

    // Port list (from boundary register pins)
    out.push_str("  port (\n");
    out.push_str("    TDI   : in  bit;\n");
    out.push_str("    TDO   : out bit;\n");
    out.push_str("    TMS   : in  bit;\n");
    out.push_str("    TCK   : in  bit;\n");
    for (i, cell) in device.boundary_register.iter().enumerate() {
        let sep = if i + 1 < device.boundary_register.len() { ";" } else { "" };
        out.push_str(&format!("    {}  : inout bit{}\n", cell.pin_name, sep));
    }
    out.push_str("  );\n\n");

    // Use statements
    out.push_str("  use STD_1149_1_2001.all;\n\n");

    // Instruction register
    out.push_str(&format!(
        "  attribute INSTRUCTION_LENGTH of {} : entity is {};\n",
        device.name, device.instruction_length
    ));

    // Instruction opcodes
    out.push_str(&format!(
        "  attribute INSTRUCTION_OPCODE of {} : entity is\n",
        device.name
    ));
    for (i, instr) in device.instructions.iter().enumerate() {
        let sep = if i + 1 < device.instructions.len() { " &" } else { "" };
        out.push_str(&format!(
            "    \"{} ({})\"{}",
            instr.name(),
            instr.opcode(device.instruction_length),
            sep,
        ));
        out.push('\n');
    }
    out.push_str("  ;\n\n");

    // IDCODE register
    out.push_str(&format!(
        "  attribute IDCODE_REGISTER of {} : entity is\n",
        device.name
    ));
    out.push_str(&format!(
        "    \"{:032b}\";\n\n",
        device.idcode
    ));

    // Boundary register
    out.push_str(&format!(
        "  attribute BOUNDARY_LENGTH of {} : entity is {};\n",
        device.name,
        device.boundary_register.len()
    ));
    out.push_str(&format!(
        "  attribute BOUNDARY_REGISTER of {} : entity is\n",
        device.name
    ));
    for (i, cell) in device.boundary_register.iter().enumerate() {
        let ctrl = match cell.control_cell {
            Some(c) => format!("{c}"),
            None => "X".into(),
        };
        let sep = if i + 1 < device.boundary_register.len() { "," } else { "" };
        out.push_str(&format!(
            "    \"{}  ({}, {}, input, X, {}, 0, Z)\"{}\n",
            i, cell.cell_type.bsdl_name(), cell.pin_name, ctrl, sep
        ));
    }
    out.push_str("  ;\n\n");

    out.push_str(&format!("end {};\n", device.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_device() -> BsdlDevice {
        BsdlDevice {
            name: "KAMI_CHIP".into(),
            instruction_length: 4,
            instructions: vec![
                JtagInstruction::Bypass,
                JtagInstruction::Extest,
                JtagInstruction::SamplePreload,
                JtagInstruction::Idcode,
            ],
            boundary_register: vec![
                BoundaryScanCell { pin_name: "PA0".into(), cell_type: CellType::BC1, control_cell: None },
                BoundaryScanCell { pin_name: "PA1".into(), cell_type: CellType::BC1, control_cell: Some(0) },
            ],
            idcode: 0x0491_A03F,
        }
    }

    #[test]
    fn bsdl_contains_idcode() {
        let bsdl = generate_bsdl(&sample_device());
        assert!(bsdl.contains("IDCODE_REGISTER"));
        assert!(bsdl.contains("IDCODE"));
        // Check the binary IDCODE is present
        assert!(bsdl.contains(&format!("{:032b}", 0x0491_A03Fu32)));
    }

    #[test]
    fn bsdl_contains_entity() {
        let bsdl = generate_bsdl(&sample_device());
        assert!(bsdl.contains("entity KAMI_CHIP is"));
        assert!(bsdl.contains("end KAMI_CHIP;"));
    }

    #[test]
    fn bsdl_contains_boundary_register() {
        let bsdl = generate_bsdl(&sample_device());
        assert!(bsdl.contains("BOUNDARY_REGISTER"));
        assert!(bsdl.contains("BC_1"));
        assert!(bsdl.contains("PA0"));
        assert!(bsdl.contains("PA1"));
    }

    #[test]
    fn instruction_opcodes() {
        let dev = sample_device();
        assert_eq!(JtagInstruction::Bypass.opcode(4), "1111");
        assert_eq!(JtagInstruction::Extest.opcode(4), "0000");
        assert_eq!(JtagInstruction::Idcode.opcode(dev.instruction_length), "0010");
    }
}
