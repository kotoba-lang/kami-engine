//! GLB binary container parse/write (zero-copy on read).

use crate::VrmError;

/// GLB magic: "glTF" in little-endian.
const GLB_MAGIC: u32 = 0x46546C67;
/// GLB version 2.
const GLB_VERSION: u32 = 2;
/// JSON chunk type.
const CHUNK_JSON: u32 = 0x4E4F534A;
/// BIN chunk type.
const CHUNK_BIN: u32 = 0x004E4942;

/// Parsed GLB container (JSON chunk + BIN chunk).
pub struct GlbChunks<'a> {
    /// JSON chunk bytes (UTF-8).
    pub json: &'a [u8],
    /// BIN chunk bytes (vertex/index/image data).
    pub bin: Option<&'a [u8]>,
}

/// Parse raw GLB bytes into JSON and BIN chunks (zero-copy slicing).
pub fn parse_glb(data: &[u8]) -> Result<GlbChunks<'_>, VrmError> {
    if data.len() < 12 {
        return Err(VrmError::InvalidGlb("too short for GLB header"));
    }

    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if magic != GLB_MAGIC {
        return Err(VrmError::InvalidGlb("invalid magic"));
    }

    let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    if version != GLB_VERSION {
        return Err(VrmError::InvalidGlb("unsupported GLB version"));
    }

    let total_len = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    if data.len() < total_len {
        return Err(VrmError::InvalidGlb("data shorter than declared length"));
    }

    // Parse JSON chunk (required)
    if data.len() < 20 {
        return Err(VrmError::InvalidGlb("too short for JSON chunk header"));
    }
    let json_len = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;
    let json_type = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
    if json_type != CHUNK_JSON {
        return Err(VrmError::InvalidGlb("first chunk is not JSON"));
    }
    let json_end = 20 + json_len;
    if data.len() < json_end {
        return Err(VrmError::InvalidGlb("JSON chunk truncated"));
    }
    let json = &data[20..json_end];

    // Parse BIN chunk (optional)
    let bin = if data.len() >= json_end + 8 {
        let bin_len = u32::from_le_bytes([
            data[json_end],
            data[json_end + 1],
            data[json_end + 2],
            data[json_end + 3],
        ]) as usize;
        let bin_type = u32::from_le_bytes([
            data[json_end + 4],
            data[json_end + 5],
            data[json_end + 6],
            data[json_end + 7],
        ]);
        if bin_type == CHUNK_BIN {
            let bin_start = json_end + 8;
            let bin_end = bin_start + bin_len;
            if data.len() >= bin_end {
                Some(&data[bin_start..bin_end])
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    Ok(GlbChunks { json, bin })
}

/// Write GLB from JSON bytes + binary buffer.
pub fn write_glb(json: &[u8], bin: &[u8]) -> Vec<u8> {
    // Pad JSON to 4-byte alignment (space padding per glTF spec)
    let json_pad = (4 - (json.len() % 4)) % 4;
    let json_chunk_len = json.len() + json_pad;

    // Pad BIN to 4-byte alignment (zero padding)
    let bin_pad = (4 - (bin.len() % 4)) % 4;
    let bin_chunk_len = bin.len() + bin_pad;

    let total_len = 12 + 8 + json_chunk_len + 8 + bin_chunk_len;
    let mut glb = Vec::with_capacity(total_len);

    // Header
    glb.extend_from_slice(&GLB_MAGIC.to_le_bytes());
    glb.extend_from_slice(&GLB_VERSION.to_le_bytes());
    glb.extend_from_slice(&(total_len as u32).to_le_bytes());

    // JSON chunk
    glb.extend_from_slice(&(json_chunk_len as u32).to_le_bytes());
    glb.extend_from_slice(&CHUNK_JSON.to_le_bytes());
    glb.extend_from_slice(json);
    glb.extend(std::iter::repeat(b' ').take(json_pad));

    // BIN chunk
    glb.extend_from_slice(&(bin_chunk_len as u32).to_le_bytes());
    glb.extend_from_slice(&CHUNK_BIN.to_le_bytes());
    glb.extend_from_slice(bin);
    glb.extend(std::iter::repeat(0u8).take(bin_pad));

    glb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let json = br#"{"asset":{"version":"2.0"}}"#;
        let bin = &[1u8, 2, 3, 4, 5, 6, 7];
        let glb = write_glb(json, bin);

        // Verify header
        assert_eq!(u32::from_le_bytes(glb[0..4].try_into().unwrap()), GLB_MAGIC);
        assert_eq!(
            u32::from_le_bytes(glb[4..8].try_into().unwrap()),
            GLB_VERSION
        );
        let total = u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize;
        assert_eq!(total, glb.len());

        // Parse back
        let chunks = parse_glb(&glb).unwrap();
        // JSON may have trailing spaces from padding
        let parsed_json = std::str::from_utf8(chunks.json).unwrap().trim_end();
        assert_eq!(parsed_json, std::str::from_utf8(json).unwrap());

        let parsed_bin = chunks.bin.unwrap();
        assert!(parsed_bin.starts_with(bin));
    }

    #[test]
    fn invalid_magic() {
        let data = vec![0u8; 20];
        assert!(parse_glb(&data).is_err());
    }

    #[test]
    fn too_short() {
        assert!(parse_glb(&[]).is_err());
        assert!(parse_glb(&[0; 8]).is_err());
    }
}
