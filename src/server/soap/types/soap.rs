use crate::server::soap::error::{SoapError, SoapProtocolError};
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SoapEnvelope {
    #[serde(rename = "@xmlns:s")]
    _xmlns: Option<String>,

    #[serde(rename = "@s:encodingStyle")]
    _encoding_style: Option<String>,

    #[serde(rename = "Body")]
    pub body: SoapBody,
}

#[derive(Debug, Deserialize)]
pub struct SoapBody {
    #[serde(flatten)]
    pub content: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct NamespacedAction<T> {
    #[serde(rename = "@xmlns:u")]
    _xmlns: Option<String>,

    #[serde(flatten)]
    pub inner: T,
}

pub struct ParsedSoapAction<T> {
    pub action: T,
}

pub fn parse_soap_action(headers: &HeaderMap) -> Result<String, SoapError> {
    // Extract and validate the `SOAPAction` header
    let soap_action = headers
        .get("SOAPAction")
        .or_else(|| headers.get("SOAPACTION"))
        .and_then(|v| v.to_str().ok())
        .ok_or({ SoapError::Soap(SoapProtocolError::MissingField("SOAPAction header missing")) })?;

    // Clean up the SOAPAction header value and extract the action name
    soap_action
        .trim_matches('"')
        .split('#')
        .last()
        .ok_or_else(|| {
            SoapError::Soap(SoapProtocolError::InvalidAction(
                "Invalid SOAPAction format".into(),
            ))
        })
        .map(|s| s.to_string())
}

#[async_trait]
impl<T, S> FromRequestParts<S> for ParsedSoapAction<T>
where
    T: TryFrom<String, Error = SoapError> + Send + Sync,
    S: Send + Sync,
{
    type Rejection = SoapError;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let action = parse_soap_action(&parts.headers)?;
        T::try_from(action).map(|action| ParsedSoapAction { action })
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    
    #[derive(Debug, Deserialize)]
    struct SetAVTransportURIRequest {
        #[serde(rename = "InstanceID")]
        instance_id: String,
        
        #[serde(rename = "CurrentURI")]
        current_uri: String,
        
        #[serde(rename = "CurrentURIMetaData")]
        current_uri_metadata: String,
    }
    
    #[derive(Debug, Deserialize)]
    struct AVTransportAction {
        #[serde(rename = "u:SetAVTransportURI")]
        set_av_transport_uri: SetAVTransportURIRequest,
    }
    
    #[test]
    fn test_parse_soap_envelope() {
        // Sample SOAP envelope
        let soap_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:SetAVTransportURI xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <InstanceID>0</InstanceID>
      <CurrentURI>http://example.com/your-media-file</CurrentURI>
      <CurrentURIMetaData></CurrentURIMetaData>
    </u:SetAVTransportURI>
  </s:Body>
</s:Envelope>"#;

        // Parse the envelope
        let result = quick_xml::de::from_str::<SoapEnvelope>(soap_xml);
        
        // Assert the result
        assert!(result.is_ok(), "Failed to parse SOAP envelope: {:?}", result.err());
        
        // Verify parsed values
        if let Ok(envelope) = result {
            let action = &envelope.body.action.set_av_transport_uri;
            assert_eq!(action.instance_id, "0");
            assert_eq!(action.current_uri, "http://example.com/your-media-file");
            assert_eq!(action.current_uri_metadata, "");
        }
    }
    
    #[test]
    fn test_parse_soap_action_header() {
        // Sample SOAP action header
        let mut headers = HeaderMap::new();
        headers.insert(
            "SOAPAction", 
            "\"urn:schemas-upnp-org:service:AVTransport:1#SetAVTransportURI\"".parse().unwrap()
        );
        
        // Parse the action
        let result = parse_soap_action(&headers);
        
        // Assert the result
        assert!(result.is_ok(), "Failed to parse SOAP action: {:?}", result.err());
        assert_eq!(result.unwrap(), "SetAVTransportURI");
    }
}