//! VehicleAssembly → CycloneDX 1.5 SBOM emitter.
//!
//! Output is consumed by `sbom.etzhayyim.com` (CDX 1.5 default per
//! `60-apps/ai-gftd-project-sbom/CLAUDE.md`). Each `VehiclePart`
//! becomes a CycloneDX component with `type: "device"` — the closest
//! first-class fit for a physical car part in CDX 1.5. The full spec is
//! at <https://cyclonedx.org/docs/1.5/json/>.
//!
//! Schema-stable fields used:
//!
//! | CDX field | Source |
//! |---|---|
//! | `bom-ref` | `VehiclePart::id` |
//! | `type` | always `"device"` |
//! | `name` | `display_name` |
//! | `version` | `revision` |
//! | `purl` | synthesized: `pkg:gftd-vehicle/{vehicleId}/part/{partId}@{rev}?supplier=...&material=...&kind=...` |
//! | `cpe` | `Supplier::cpe` if non-empty |
//! | `manufacturer` | `Supplier::name` |
//! | `licenses` | `[{ "expression": Source.license }]` |
//! | `evidence.identity` | `{ "field": "hash", "concludedValue": Source.sha256 }` |
//! | `properties[]` | `cdx:gftd:vehicle:{break_group,mass_kg,material,kind,parent,supplier_mpn}` |
//!
//! Top-level metadata declares the vehicle itself as a `device` component
//! and lists every part as a sub-component, plus a `dependencies` graph
//! that mirrors the parent + hardpoint relationships.

use serde::Serialize;

use crate::part::{
    AssemblyError, Hardpoint, HardpointKind, Material, PartKind, VehicleAssembly, VehiclePart,
};

const CDX_SPEC_VERSION: &str = "1.5";
const PROP_NS: &str = "cdx:gftd:vehicle";

#[derive(Debug, Serialize)]
struct CdxFile {
    #[serde(rename = "bomFormat")]
    bom_format: &'static str,
    #[serde(rename = "specVersion")]
    spec_version: &'static str,
    version: u32,
    #[serde(rename = "serialNumber")]
    serial_number: String,
    metadata: CdxMetadata,
    components: Vec<CdxComponent>,
    dependencies: Vec<CdxDependency>,
}

#[derive(Debug, Serialize)]
struct CdxMetadata {
    timestamp: String,
    tools: Vec<CdxTool>,
    component: CdxComponent,
}

#[derive(Debug, Serialize)]
struct CdxTool {
    vendor: &'static str,
    name: &'static str,
    version: &'static str,
}

#[derive(Debug, Serialize)]
struct CdxComponent {
    #[serde(rename = "bom-ref")]
    bom_ref: String,
    #[serde(rename = "type")]
    component_type: &'static str,
    name: String,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manufacturer: Option<CdxOrganization>,
    #[serde(skip_serializing_if = "String::is_empty")]
    purl: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    cpe: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    licenses: Vec<CdxLicense>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence: Option<CdxEvidence>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    properties: Vec<CdxProperty>,
}

#[derive(Debug, Serialize)]
struct CdxOrganization {
    name: String,
}

#[derive(Debug, Serialize)]
struct CdxLicense {
    expression: String,
}

#[derive(Debug, Serialize)]
struct CdxEvidence {
    identity: CdxEvidenceIdentity,
}

#[derive(Debug, Serialize)]
struct CdxEvidenceIdentity {
    field: &'static str,
    #[serde(rename = "concludedValue")]
    concluded_value: String,
    methods: Vec<CdxEvidenceMethod>,
}

#[derive(Debug, Serialize)]
struct CdxEvidenceMethod {
    technique: &'static str,
    confidence: f32,
    value: String,
}

#[derive(Debug, Serialize)]
struct CdxProperty {
    name: String,
    value: String,
}

#[derive(Debug, Serialize)]
struct CdxDependency {
    #[serde(rename = "ref")]
    bom_ref: String,
    #[serde(rename = "dependsOn", skip_serializing_if = "Vec::is_empty")]
    depends_on: Vec<String>,
}

fn material_label(m: Material) -> &'static str {
    match m {
        Material::SteelHss => "steel-hss",
        Material::SteelMild => "steel-mild",
        Material::AluminiumCast => "aluminium-cast",
        Material::AluminiumSheet => "aluminium-sheet",
        Material::Glass => "glass",
        Material::Rubber => "rubber",
        Material::Plastic => "plastic",
        Material::LiIon => "lithium-ion",
        Material::Composite => "composite",
        Material::Other => "other",
    }
}

