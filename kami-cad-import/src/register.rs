//! SBOM registration request builder for `sbom.gftd.ai`.
//!
//! This crate stays runtime-agnostic — it does not bring in an HTTP
//! client. Instead, `register_request` produces a `RegisterRequest`
//! struct describing the call envelope (URL + headers + JSON body).
//! The caller — a CLI tool, a `gftd build` hook, or a Worker — performs
//! the actual POST.
//!
//! Forward-compatible Lexicon (defined in
//! `00-contracts/lexicons/ai/gftd/apps/sbom/registerArtifact.json`):
//!
//! ```text
//! POST https://atproto.gftd.ai/xrpc/ai.gftd.sbom.registerArtifact
//! Authorization: Bearer <Service Auth JWT, lxm=ai.gftd.sbom.registerArtifact>
//! Content-Type: application/json
//!
//! {
//!   "format": "CycloneDX",
//!   "specVersion": "1.5",
//!   "vehicleId": "<asm.vehicle_id>",
//!   "vehicleRevision": "<asm.revision>",
//!   "totalMassKg": <number>,
//!   "partCount": <int>,
//!   "sourceUri": "<asm.source.uri>",
//!   "sourceSha256": "<hex>",
//!   "license": "<spdx>",
//!   "cdxJson": "<full CycloneDX 1.5 document, JSON string>"
//! }
//! ```
//!
//! sbom.gftd.ai writes this to its `SbomArtifact` graph + ingests every
//! `components[]` entry into `SbomComponent`. CVE / recall pipeline
//! (already documented in `60-apps/ai-gftd-project-sbom/CLAUDE.md`)
//! takes over from there — zero new infrastructure on the consumer side.

use crate::part::{AssemblyError, VehicleAssembly};
use crate::sbom;

/// Default endpoint. Vehicles registered through this URL flow into
/// the same `sbom.gftd.ai` SbomArtifact graph as software SBOMs from
/// `cargo-cyclonedx` etc. The host is `atproto.gftd.ai` (sole XRPC
/// gateway per Layer 2 routing — see ADR-2604231828).
pub const DEFAULT_ENDPOINT: &str =
    "https://atproto.gftd.ai/xrpc/ai.gftd.sbom.registerArtifact";

#[derive(Debug, Clone)]
pub struct RegisterRequest {
    pub url: String,
    pub method: &'static str,
    pub headers: Vec<(String, String)>,
    /// JSON body, ready to POST.
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct RegisterOptions {
    /// Override `DEFAULT_ENDPOINT` (e.g. for staging or self-hosted PDS).
    pub endpoint: Option<String>,
    /// Service Auth JWT — the caller mints this with
    /// `gftd agent-token --lxm ai.gftd.sbom.registerArtifact`.
    /// Leave `None` if the caller adds the Authorization header itself.
    pub bearer_token: Option<String>,
    /// Override the SBOM serial number (defaults to deterministic UUID
    /// from `assembly.source.sha256`, matching `sbom::emit`).
    pub serial_number: Option<String>,
    /// Override the SBOM timestamp.
    pub timestamp: Option<String>,
}

impl Default for RegisterOptions {
    fn default() -> Self {
        Self {
            endpoint: None,
            bearer_token: None,
            serial_number: None,
            timestamp: None,
        }
    }
}

#[derive(serde::Serialize)]
struct RegisterBody<'a> {
    format: &'static str,
    #[serde(rename = "specVersion")]
    spec_version: &'static str,
    #[serde(rename = "vehicleId")]
    vehicle_id: &'a str,
    #[serde(rename = "vehicleRevision")]
    vehicle_revision: &'a str,
    #[serde(rename = "totalMassKg")]
    total_mass_kg: f32,
    #[serde(rename = "partCount")]
    part_count: u32,
    #[serde(rename = "sourceUri")]
    source_uri: &'a str,
    #[serde(rename = "sourceSha256")]
    source_sha256: &'a str,
    license: &'a str,
    #[serde(rename = "cdxJson")]
    cdx_json: String,
}

/// Build a `RegisterRequest` for the given assembly. The returned
/// envelope is byte-for-byte stable across runs (deterministic SBOM
/// serial number) — diffable across builds.
pub fn register_request(
    asm: &VehicleAssembly,
    opts: &RegisterOptions,
) -> Result<RegisterRequest, AssemblyError> {
    let cdx = sbom::emit(
        asm,
        &sbom::CycloneDxOptions {
            serial_number: opts.serial_number.clone(),
            timestamp: opts.timestamp.clone(),
        },
    )?;
    let body = RegisterBody {
        format: "CycloneDX",
        spec_version: "1.5",
        vehicle_id: &asm.vehicle_id,
        vehicle_revision: &asm.revision,
        total_mass_kg: asm.total_mass_kg(),
        part_count: asm.parts.len() as u32,
        source_uri: &asm.source.uri,
        source_sha256: &asm.source.sha256,
        license: &asm.source.license,
        cdx_json: cdx,
    };
    let json = serde_json::to_string(&body).expect("RegisterBody serialise");

    let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
    if let Some(tok) = &opts.bearer_token {
        headers.push(("Authorization".to_string(), format!("Bearer {tok}")));
    }
    Ok(RegisterRequest {
        url: opts.endpoint.clone().unwrap_or_else(|| DEFAULT_ENDPOINT.to_string()),
        method: "POST",
        headers,
        body: json,
    })
}

