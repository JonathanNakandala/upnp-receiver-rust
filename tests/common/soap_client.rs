use reqwest::Client;
use std::net::SocketAddr;
use tracing::info;

pub struct SoapClient {
    client: Client,
    base_url: String,
}

impl SoapClient {
    pub fn new(address: SocketAddr) -> Self {
        Self {
            client: Client::new(),
            base_url: format!("http://{}", address),
        }
    }

    pub async fn set_av_transport_uri(
        &self,
        uri: &str,
        metadata: Option<&str>,
    ) -> reqwest::Response {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
        <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
            <s:Body>
                <u:SetAVTransportURI xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                    <InstanceID>0</InstanceID>
                    <CurrentURI>{}</CurrentURI>
                    <CurrentURIMetaData>{}</CurrentURIMetaData>
                </u:SetAVTransportURI>
            </s:Body>
        </s:Envelope>"#,
            uri,
            metadata.unwrap_or("")
        );

        info!(body = body.as_str(), "Made Set AV Transport URI Body");

        self.client
            .post(format!("{}/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#SetAVTransportURI\"",
            )
            .body(body)
            .send()
            .await.unwrap()
    }

    pub async fn play(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:Play xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                        <InstanceID>0</InstanceID>
                        <Speed>1</Speed>
                    </u:Play>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!("{}/services/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header("SOAPAction", "Play")
            .body(body)
            .send()
            .await
            .unwrap()
    }
    pub async fn get_transport_info(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetTransportInfo xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                        <InstanceID>0</InstanceID>
                    </u:GetTransportInfo>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!("{}/services/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header("SOAPAction", "GetTransportInfo")
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn stop(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:Stop xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                        <InstanceID>0</InstanceID>
                    </u:Stop>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!("{}/services/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header("SOAPAction", "Stop")
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn pause(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:Pause xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                        <InstanceID>0</InstanceID>
                    </u:Pause>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!("{}/services/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header("SOAPAction", "Pause")
            .body(body)
            .send()
            .await
            .unwrap()
    }
}