fn kind_label(k: PartKind) -> &'static str {
    match k {
        PartKind::Chassis => "chassis",
        PartKind::Body => "body",
        PartKind::Window => "window",
        PartKind::Powertrain => "powertrain",
        PartKind::Suspension => "suspension",
        PartKind::Wheel => "wheel",
        PartKind::Brake => "brake",
        PartKind::Interior => "interior",
        PartKind::Electrical => "electrical",
        PartKind::Fluid => "fluid",
        PartKind::Trim => "trim",
    }
}

fn hardpoint_label(k: HardpointKind) -> &'static str {
    match k {
        HardpointKind::Bolt => "bolt",
        HardpointKind::Weld => "weld",
        HardpointKind::Hinge => "hinge",
        HardpointKind::Latch => "latch",
        HardpointKind::Press => "press",
        HardpointKind::Adhesive => "adhesive",
    }
}

/// Synthesize a `purl` for a part. We use the `gftd-vehicle` namespace
/// (vendor-specific, allowed under <https://github.com/package-url/purl-spec>
/// as a custom type) so `sbom.etzhayyim.com` queries can filter on it cleanly.
fn synth_purl(asm: &VehicleAssembly, part: &VehiclePart) -> String {
    let mut q = Vec::new();
    if !part.supplier.name.is_empty() {
        q.push(format!("supplier={}", urlencode(&part.supplier.name)));
    }
    if !part.supplier.mpn.is_empty() {
        q.push(format!("mpn={}", urlencode(&part.supplier.mpn)));
    }
    q.push(format!("material={}", material_label(part.material)));
    q.push(format!("kind={}", kind_label(part.kind)));
    q.push(format!("license={}", urlencode(&part.source.license)));
    format!(
        "pkg:gftd-vehicle/{}/part/{}@{}?{}",
        urlencode(&asm.vehicle_id),
        urlencode(&part.id),
        urlencode(&part.revision),
        q.join("&"),
    )
}

fn vehicle_purl(asm: &VehicleAssembly) -> String {
    format!(
        "pkg:gftd-vehicle/{}@{}",
        urlencode(&asm.vehicle_id),
        urlencode(&asm.revision)
    )
}

