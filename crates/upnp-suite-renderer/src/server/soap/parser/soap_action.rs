use crate::server::soap::error::{SoapError, SoapProtocolError};
use crate::server::soap::types::soap_action::{ParsedSoapAction, UpnpService, UpnpServiceVersion};
use axum::http::HeaderMap;

pub fn parse_soap_action(headers: &HeaderMap) -> Result<ParsedSoapAction, SoapError> {
    // Extract and validate the `SOAPAction` header
    let soap_action = headers
        .get("SOAPAction")
        .or_else(|| headers.get("SOAPACTION"))
        .and_then(|v| v.to_str().ok())
        .ok_or(SoapError::Soap(SoapProtocolError::MissingField(
            "SOAPAction header missing",
        )))?;

    // Clean up the SOAPAction header value
    let soap_action = soap_action.trim_matches('"');
    let parts: Vec<&str> = soap_action.split(':').collect();

    if parts.len() < 5 {
        return Err(SoapError::Soap(SoapProtocolError::InvalidAction(
            "Invalid SOAPAction format".into(),
        )));
    }

    // Service name is at index 3 (4th part)
    let service = match parts.get(3) {
        Some(&"AVTransport") => UpnpService::AVTransport,
        Some(&"ConnectionManager") => UpnpService::ConnectionManager,
        Some(&"RenderingControl") => UpnpService::RenderingControl,
        Some(other) => {
            return Err(SoapError::Soap(SoapProtocolError::InvalidAction(
                format!("Unrecognized service: {}", other),
            )))
        }
        None => {
            return Err(SoapError::Soap(SoapProtocolError::InvalidAction(
                "Missing service name".into(),
            )))
        }
    };

    // Version and action are in the 5th part (index 4)
    let version_action = parts.get(4).ok_or_else(|| {
        SoapError::Soap(SoapProtocolError::InvalidAction(
            "Missing version and action".into(),
        ))
    })?;

    // Split version and action by '#'
    let version_action_parts: Vec<&str> = version_action.split('#').collect();
    if version_action_parts.len() != 2 {
        return Err(SoapError::Soap(SoapProtocolError::InvalidAction(
            "Invalid version and action format".into(),
        )));
    }

    let version = match version_action_parts[0] {
        "1" => UpnpServiceVersion::V1,
        // Add more versions if needed
        _ => {
            return Err(SoapError::Soap(SoapProtocolError::InvalidAction(
                "Unsupported version".into(),
            )))
        }
    };

    let action = version_action_parts[1].to_string();

    Ok(ParsedSoapAction {
        service,
        version,
        action,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    fn create_headers(soap_action: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("SOAPAction", soap_action.parse().unwrap());
        headers
    }

    #[test]
    fn test_av_transport() {
        let headers =
            create_headers("urn:schemas-upnp-org:service:AVTransport:1#SetAVTransportURI");
        let result = parse_soap_action(&headers).unwrap();
        assert_eq!(result.service, UpnpService::AVTransport);
        assert_eq!(result.version, UpnpServiceVersion::V1);
        assert_eq!(result.action, "SetAVTransportURI");
    }

    #[test]
    fn test_connection_manager() {
        let headers =
            create_headers("urn:schemas-upnp-org:service:ConnectionManager:1#GetProtocolInfo");
        let result = parse_soap_action(&headers).unwrap();
        assert_eq!(result.service, UpnpService::ConnectionManager);
        assert_eq!(result.version, UpnpServiceVersion::V1);
        assert_eq!(result.action, "GetProtocolInfo");
    }

    #[test]
    fn test_rendering_control() {
        let headers = create_headers("urn:schemas-upnp-org:service:RenderingControl:1#SetVolume");
        let result = parse_soap_action(&headers).unwrap();
        assert_eq!(result.service, UpnpService::RenderingControl);
        assert_eq!(result.version, UpnpServiceVersion::V1);
        assert_eq!(result.action, "SetVolume");
    }

    #[test]
    fn test_quoted_soap_action() {
        let headers = create_headers("\"urn:schemas-upnp-org:service:AVTransport:1#Play\"");
        let result = parse_soap_action(&headers).unwrap();
        assert_eq!(result.service, UpnpService::AVTransport);
        assert_eq!(result.version, UpnpServiceVersion::V1);
        assert_eq!(result.action, "Play");
    }

    #[test]
    fn test_invalid_format() {
        let headers = create_headers("invalid");
        let result = parse_soap_action(&headers);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_header() {
        let headers = HeaderMap::new();
        let result = parse_soap_action(&headers);
        assert!(result.is_err());
    }

    #[test]
    fn test_unrecognized_service() {
        let headers = create_headers("urn:schemas-upnp-org:service:UnknownService:1#Action");
        let result = parse_soap_action(&headers);
        assert!(result.is_err());
    }
}
