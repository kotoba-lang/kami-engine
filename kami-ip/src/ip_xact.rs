/// IP-XACT component catalog and XML export.
use serde::{Deserialize, Serialize};

/// Standard bus protocol types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BusType {
    Axi4,
    Axi4Lite,
    Ahb,
    Apb,
    Wishbone,
    TileLink,
    Avalon,
}

/// Bus interface mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InterfaceMode {
    Master,
    Slave,
    System,
}

/// Mapping between logical and physical port names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMap {
    /// Logical port name in the bus definition.
    pub logical: String,
    /// Physical port name in the component.
    pub physical: String,
}

/// Bus interface on a component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusInterface {
    /// Interface name.
    pub name: String,
    /// Bus protocol type.
    pub bus_type: BusType,
    /// Master, slave, or system mode.
    pub mode: InterfaceMode,
    /// Logical-to-physical port mappings.
    pub port_maps: Vec<PortMap>,
}

/// Port direction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PortDirection {
    In,
    Out,
    InOut,
}

/// A component port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpPort {
    /// Port name.
    pub name: String,
    /// Direction.
    pub direction: PortDirection,
    /// Bit width.
    pub width: u32,
}

/// Parameter resolve type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResolveType {
    Immediate,
    User,
    Generated,
}

/// A configurable parameter on a component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpParam {
    /// Parameter name.
    pub name: String,
    /// Parameter value as string.
    pub value: String,
    /// How the parameter is resolved.
    pub resolve: ResolveType,
}

/// An IP-XACT component description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpXactComponent {
    /// Vendor name (VLNV).
    pub vendor: String,
    /// Library name (VLNV).
    pub library: String,
    /// Component name (VLNV).
    pub name: String,
    /// Component version (VLNV).
    pub version: String,
    /// Bus interfaces.
    pub bus_interfaces: Vec<BusInterface>,
    /// Ports.
    pub ports: Vec<IpPort>,
    /// Parameters.
    pub parameters: Vec<IpParam>,
}

/// Catalog of IP components.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IpCatalog {
    /// All registered components.
    pub components: Vec<IpXactComponent>,
}

impl IpCatalog {
    /// Find components that expose a specific bus type.
    pub fn find_by_bus_type(&self, bus_type: &BusType) -> Vec<&IpXactComponent> {
        self.components
            .iter()
            .filter(|c| c.bus_interfaces.iter().any(|bi| bi.bus_type == *bus_type))
            .collect()
    }
}