/// Minimal `application/x-www-form-urlencoded`-compatible escaper. We
/// keep this in-crate to avoid pulling `urlencoding` for one helper.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '.' | '_' | '~' => out.push(c),
            _ => {
                let mut buf = [0u8; 4];
                for byte in c.encode_utf8(&mut buf).as_bytes() {
                    out.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    out
}

fn build_part_component(asm: &VehicleAssembly, part: &VehiclePart) -> CdxComponent {
    let mut props = vec![
        CdxProperty {
            name: format!("{PROP_NS}:break_group"),
            value: part.effective_break_group().to_string(),
        },
        CdxProperty {
            name: format!("{PROP_NS}:mass_kg"),
            value: format!("{:.4}", part.effective_mass_kg()),
        },
        CdxProperty {
            name: format!("{PROP_NS}:material"),
            value: material_label(part.material).to_string(),
        },
        CdxProperty {
            name: format!("{PROP_NS}:kind"),
            value: kind_label(part.kind).to_string(),
        },
    ];
    if let Some(parent) = &part.parent {
        props.push(CdxProperty {
            name: format!("{PROP_NS}:parent"),
            value: parent.clone(),
        });
    }
    if !part.supplier.mpn.is_empty() {
        props.push(CdxProperty {
            name: format!("{PROP_NS}:supplier_mpn"),
            value: part.supplier.mpn.clone(),
        });
    }
    props.push(CdxProperty {
        name: format!("{PROP_NS}:source_uri"),
        value: part.source.uri.clone(),
    });

    CdxComponent {
        bom_ref: part.id.clone(),
        component_type: "device",
        name: part.display_name.clone(),
        version: part.revision.clone(),
        description: None,
        manufacturer: if part.supplier.name.is_empty() {
            None
        } else {
            Some(CdxOrganization {
                name: part.supplier.name.clone(),
            })
        },
        purl: synth_purl(asm, part),
        cpe: part.supplier.cpe.clone(),
        licenses: vec![CdxLicense {
            expression: part.source.license.clone(),
        }],
        evidence: Some(CdxEvidence {
            identity: CdxEvidenceIdentity {
                field: "hash",
                concluded_value: part.source.sha256.clone(),
                methods: vec![CdxEvidenceMethod {
                    technique: "filename",
                    confidence: 1.0,
                    value: part.source.uri.clone(),
                }],
            },
        }),
        properties: props,
    }
}

fn build_root_component(asm: &VehicleAssembly) -> CdxComponent {
    CdxComponent {
        bom_ref: asm.vehicle_id.clone(),
        component_type: "device",
        name: asm.display_name.clone(),
        version: asm.revision.clone(),
        description: Some(format!("driver.etzhayyim.com vehicle {}", asm.vehicle_id)),
        manufacturer: Some(CdxOrganization {
            name: "gftd".into(),
        }),
        purl: vehicle_purl(asm),
        cpe: String::new(),
        licenses: vec![CdxLicense {
            expression: asm.source.license.clone(),
        }],
        evidence: Some(CdxEvidence {
            identity: CdxEvidenceIdentity {
                field: "hash",
                concluded_value: asm.source.sha256.clone(),
                methods: vec![CdxEvidenceMethod {
                    technique: "filename",
                    confidence: 1.0,
                    value: asm.source.uri.clone(),
                }],
            },
        }),
        properties: vec![
            CdxProperty {
                name: format!("{PROP_NS}:total_mass_kg"),
                value: format!("{:.4}", asm.total_mass_kg()),
            },
            CdxProperty {
                name: format!("{PROP_NS}:part_count"),
                value: asm.parts.len().to_string(),
            },
            CdxProperty {
                name: format!("{PROP_NS}:hardpoint_count"),
                value: asm.hardpoints.len().to_string(),
            },
        ],
    }
}

fn build_dependencies(asm: &VehicleAssembly) -> Vec<CdxDependency> {
    let mut deps: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();

    deps.insert(
        asm.vehicle_id.clone(),
        asm.parts.iter().map(|p| p.id.clone()).collect(),
    );

    // parent → child
    for p in &asm.parts {
        if let Some(parent) = &p.parent {
            deps.entry(parent.clone()).or_default().push(p.id.clone());
        }
    }

    // hardpoints — surface as bidirectional edges (target depends on source).
    // We record both directions so a CVE on either side flags the other.
    for hp in &asm.hardpoints {
        deps.entry(hp.from_part.clone())
            .or_default()
            .push(hp.to_part.clone());
        deps.entry(hp.to_part.clone())
            .or_default()
            .push(hp.from_part.clone());
    }

    deps.into_iter()
        .map(|(bom_ref, mut depends_on)| {
            depends_on.sort();
            depends_on.dedup();
            CdxDependency {
                bom_ref,
                depends_on,
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct CycloneDxOptions {
    /// Stable serial number; if `None` we synthesize from
    /// `urn:uuid:` + sha256 of the vehicle source. Using a deterministic
    /// id makes byte-for-byte SBOM diffs across builds tractable.
    pub serial_number: Option<String>,
    pub timestamp: Option<String>,
}

impl Default for CycloneDxOptions {
    fn default() -> Self {
        Self {
            serial_number: None,
            timestamp: Some("2026-05-05T00:00:00Z".into()),
        }
    }
}

/// Emit a CycloneDX 1.5 JSON SBOM for the vehicle assembly.
pub fn emit(asm: &VehicleAssembly, opts: &CycloneDxOptions) -> Result<String, AssemblyError> {
    asm.validate()?;
    // Hardpoint kinds aren't placed in the SBOM body, but we surface them
    // as `cdx:gftd:vehicle:hardpoints` aggregate counts on the root for
    // quick recall queries (e.g. "all vehicles with > 0 adhesive joints").
    let hp_summary = hardpoint_summary(&asm.hardpoints);

    let mut root = build_root_component(asm);
    for (kind, count) in hp_summary {
        root.properties.push(CdxProperty {
            name: format!("{PROP_NS}:hardpoints:{}", hardpoint_label(kind)),
            value: count.to_string(),
        });
    }

    let serial = opts
        .serial_number
        .clone()
        .unwrap_or_else(|| format!("urn:uuid:{}", deterministic_uuid_v5(&asm.source.sha256)));

    let timestamp = opts
        .timestamp
        .clone()
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".into());

    let file = CdxFile {
        bom_format: "CycloneDX",
        spec_version: CDX_SPEC_VERSION,
        version: 1,
        serial_number: serial,
        metadata: CdxMetadata {
            timestamp,
            tools: vec![CdxTool {
                vendor: "gftd",
                name: "kami-cad-import",
                version: env!("CARGO_PKG_VERSION"),
            }],
            component: root,
        },
        components: asm
            .parts
            .iter()
            .map(|p| build_part_component(asm, p))
            .collect(),
        dependencies: build_dependencies(asm),
    };

    Ok(serde_json::to_string_pretty(&file).expect("CDX structs are infallible to serialise"))
}

fn hardpoint_summary(hps: &[Hardpoint]) -> Vec<(HardpointKind, usize)> {
    let mut counts: std::collections::BTreeMap<u8, (HardpointKind, usize)> =
        std::collections::BTreeMap::new();
    for hp in hps {
        let key = hp.kind as u8;
        let entry = counts.entry(key).or_insert((hp.kind, 0));
        entry.1 += 1;
    }
    counts.into_values().collect()
}

/// Stable URN-friendly id derived from a hex string. We don't depend on
/// `uuid` to keep the dep tree tiny — the format below is a valid v5 layout
/// (RFC 4122 §4.3 stub) for the sole purpose of producing a stable URN per
/// vehicle source hash.
fn deterministic_uuid_v5(seed_hex: &str) -> String {
    // Take 32 hex chars, force version=5, variant=10xx.
    let mut chars: Vec<char> = seed_hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    while chars.len() < 32 {
        chars.push('0');
    }
    chars.truncate(32);
    chars[12] = '5'; // version 5
    chars[16] = match chars[16] {
        '0'..='3' => '8',
        '4'..='7' => '9',
        '8'..='b' => 'a',
        _ => 'b',
    };
    let s: String = chars.into_iter().collect();
    format!(
        "{}-{}-{}-{}-{}",
        &s[0..8],
        &s[8..12],
        &s[12..16],
        &s[16..20],
        &s[20..32],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::part::{
        Hardpoint, HardpointKind, Material, PartKind, ProvenanceSource, Supplier, VehiclePart,
    };

    fn provenance() -> ProvenanceSource {
        ProvenanceSource {
            uri: "scad://test".into(),
            sha256: "deadbeef".repeat(8),
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
    fn emits_cdx_15_top_level() {
        let mut a = VehicleAssembly::new("v1", provenance());
        a.add_part(part("rail", PartKind::Chassis, Material::SteelHss));
        let json = emit(&a, &CycloneDxOptions::default()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["bomFormat"], "CycloneDX");
        assert_eq!(v["specVersion"], "1.5");
        assert!(v["serialNumber"].as_str().unwrap().starts_with("urn:uuid:"));
        assert_eq!(v["metadata"]["component"]["type"], "device");
        assert_eq!(v["metadata"]["component"]["bom-ref"], "v1");
        assert_eq!(v["components"][0]["type"], "device");
        assert_eq!(v["components"][0]["bom-ref"], "rail");
    }

    #[test]
    fn purl_carries_supplier_and_material() {
        let mut a = VehicleAssembly::new("v1", provenance());
        let mut p = part("rail", PartKind::Chassis, Material::SteelHss);
        p.supplier = Supplier {
            name: "Toray".into(),
            cpe: String::new(),
            mpn: "T700S-12K".into(),
        };
        a.add_part(p);
        let json = emit(&a, &CycloneDxOptions::default()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let purl = v["components"][0]["purl"].as_str().unwrap();
        assert!(purl.starts_with("pkg:gftd-vehicle/v1/part/rail@1.0.0?"));
        assert!(purl.contains("supplier=Toray"));
        assert!(purl.contains("mpn=T700S-12K"));
        assert!(purl.contains("material=steel-hss"));
        assert!(purl.contains("kind=chassis"));
        assert!(purl.contains("license=MIT"));
    }

    #[test]
    fn evidence_carries_sha256() {
        let mut a = VehicleAssembly::new("v1", provenance());
        a.add_part(part("rail", PartKind::Chassis, Material::SteelHss));
        let json = emit(&a, &CycloneDxOptions::default()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let ev = &v["components"][0]["evidence"]["identity"];
        assert_eq!(ev["field"], "hash");
        let sha = ev["concludedValue"].as_str().unwrap();
        assert_eq!(sha.len(), 64);
    }

    #[test]
    fn dependencies_mirror_parent_and_hardpoints() {
        let mut a = VehicleAssembly::new("v1", provenance());
        a.add_part(part("rail", PartKind::Chassis, Material::SteelHss));
        let mut hood = part("hood", PartKind::Body, Material::AluminiumSheet);
        hood.parent = Some("rail".into());
        a.add_part(hood);
        a.add_hardpoint(Hardpoint {
            id: "hp1".into(),
            from_part: "rail".into(),
            to_part: "hood".into(),
            position: [0.5, 0.5, 0.15],
            kind: HardpointKind::Bolt,
        });
        let json = emit(&a, &CycloneDxOptions::default()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let deps = v["dependencies"].as_array().unwrap();
        // root + rail + hood
        let by_ref: std::collections::HashMap<String, Vec<String>> = deps
            .iter()
            .map(|d| {
                let r = d["ref"].as_str().unwrap().to_string();
                let on: Vec<String> = d["dependsOn"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap().to_string())
                    .collect();
                (r, on)
            })
            .collect();
        assert!(by_ref["v1"].contains(&"rail".to_string()));
        assert!(by_ref["v1"].contains(&"hood".to_string()));
        // parent + hardpoint = 2 unique dependencies on rail
        let rail_on = &by_ref["rail"];
        assert!(rail_on.contains(&"hood".to_string()));
        let hood_on = &by_ref["hood"];
        assert!(hood_on.contains(&"rail".to_string()));
    }

    #[test]
    fn properties_carry_break_group_mass_material() {
        let mut a = VehicleAssembly::new("v1", provenance());
        a.add_part(part("rail", PartKind::Chassis, Material::SteelHss));
        let json = emit(&a, &CycloneDxOptions::default()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let props = v["components"][0]["properties"].as_array().unwrap();
        let names: std::collections::HashSet<&str> =
            props.iter().map(|p| p["name"].as_str().unwrap()).collect();
        assert!(names.contains("cdx:gftd:vehicle:break_group"));
        assert!(names.contains("cdx:gftd:vehicle:mass_kg"));
        assert!(names.contains("cdx:gftd:vehicle:material"));
        assert!(names.contains("cdx:gftd:vehicle:kind"));
        assert!(names.contains("cdx:gftd:vehicle:source_uri"));
    }

    #[test]
    fn refuses_when_provenance_missing() {
        let mut a = VehicleAssembly::new("v1", provenance());
        let mut p = part("rail", PartKind::Chassis, Material::SteelHss);
        p.source.sha256 = String::new();
        a.add_part(p);
        assert!(emit(&a, &CycloneDxOptions::default()).is_err());
    }

    #[test]
    fn deterministic_serial_for_same_source() {
        let mut a = VehicleAssembly::new("v1", provenance());
        a.add_part(part("rail", PartKind::Chassis, Material::SteelHss));
        let s1 = emit(&a, &CycloneDxOptions::default()).unwrap();
        let s2 = emit(&a, &CycloneDxOptions::default()).unwrap();
        let v1: serde_json::Value = serde_json::from_str(&s1).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&s2).unwrap();
        assert_eq!(v1["serialNumber"], v2["serialNumber"]);
    }

    #[test]
    fn root_carries_total_mass_and_part_count() {
        let mut a = VehicleAssembly::new("v1", provenance());
        a.add_part(part("rail", PartKind::Chassis, Material::SteelHss));
        a.add_part(part("door", PartKind::Body, Material::SteelMild));
        let json = emit(&a, &CycloneDxOptions::default()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let props = v["metadata"]["component"]["properties"].as_array().unwrap();
        let by_name: std::collections::HashMap<&str, &str> = props
            .iter()
            .map(|p| (p["name"].as_str().unwrap(), p["value"].as_str().unwrap()))
            .collect();
        assert_eq!(by_name["cdx:gftd:vehicle:part_count"], "2");
        let total: f32 = by_name["cdx:gftd:vehicle:total_mass_kg"].parse().unwrap();
        assert!(total > 0.0);
    }
}
