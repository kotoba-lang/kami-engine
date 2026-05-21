//! kami-eng-io: Engineering file format I/O.
//!
//! Parsers and exporters for industry-standard engineering file formats.
//! Each format module provides read/write functions operating on byte slices.

/// Supported engineering file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    // CAD formats
    StepAp203,
    StepAp214,
    Iges,
    Stl,
    StlBinary,
    Obj,
    GltfJson,
    Glb,
    // EDA formats
    GerberRs274x,
    ExcellonDrill,
    OdbPlusPlus,
    EdifNetlist,
    SpiceNetlist,
    // RTL formats
    Verilog,
    Vhdl,
    SystemVerilog,
    Vcd,
    LibertyTiming,
    LefDef,
    Sdf,
    // CAM formats
    Gcode,
}

impl FileFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::StepAp203 | Self::StepAp214 => "step",
            Self::Iges => "igs",
            Self::Stl => "stl",
            Self::StlBinary => "stl",
            Self::Obj => "obj",
            Self::GltfJson => "gltf",
            Self::Glb => "glb",
            Self::GerberRs274x => "gbr",
            Self::ExcellonDrill => "drl",
            Self::OdbPlusPlus => "tgz",
            Self::EdifNetlist => "edf",
            Self::SpiceNetlist => "cir",
            Self::Verilog => "v",
            Self::Vhdl => "vhd",
            Self::SystemVerilog => "sv",
            Self::Vcd => "vcd",
            Self::LibertyTiming => "lib",
            Self::LefDef => "lef",
            Self::Sdf => "sdf",
            Self::Gcode => "nc",
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::StepAp203 | Self::StepAp214 => "model/step",
            Self::Iges => "model/iges",
            Self::Stl | Self::StlBinary => "model/stl",
            Self::Obj => "model/obj",
            Self::GltfJson => "model/gltf+json",
            Self::Glb => "model/gltf-binary",
            Self::GerberRs274x => "application/x-gerber",
            Self::ExcellonDrill => "application/x-excellon",
            _ => "application/octet-stream",
        }
    }

    /// Detect format from file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "step" | "stp" => Some(Self::StepAp214),
            "igs" | "iges" => Some(Self::Iges),
            "stl" => Some(Self::Stl),
            "obj" => Some(Self::Obj),
            "gltf" => Some(Self::GltfJson),
            "glb" => Some(Self::Glb),
            "gbr" | "gtl" | "gbl" | "gts" | "gbs" | "gto" | "gbo" => Some(Self::GerberRs274x),
            "drl" | "xln" => Some(Self::ExcellonDrill),
            "edf" | "edif" => Some(Self::EdifNetlist),
            "cir" | "spice" | "sp" => Some(Self::SpiceNetlist),
            "v" => Some(Self::Verilog),
            "vhd" | "vhdl" => Some(Self::Vhdl),
            "sv" => Some(Self::SystemVerilog),
            "vcd" => Some(Self::Vcd),
            "lib" => Some(Self::LibertyTiming),
            "lef" | "def" => Some(Self::LefDef),
            "sdf" => Some(Self::Sdf),
            "nc" | "ngc" | "gcode" | "tap" => Some(Self::Gcode),
            _ => None,
        }
    }
}

/// STL ASCII exporter.
pub mod stl {
    use glam::Vec3;

    /// Triangle with normal.
    pub struct StlTriangle {
        pub normal: Vec3,
        pub v0: Vec3,
        pub v1: Vec3,
        pub v2: Vec3,
    }

    /// Export triangles to ASCII STL.
    pub fn export_ascii(name: &str, triangles: &[StlTriangle]) -> String {
        let mut s = format!("solid {}\n", name);
        for tri in triangles {
            s.push_str(&format!(
                "  facet normal {} {} {}\n    outer loop\n      vertex {} {} {}\n      vertex {} {} {}\n      vertex {} {} {}\n    endloop\n  endfacet\n",
                tri.normal.x, tri.normal.y, tri.normal.z,
                tri.v0.x, tri.v0.y, tri.v0.z,
                tri.v1.x, tri.v1.y, tri.v1.z,
                tri.v2.x, tri.v2.y, tri.v2.z,
            ));
        }
        s.push_str(&format!("endsolid {}\n", name));
        s
    }

    /// Export triangles to binary STL.
    pub fn export_binary(triangles: &[StlTriangle]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(84 + triangles.len() * 50);
        // 80-byte header
        buf.extend_from_slice(&[0u8; 80]);
        // triangle count
        buf.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
        for tri in triangles {
            for v in [tri.normal, tri.v0, tri.v1, tri.v2] {
                buf.extend_from_slice(&v.x.to_le_bytes());
                buf.extend_from_slice(&v.y.to_le_bytes());
                buf.extend_from_slice(&v.z.to_le_bytes());
            }
            buf.extend_from_slice(&[0u8; 2]); // attribute byte count
        }
        buf
    }
}

/// Gerber RS-274X generator.
pub mod gerber {
    /// Aperture definition.
    #[derive(Debug, Clone)]
    pub enum Aperture {
        Circle { diameter: f64 },
        Rectangle { width: f64, height: f64 },
        Obround { width: f64, height: f64 },
    }

