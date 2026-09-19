//! Fetches and parses a discovered device's description XML and each of
//! its services' SCPD documents, to report what the device actually
//! claims to implement: service types (each carrying its own version in
//! the URN, e.g. `...:AVTransport:1`), the actions each one advertises,
//! the UPnP Device Architecture `specVersion` each SCPD declares, and each
//! state variable's `allowedValueList`/`allowedValueRange` — the enum
//! values or numeric range a control point is allowed to send it.

use crate::discovery::error::DiscoveryServiceError;
use reqwest::{Client, Url};
use serde::Deserialize;
use tracing::warn;

#[derive(Debug, Deserialize)]
struct DeviceDescriptionDoc {
    device: DeviceDecl,
}

#[derive(Debug, Deserialize)]
struct DeviceDecl {
    #[serde(rename = "deviceType")]
    device_type: String,
    #[serde(rename = "friendlyName", default)]
    friendly_name: String,
    #[serde(rename = "manufacturer", default)]
    manufacturer: String,
    #[serde(rename = "modelName", default)]
    model_name: String,
    #[serde(rename = "UDN", default)]
    udn: String,
    #[serde(rename = "serviceList", default)]
    service_list: ServiceList,
}

#[derive(Debug, Deserialize, Default)]
struct ServiceList {
    #[serde(rename = "service", default)]
    service: Vec<ServiceDecl>,
}

#[derive(Debug, Deserialize, Clone)]
struct ServiceDecl {
    #[serde(rename = "serviceType")]
    service_type: String,
    #[serde(rename = "serviceId", default)]
    service_id: String,
    #[serde(rename = "SCPDURL")]
    scpd_url: String,
}

#[derive(Debug, Deserialize)]
struct ScpdDoc {
    #[serde(rename = "specVersion", default)]
    spec_version: SpecVersion,
    #[serde(rename = "actionList", default)]
    action_list: ActionList,
    #[serde(rename = "serviceStateTable", default)]
    service_state_table: ServiceStateTable,
}

#[derive(Debug, Deserialize, Default)]
struct SpecVersion {
    #[serde(default)]
    major: u32,
    #[serde(default)]
    minor: u32,
}

#[derive(Debug, Deserialize, Default)]
struct ActionList {
    #[serde(rename = "action", default)]
    action: Vec<ActionDecl>,
}

#[derive(Debug, Deserialize)]
struct ActionDecl {
    name: String,
}

#[derive(Debug, Deserialize, Default)]
struct ServiceStateTable {
    #[serde(rename = "stateVariable", default)]
    state_variable: Vec<StateVariableDecl>,
}

#[derive(Debug, Deserialize)]
struct StateVariableDecl {
    name: String,
    #[serde(rename = "dataType", default)]
    data_type: String,
    #[serde(rename = "allowedValueList", default)]
    allowed_value_list: Option<AllowedValueList>,
    #[serde(rename = "allowedValueRange", default)]
    allowed_value_range: Option<AllowedValueRange>,
}

