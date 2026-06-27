/// GDSII stream format export — generates binary GDSII II layout data.
///
/// Reference: GDSII Stream Format Manual (Calma GDS II).
/// Record format: 2 bytes length + 1 byte record type + 1 byte data type + payload.
use serde::{Deserialize, Serialize};

// GDSII record type constants
pub const HEADER: u8 = 0x00;
pub const BGNLIB: u8 = 0x01;
pub const LIBNAME: u8 = 0x02;
pub const UNITS: u8 = 0x03;
pub const ENDLIB: u8 = 0x04;
pub const BGNSTR: u8 = 0x05;
pub const STRNAME: u8 = 0x06;
pub const ENDSTR: u8 = 0x07;
pub const BOUNDARY: u8 = 0x08;
pub const PATH: u8 = 0x09;
pub const SREF: u8 = 0x0A;
pub const TEXT: u8 = 0x0C;
pub const LAYER: u8 = 0x0D;
pub const DATATYPE: u8 = 0x0E;
pub const XY: u8 = 0x10;
pub const ENDEL: u8 = 0x11;
pub const SNAME: u8 = 0x12;
pub const STRING: u8 = 0x19;
pub const WIDTH: u8 = 0x0F;
pub const TEXTTYPE: u8 = 0x16;

// Data type constants
const DT_NONE: u8 = 0x00;
const DT_INT16: u8 = 0x01;
const DT_INT32: u8 = 0x03;
const DT_REAL8: u8 = 0x05;
const DT_ASCII: u8 = 0x06;

/// A GDSII element within a structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GdsiiElement {
    Boundary {
        layer: i16,
        datatype: i16,
        xy: Vec<(i32, i32)>,
    },
    Path {
        layer: i16,
        datatype: i16,
        width: i32,
        xy: Vec<(i32, i32)>,
    },
    SRef {
        sname: String,
        xy: (i32, i32),
    },
    Text {
        layer: i16,
        xy: (i32, i32),
        string: String,
    },
}

/// A named GDSII structure (cell).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdsiiStructure {
    pub name: String,
    pub elements: Vec<GdsiiElement>,
}

/// Top-level GDSII stream container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GdsiiStream {
    pub header: u16,
    pub structures: Vec<GdsiiStructure>,
}

/// Write a GDSII record to a byte buffer.
fn write_record(buf: &mut Vec<u8>, record_type: u8, data_type: u8, payload: &[u8]) {
    let total_len = (4 + payload.len()) as u16;
    buf.extend_from_slice(&total_len.to_be_bytes());
    buf.push(record_type);
    buf.push(data_type);
    buf.extend_from_slice(payload);
}

fn write_int16_record(buf: &mut Vec<u8>, record_type: u8, values: &[i16]) {
    let mut payload = Vec::new();
    for v in values {
        payload.extend_from_slice(&v.to_be_bytes());
    }
    write_record(buf, record_type, DT_INT16, &payload);
}

fn write_int32_record(buf: &mut Vec<u8>, record_type: u8, values: &[i32]) {
    let mut payload = Vec::new();
    for v in values {
        payload.extend_from_slice(&v.to_be_bytes());
    }
    write_record(buf, record_type, DT_INT32, &payload);
}

fn write_string_record(buf: &mut Vec<u8>, record_type: u8, s: &str) {
    let mut bytes = s.as_bytes().to_vec();
    // GDSII strings must be even length
    if bytes.len() % 2 != 0 {
        bytes.push(0);
    }
    write_record(buf, record_type, DT_ASCII, &bytes);
}

fn write_empty_record(buf: &mut Vec<u8>, record_type: u8) {
    write_record(buf, record_type, DT_NONE, &[]);
}

/// Convert a 64-bit float to GDSII 8-byte real format (excess-64 exponent).
fn f64_to_gdsii_real(val: f64) -> [u8; 8] {
    if val == 0.0 {
        return [0u8; 8];
    }
    let negative = val < 0.0;
    let mut mantissa = val.abs();
    let mut exponent: i32 = 64;

    // Normalize: 1/16 <= mantissa < 1 with base-16 exponent
    while mantissa >= 1.0 && exponent < 127 {
        mantissa /= 16.0;
        exponent += 1;
    }
    while mantissa < 1.0 / 16.0 && exponent > 0 {
        mantissa *= 16.0;
        exponent -= 1;
    }

    let mant_bits = (mantissa * (1u64 << 56) as f64) as u64;
    let mut bytes = mant_bits.to_be_bytes();
    bytes[0] = exponent as u8;
    if negative {
        bytes[0] |= 0x80;
    }
    bytes
}