    /// Gerber command.
    #[derive(Debug, Clone)]
    pub enum GerberCmd {
        /// Set aperture: D10, D11, ...
        SelectAperture(u32),
        /// Move without drawing.
        MoveTo { x: f64, y: f64 },
        /// Linear interpolation (draw).
        LineTo { x: f64, y: f64 },
        /// Flash pad at position.
        Flash { x: f64, y: f64 },
        /// Arc (clockwise).
        ArcCW { x: f64, y: f64, i: f64, j: f64 },
        /// Arc (counter-clockwise).
        ArcCCW { x: f64, y: f64, i: f64, j: f64 },
    }

    /// Generate RS-274X Gerber file content.
    pub fn generate(apertures: &[(u32, Aperture)], commands: &[GerberCmd]) -> String {
        let mut s = String::new();
        // Header
        s.push_str("%FSLAX36Y36*%\n"); // format: leading zeros, absolute, 3.6
        s.push_str("%MOIN*%\n");        // units: mm
        s.push_str("%IPPOS*%\n");       // image polarity: positive

        // Aperture definitions
        for (id, ap) in apertures {
            match ap {
                Aperture::Circle { diameter } => {
                    s.push_str(&format!("%ADD{}C,{:.6}*%\n", id, diameter));
                }
                Aperture::Rectangle { width, height } => {
                    s.push_str(&format!("%ADD{}R,{:.6}X{:.6}*%\n", id, width, height));
                }
                Aperture::Obround { width, height } => {
                    s.push_str(&format!("%ADD{}O,{:.6}X{:.6}*%\n", id, width, height));
                }
            }
        }

        // Commands
        for cmd in commands {
            match cmd {
                GerberCmd::SelectAperture(id) => s.push_str(&format!("D{}*\n", id)),
                GerberCmd::MoveTo { x, y } => {
                    s.push_str(&format!("X{}Y{}D02*\n", coord(*x), coord(*y)));
                }
                GerberCmd::LineTo { x, y } => {
                    s.push_str("G01*\n");
                    s.push_str(&format!("X{}Y{}D01*\n", coord(*x), coord(*y)));
                }
                GerberCmd::Flash { x, y } => {
                    s.push_str(&format!("X{}Y{}D03*\n", coord(*x), coord(*y)));
                }
                GerberCmd::ArcCW { x, y, i, j } => {
                    s.push_str("G02*\n");
                    s.push_str(&format!("X{}Y{}I{}J{}D01*\n", coord(*x), coord(*y), coord(*i), coord(*j)));
                }
                GerberCmd::ArcCCW { x, y, i, j } => {
                    s.push_str("G03*\n");
                    s.push_str(&format!("X{}Y{}I{}J{}D01*\n", coord(*x), coord(*y), coord(*i), coord(*j)));
                }
            }
        }

        s.push_str("M02*\n"); // end of file
        s
    }

    fn coord(val: f64) -> i64 {
        (val * 1_000_000.0) as i64
    }
}

/// STEP file header generator (AP214 subset).
pub mod step {
    /// Generate minimal STEP AP214 header.
    pub fn generate_header(filename: &str, author: &str) -> String {
        format!(
            "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('KAMI Engineering SDK export'), '2;1');\nFILE_NAME('{}', '2026-04-09', ('{}'), ('GFTD'), 'KAMI-ENG-SDK', 'kami-cad', '');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\n",
            filename, author
        )
    }

    pub fn generate_footer() -> &'static str {
        "ENDSEC;\nEND-ISO-10303-21;\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn format_detection() {
        assert_eq!(FileFormat::from_extension("step"), Some(FileFormat::StepAp214));
        assert_eq!(FileFormat::from_extension("v"), Some(FileFormat::Verilog));
        assert_eq!(FileFormat::from_extension("gbr"), Some(FileFormat::GerberRs274x));
        assert_eq!(FileFormat::from_extension("nc"), Some(FileFormat::Gcode));
        assert!(FileFormat::from_extension("xyz").is_none());
    }

    #[test]
    fn stl_ascii_export() {
        let tris = vec![stl::StlTriangle {
            normal: Vec3::Z,
            v0: Vec3::ZERO,
            v1: Vec3::X,
            v2: Vec3::Y,
        }];
        let out = stl::export_ascii("test", &tris);
        assert!(out.starts_with("solid test"));
        assert!(out.contains("facet normal"));
        assert!(out.ends_with("endsolid test\n"));
    }

    #[test]
    fn stl_binary_export() {
        let tris = vec![stl::StlTriangle {
            normal: Vec3::Z,
            v0: Vec3::ZERO,
            v1: Vec3::X,
            v2: Vec3::Y,
        }];
        let buf = stl::export_binary(&tris);
        assert_eq!(buf.len(), 84 + 50); // header(80) + count(4) + 1 tri(50)
        let count = u32::from_le_bytes([buf[80], buf[81], buf[82], buf[83]]);
        assert_eq!(count, 1);
    }

    #[test]
    fn gerber_generation() {
        let apertures = vec![
            (10, gerber::Aperture::Circle { diameter: 0.2 }),
        ];
        let cmds = vec![
            gerber::GerberCmd::SelectAperture(10),
            gerber::GerberCmd::Flash { x: 1.0, y: 2.0 },
        ];
        let out = gerber::generate(&apertures, &cmds);
        assert!(out.contains("%FSLAX36Y36*%"));
        assert!(out.contains("%ADD10C,"));
        assert!(out.contains("D10*"));
        assert!(out.contains("D03*"));
        assert!(out.contains("M02*"));
    }

    #[test]
    fn step_header() {
        let h = step::generate_header("part.step", "Engineer");
        assert!(h.contains("ISO-10303-21"));
        assert!(h.contains("KAMI-ENG-SDK"));
        assert!(h.contains("Engineer"));
    }
}
