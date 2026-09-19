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
            .post(format!("{}/services/AVTransport/control", self.base_url))
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
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#Play\"",
            )
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
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#GetTransportInfo\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_position_info(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetPositionInfo xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                        <InstanceID>0</InstanceID>
                    </u:GetPositionInfo>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!("{}/services/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#GetPositionInfo\"",
            )
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
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#Stop\"",
            )
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
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#Pause\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn seek(&self, target: &str) -> reqwest::Response {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:Seek xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                        <InstanceID>0</InstanceID>
                        <Unit>REL_TIME</Unit>
                        <Target>{}</Target>
                    </u:Seek>
                </s:Body>
            </s:Envelope>"#,
            target
        );

        self.client
            .post(format!("{}/services/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#Seek\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn set_volume(&self, volume: u16) -> reqwest::Response {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:SetVolume xmlns:u="urn:schemas-upnp-org:service:RenderingControl:1">
                        <InstanceID>0</InstanceID>
                        <Channel>Master</Channel>
                        <DesiredVolume>{}</DesiredVolume>
                    </u:SetVolume>
                </s:Body>
            </s:Envelope>"#,
            volume
        );

        self.client
            .post(format!(
                "{}/services/RenderingControl/control",
                self.base_url
            ))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:RenderingControl:1#SetVolume\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_volume(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetVolume xmlns:u="urn:schemas-upnp-org:service:RenderingControl:1">
                        <InstanceID>0</InstanceID>
                        <Channel>Master</Channel>
                    </u:GetVolume>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!(
                "{}/services/RenderingControl/control",
                self.base_url
            ))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:RenderingControl:1#GetVolume\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn set_mute(&self, mute: bool) -> reqwest::Response {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:SetMute xmlns:u="urn:schemas-upnp-org:service:RenderingControl:1">
                        <InstanceID>0</InstanceID>
                        <Channel>Master</Channel>
                        <DesiredMute>{}</DesiredMute>
                    </u:SetMute>
                </s:Body>
            </s:Envelope>"#,
            mute
        );

        self.client
            .post(format!(
                "{}/services/RenderingControl/control",
                self.base_url
            ))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:RenderingControl:1#SetMute\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_mute(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetMute xmlns:u="urn:schemas-upnp-org:service:RenderingControl:1">
                        <InstanceID>0</InstanceID>
                        <Channel>Master</Channel>
                    </u:GetMute>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!(
                "{}/services/RenderingControl/control",
                self.base_url
            ))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:RenderingControl:1#GetMute\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }
}