/// Export an IP-XACT component to IEEE 1685-2014 XML format.
pub fn export_ip_xact_xml(component: &IpXactComponent) -> String {
    let mut xml = String::with_capacity(2048);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str(
        "<ipxact:component xmlns:ipxact=\"http://www.accellera.org/XMLSchema/IPXACT/1685-2014\">\n",
    );
    xml.push_str(&format!(
        "  <ipxact:vendor>{}</ipxact:vendor>\n",
        component.vendor
    ));
    xml.push_str(&format!(
        "  <ipxact:library>{}</ipxact:library>\n",
        component.library
    ));
    xml.push_str(&format!(
        "  <ipxact:name>{}</ipxact:name>\n",
        component.name
    ));
    xml.push_str(&format!(
        "  <ipxact:version>{}</ipxact:version>\n",
        component.version
    ));

    if !component.bus_interfaces.is_empty() {
        xml.push_str("  <ipxact:busInterfaces>\n");
        for bi in &component.bus_interfaces {
            xml.push_str("    <ipxact:busInterface>\n");
            xml.push_str(&format!("      <ipxact:name>{}</ipxact:name>\n", bi.name));
            let mode_str = match bi.mode {
                InterfaceMode::Master => "master",
                InterfaceMode::Slave => "slave",
                InterfaceMode::System => "system",
            };
            xml.push_str(&format!("      <ipxact:{}/>", mode_str));
            xml.push('\n');
            if !bi.port_maps.is_empty() {
                xml.push_str("      <ipxact:portMaps>\n");
                for pm in &bi.port_maps {
                    xml.push_str("        <ipxact:portMap>\n");
                    xml.push_str(&format!("          <ipxact:logicalPort><ipxact:name>{}</ipxact:name></ipxact:logicalPort>\n", pm.logical));
                    xml.push_str(&format!("          <ipxact:physicalPort><ipxact:name>{}</ipxact:name></ipxact:physicalPort>\n", pm.physical));
                    xml.push_str("        </ipxact:portMap>\n");
                }
                xml.push_str("      </ipxact:portMaps>\n");
            }
            xml.push_str("    </ipxact:busInterface>\n");
        }
        xml.push_str("  </ipxact:busInterfaces>\n");
    }

    if !component.ports.is_empty() {
        xml.push_str("  <ipxact:model>\n    <ipxact:ports>\n");
        for port in &component.ports {
            let dir = match port.direction {
                PortDirection::In => "in",
                PortDirection::Out => "out",
                PortDirection::InOut => "inout",
            };
            xml.push_str(&format!(
                "      <ipxact:port>\n        <ipxact:name>{}</ipxact:name>\n",
                port.name
            ));
            xml.push_str(&format!(
                "        <ipxact:wire><ipxact:direction>{dir}</ipxact:direction>\n"
            ));
            xml.push_str(&format!("          <ipxact:vectors><ipxact:vector><ipxact:left>{}</ipxact:left><ipxact:right>0</ipxact:right></ipxact:vector></ipxact:vectors>\n", port.width.saturating_sub(1)));
            xml.push_str("        </ipxact:wire>\n      </ipxact:port>\n");
        }
        xml.push_str("    </ipxact:ports>\n  </ipxact:model>\n");
    }

    if !component.parameters.is_empty() {
        xml.push_str("  <ipxact:parameters>\n");
        for p in &component.parameters {
            let resolve = match p.resolve {
                ResolveType::Immediate => "immediate",
                ResolveType::User => "user",
                ResolveType::Generated => "generated",
            };
            xml.push_str(&format!("    <ipxact:parameter resolve=\"{resolve}\">\n"));
            xml.push_str(&format!("      <ipxact:name>{}</ipxact:name>\n", p.name));
            xml.push_str(&format!("      <ipxact:value>{}</ipxact:value>\n", p.value));
            xml.push_str("    </ipxact:parameter>\n");
        }
        xml.push_str("  </ipxact:parameters>\n");
    }

    xml.push_str("</ipxact:component>\n");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_component() -> IpXactComponent {
        IpXactComponent {
            vendor: "gftd".to_string(),
            library: "kami".to_string(),
            name: "uart_controller".to_string(),
            version: "1.0".to_string(),
            bus_interfaces: vec![BusInterface {
                name: "s_apb".to_string(),
                bus_type: BusType::Apb,
                mode: InterfaceMode::Slave,
                port_maps: vec![
                    PortMap {
                        logical: "PADDR".to_string(),
                        physical: "apb_addr".to_string(),
                    },
                    PortMap {
                        logical: "PWDATA".to_string(),
                        physical: "apb_wdata".to_string(),
                    },
                ],
            }],
            ports: vec![
                IpPort {
                    name: "clk".to_string(),
                    direction: PortDirection::In,
                    width: 1,
                },
                IpPort {
                    name: "rst_n".to_string(),
                    direction: PortDirection::In,
                    width: 1,
                },
                IpPort {
                    name: "tx".to_string(),
                    direction: PortDirection::Out,
                    width: 1,
                },
                IpPort {
                    name: "rx".to_string(),
                    direction: PortDirection::In,
                    width: 1,
                },
            ],
            parameters: vec![IpParam {
                name: "BAUD_RATE".to_string(),
                value: "115200".to_string(),
                resolve: ResolveType::User,
            }],
        }
    }

    #[test]
    fn ip_xact_xml_contains_component() {
        let xml = export_ip_xact_xml(&sample_component());
        assert!(
            xml.contains("<ipxact:component"),
            "Should contain component element"
        );
        assert!(xml.contains("<ipxact:vendor>gftd</ipxact:vendor>"));
        assert!(xml.contains("<ipxact:name>uart_controller</ipxact:name>"));
        assert!(xml.contains("PADDR"));
    }

    #[test]
    fn catalog_find_by_bus_type() {
        let mut catalog = IpCatalog::default();
        catalog.components.push(sample_component());
        let apb_ips = catalog.find_by_bus_type(&BusType::Apb);
        assert_eq!(apb_ips.len(), 1);
        let axi_ips = catalog.find_by_bus_type(&BusType::Axi4);
        assert_eq!(axi_ips.len(), 0);
    }
}