/// Convenience: emit a single `curl(1)` command line that performs the
/// registration. Pipe the output to `bash` to actually run it. The token
/// is read from the `GFTD_TOKEN` env var when not provided in opts.
pub fn curl_command(
    asm: &VehicleAssembly,
    opts: &RegisterOptions,
) -> Result<String, AssemblyError> {
    let req = register_request(asm, opts)?;
    let mut s = String::from("curl -fsSL");
    s.push_str(" -X ");
    s.push_str(req.method);
    s.push_str(" \\\n  ");
    // shell-escape the URL
    s.push_str(&shell_escape(&req.url));
    for (k, v) in &req.headers {
        s.push_str(" \\\n  -H ");
        s.push_str(&shell_escape(&format!("{k}: {v}")));
    }
    if opts.bearer_token.is_none() {
        s.push_str(" \\\n  -H \"Authorization: Bearer ${GFTD_TOKEN:?GFTD_TOKEN must be set, run: gftd agent-token --lxm ai.gftd.sbom.registerArtifact}\"");
    }
    s.push_str(" \\\n  --data ");
    s.push_str(&shell_escape(&req.body));
    Ok(s)
}

fn shell_escape(s: &str) -> String {
    // single-quote with embedded ' → '\'' substitution.
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::part::{Material, PartKind, ProvenanceSource, Supplier, VehiclePart};

    fn provenance() -> ProvenanceSource {
        ProvenanceSource {
            uri: "scad://t".into(),
            sha256: "ab".repeat(32),
            license: "MIT".into(),
        }
    }
    fn part(id: &str, kind: PartKind, mat: Material) -> VehiclePart {
        VehiclePart {
            id: id.into(),
            display_name: id.into(),
            kind,
            material: mat,
            aabb_min: [0.0, 0.0, 0.0],
            aabb_max: [1.0, 0.5, 0.3],
            mass_kg: None,
            parent: None,
            break_group: None,
            source: provenance(),
            supplier: Supplier::default(),
            revision: "1.0.0".into(),
        }
    }

    #[test]
    fn request_envelope_targets_atproto_xrpc() {
        let mut a = VehicleAssembly::new("v1", provenance());
        a.add_part(part("rail", PartKind::Chassis, Material::SteelHss));
        let req = register_request(&a, &RegisterOptions::default()).unwrap();
        assert_eq!(req.method, "POST");
        assert!(req.url.contains("/xrpc/ai.gftd.sbom.registerArtifact"));
        assert!(req.url.starts_with("https://atproto.gftd.ai"));
    }

    #[test]
    fn body_carries_vehicle_metadata() {
        let mut a = VehicleAssembly::new("v1", provenance());
        a.add_part(part("rail", PartKind::Chassis, Material::SteelHss));
        a.add_part(part("door", PartKind::Body, Material::SteelMild));
        let req = register_request(&a, &RegisterOptions::default()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&req.body).unwrap();
        assert_eq!(v["format"], "CycloneDX");
        assert_eq!(v["specVersion"], "1.5");
        assert_eq!(v["vehicleId"], "v1");
        assert_eq!(v["partCount"], 2);
        assert!(v["totalMassKg"].as_f64().unwrap() > 0.0);
        // cdxJson is a string containing a full CDX document
        let cdx = v["cdxJson"].as_str().unwrap();
        let inner: serde_json::Value = serde_json::from_str(cdx).unwrap();
        assert_eq!(inner["bomFormat"], "CycloneDX");
        assert_eq!(inner["specVersion"], "1.5");
    }

    #[test]
    fn auth_header_added_when_token_supplied() {
        let mut a = VehicleAssembly::new("v1", provenance());
        a.add_part(part("rail", PartKind::Chassis, Material::SteelHss));
        let opts = RegisterOptions {
            bearer_token: Some("eyJ.testtoken".into()),
            ..Default::default()
        };
        let req = register_request(&a, &opts).unwrap();
        let auth = req.headers.iter().find(|(k, _)| k == "Authorization").unwrap();
        assert_eq!(auth.1, "Bearer eyJ.testtoken");
    }

    #[test]
    fn curl_command_is_runnable() {
        let mut a = VehicleAssembly::new("v1", provenance());
        a.add_part(part("rail", PartKind::Chassis, Material::SteelHss));
        let cmd = curl_command(&a, &RegisterOptions::default()).unwrap();
        assert!(cmd.starts_with("curl -fsSL -X POST"));
        assert!(cmd.contains("/xrpc/ai.gftd.sbom.registerArtifact"));
        assert!(cmd.contains("Content-Type: application/json"));
        // GFTD_TOKEN env-var indirection in the printed command.
        assert!(cmd.contains("GFTD_TOKEN"));
    }

    #[test]
    fn endpoint_override_respected() {
        let mut a = VehicleAssembly::new("v1", provenance());
        a.add_part(part("rail", PartKind::Chassis, Material::SteelHss));
        let opts = RegisterOptions {
            endpoint: Some("https://staging.atproto.gftd.ai/xrpc/ai.gftd.sbom.registerArtifact".into()),
            ..Default::default()
        };
        let req = register_request(&a, &opts).unwrap();
        assert!(req.url.starts_with("https://staging."));
    }
}