#[derive(Debug, Deserialize, Default)]
struct AllowedValueList {
    #[serde(rename = "allowedValue", default)]
    allowed_value: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AllowedValueRange {
    minimum: String,
    maximum: String,
    #[serde(default)]
    step: Option<String>,
}

/// A discovered device's advertised identity plus everything gathered
/// about each service it declares.
#[derive(Debug)]
pub struct DeviceReport {
    pub location: String,
    pub device_type: String,
    pub friendly_name: String,
    pub manufacturer: String,
    pub model_name: String,
    pub udn: String,
    pub services: Vec<ServiceReport>,
}

/// What a single service's SCPD says it implements.
#[derive(Debug)]
pub struct ServiceReport {
    pub service_type: String,
    pub service_id: String,
    /// UPnP Device Architecture spec version this SCPD document was
    /// written against (major, minor) — distinct from the service's own
    /// version number, which is the trailing segment of `service_type`.
    pub spec_version: (u32, u32),
    pub actions: Vec<String>,
    pub state_variables: Vec<StateVariableReport>,
}

/// One state variable's declared type and — where the SCPD constrains it —
/// the enum values or numeric range a control point is allowed to send.
/// Most state variables declare neither (plain `string`/`ui4`/etc. with no
/// `allowedValueList`/`allowedValueRange`), hence `constraint` being
/// optional rather than every variant always carrying one.
#[derive(Debug)]
pub struct StateVariableReport {
    pub name: String,
    pub data_type: String,
    pub constraint: Option<VariableConstraint>,
}

#[derive(Debug)]
pub enum VariableConstraint {
    Enum(Vec<String>),
    Range {
        minimum: String,
        maximum: String,
        step: Option<String>,
    },
}

/// Fetches `location` (a device's `LOCATION` header from an SSDP search
/// response) and every service SCPD it declares, building a full report.
/// A service whose SCPD can't be fetched/parsed is logged and skipped
/// rather than failing the whole device — one broken service shouldn't
/// hide what the rest of the device advertises.
pub async fn fetch_device_report(
    http: &Client,
    location: &str,
) -> Result<DeviceReport, DiscoveryServiceError> {
    let base = Url::parse(location).map_err(|e| {
        DiscoveryServiceError::ProtocolError(format!("invalid LOCATION URL {}: {}", location, e))
    })?;

    let body = http.get(base.clone()).send().await?.text().await?;
    let doc: DeviceDescriptionDoc = quick_xml::de::from_str(&body)?;

    let mut services = Vec::with_capacity(doc.device.service_list.service.len());
    for service in &doc.device.service_list.service {
        match fetch_service_report(http, &base, service).await {
            Ok(report) => services.push(report),
            Err(err) => warn!(
                service_type = %service.service_type,
                error = %err,
                "Failed to fetch/parse SCPD for service"
            ),
        }
    }

    Ok(DeviceReport {
        location: location.to_string(),
        device_type: doc.device.device_type,
        friendly_name: doc.device.friendly_name,
        manufacturer: doc.device.manufacturer,
        model_name: doc.device.model_name,
        udn: doc.device.udn,
        services,
    })
}

async fn fetch_service_report(
    http: &Client,
    base: &Url,
    service: &ServiceDecl,
) -> Result<ServiceReport, DiscoveryServiceError> {
    let scpd_url = base.join(&service.scpd_url).map_err(|e| {
        DiscoveryServiceError::ProtocolError(format!(
            "invalid SCPDURL {}: {}",
            service.scpd_url, e
        ))
    })?;

    let body = http.get(scpd_url).send().await?.text().await?;
    let doc: ScpdDoc = quick_xml::de::from_str(&body)?;

    let state_variables = doc
        .service_state_table
        .state_variable
        .into_iter()
        .map(|v| StateVariableReport {
            constraint: variable_constraint(&v),
            name: v.name,
            data_type: v.data_type,
        })
        .collect();

    Ok(ServiceReport {
        service_type: service.service_type.clone(),
        service_id: service.service_id.clone(),
        spec_version: (doc.spec_version.major, doc.spec_version.minor),
        actions: doc.action_list.action.into_iter().map(|a| a.name).collect(),
        state_variables,
    })
}

/// A variable can declare an `allowedValueList` (enum) or an
/// `allowedValueRange` (numeric bounds), never both per the SCPD schema —
/// `allowedValueList` is checked first since it's the more specific/common
/// constraint if a malformed document somehow declared both.
fn variable_constraint(variable: &StateVariableDecl) -> Option<VariableConstraint> {
    if let Some(list) = &variable.allowed_value_list {
        if !list.allowed_value.is_empty() {
            return Some(VariableConstraint::Enum(list.allowed_value.clone()));
        }
    }

    variable
        .allowed_value_range
        .as_ref()
        .map(|range| VariableConstraint::Range {
            minimum: range.minimum.clone(),
            maximum: range.maximum.clone(),
            step: range.step.clone(),
        })
}