fn write_real8_record(buf: &mut Vec<u8>, record_type: u8, values: &[f64]) {
    let mut payload = Vec::new();
    for v in values {
        payload.extend_from_slice(&f64_to_gdsii_real(*v));
    }
    write_record(buf, record_type, DT_REAL8, &payload);
}

/// Timestamp record payload (12 i16 values: year, month, day, hour, min, sec x2).
fn timestamp_payload() -> Vec<i16> {
    vec![2026, 4, 9, 0, 0, 0, 2026, 4, 9, 0, 0, 0]
}

/// Export GDSII structures to a valid binary GDSII stream.
pub fn export_gdsii(structures: &[GdsiiStructure]) -> Vec<u8> {
    let mut buf = Vec::new();

    // HEADER (version 600)
    write_int16_record(&mut buf, HEADER, &[600]);

    // BGNLIB (timestamps)
    let ts = timestamp_payload();
    write_int16_record(&mut buf, BGNLIB, &ts);

    // LIBNAME
    write_string_record(&mut buf, LIBNAME, "KAMI_PNR");

    // UNITS: user units per db unit, meters per db unit
    write_real8_record(&mut buf, UNITS, &[0.001, 1e-9]);

    for structure in structures {
        // BGNSTR
        let ts = timestamp_payload();
        write_int16_record(&mut buf, BGNSTR, &ts);
        write_string_record(&mut buf, STRNAME, &structure.name);

        for element in &structure.elements {
            match element {
                GdsiiElement::Boundary {
                    layer,
                    datatype,
                    xy,
                } => {
                    write_empty_record(&mut buf, BOUNDARY);
                    write_int16_record(&mut buf, LAYER, &[*layer]);
                    write_int16_record(&mut buf, DATATYPE, &[*datatype]);
                    let coords: Vec<i32> = xy.iter().flat_map(|(x, y)| [*x, *y]).collect();
                    write_int32_record(&mut buf, XY, &coords);
                    write_empty_record(&mut buf, ENDEL);
                }
                GdsiiElement::Path {
                    layer,
                    datatype,
                    width,
                    xy,
                } => {
                    write_empty_record(&mut buf, PATH);
                    write_int16_record(&mut buf, LAYER, &[*layer]);
                    write_int16_record(&mut buf, DATATYPE, &[*datatype]);
                    write_int32_record(&mut buf, WIDTH, &[*width]);
                    let coords: Vec<i32> = xy.iter().flat_map(|(x, y)| [*x, *y]).collect();
                    write_int32_record(&mut buf, XY, &coords);
                    write_empty_record(&mut buf, ENDEL);
                }
                GdsiiElement::SRef { sname, xy } => {
                    write_empty_record(&mut buf, SREF);
                    write_string_record(&mut buf, SNAME, sname);
                    write_int32_record(&mut buf, XY, &[xy.0, xy.1]);
                    write_empty_record(&mut buf, ENDEL);
                }
                GdsiiElement::Text { layer, xy, string } => {
                    write_empty_record(&mut buf, TEXT);
                    write_int16_record(&mut buf, LAYER, &[*layer]);
                    write_int16_record(&mut buf, TEXTTYPE, &[0]);
                    write_int32_record(&mut buf, XY, &[xy.0, xy.1]);
                    write_string_record(&mut buf, STRING, string);
                    write_empty_record(&mut buf, ENDEL);
                }
            }
        }

        write_empty_record(&mut buf, ENDSTR);
    }

    write_empty_record(&mut buf, ENDLIB);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gdsii_valid_header_bytes() {
        let structures = vec![GdsiiStructure {
            name: "TOP".into(),
            elements: vec![GdsiiElement::Boundary {
                layer: 1,
                datatype: 0,
                xy: vec![(0, 0), (1000, 0), (1000, 1000), (0, 1000), (0, 0)],
            }],
        }];

        let bytes = export_gdsii(&structures);
        assert!(bytes.len() > 20);

        // First record: HEADER. Length = 6 (4 header + 2 bytes for version 600)
        assert_eq!(bytes[0], 0x00); // high byte of length
        assert_eq!(bytes[1], 0x06); // low byte = 6
        assert_eq!(bytes[2], HEADER);
        // Version 600 = 0x0258
        assert_eq!(bytes[4], 0x02);
        assert_eq!(bytes[5], 0x58);
    }

    #[test]
    fn gdsii_contains_endlib() {
        let bytes = export_gdsii(&[]);
        // Last record should be ENDLIB (4 bytes: len=4, type=ENDLIB, dt=0)
        let len = bytes.len();
        assert!(len >= 4);
        assert_eq!(bytes[len - 4], 0x00);
        assert_eq!(bytes[len - 3], 0x04); // length = 4
        assert_eq!(bytes[len - 2], ENDLIB);
    }
}
