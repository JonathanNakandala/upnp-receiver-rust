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
            .await
            .unwrap()
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

    /// Same as [`Self::play`] but with a caller-supplied `InstanceID` —
    /// exists only to exercise the `InvalidInstanceID` (718) rejection path
    /// for a non-zero id nothing was ever created under.
    pub async fn play_with_instance_id(&self, instance_id: u32) -> reqwest::Response {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:Play xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                        <InstanceID>{}</InstanceID>
                        <Speed>1</Speed>
                    </u:Play>
                </s:Body>
            </s:Envelope>"#,
            instance_id
        );

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

    pub async fn set_next_av_transport_uri(
        &self,
        uri: &str,
        metadata: Option<&str>,
    ) -> reqwest::Response {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
        <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
            <s:Body>
                <u:SetNextAVTransportURI xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                    <InstanceID>0</InstanceID>
                    <NextURI>{}</NextURI>
                    <NextURIMetaData>{}</NextURIMetaData>
                </u:SetNextAVTransportURI>
            </s:Body>
        </s:Envelope>"#,
            uri,
            metadata.unwrap_or("")
        );

        self.client
            .post(format!("{}/services/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#SetNextAVTransportURI\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn next(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:Next xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                        <InstanceID>0</InstanceID>
                    </u:Next>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!("{}/services/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#Next\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn previous(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:Previous xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                        <InstanceID>0</InstanceID>
                    </u:Previous>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!("{}/services/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#Previous\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_media_info(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetMediaInfo xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                        <InstanceID>0</InstanceID>
                    </u:GetMediaInfo>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!("{}/services/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#GetMediaInfo\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn set_play_mode(&self, mode: &str) -> reqwest::Response {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:SetPlayMode xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                        <InstanceID>0</InstanceID>
                        <NewPlayMode>{}</NewPlayMode>
                    </u:SetPlayMode>
                </s:Body>
            </s:Envelope>"#,
            mode
        );

        self.client
            .post(format!("{}/services/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#SetPlayMode\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_transport_settings(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetTransportSettings xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                        <InstanceID>0</InstanceID>
                    </u:GetTransportSettings>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!("{}/services/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#GetTransportSettings\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_current_transport_actions(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetCurrentTransportActions xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                        <InstanceID>0</InstanceID>
                    </u:GetCurrentTransportActions>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!("{}/services/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#GetCurrentTransportActions\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_device_capabilities(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetDeviceCapabilities xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
                        <InstanceID>0</InstanceID>
                    </u:GetDeviceCapabilities>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!("{}/services/AVTransport/control", self.base_url))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:AVTransport:1#GetDeviceCapabilities\"",
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

    /// Same as [`Self::set_volume`] but with a caller-supplied `InstanceID`
    /// — exists only to exercise the `InvalidInstanceID` (718) rejection
    /// path for a non-zero id nothing was ever created under.
    pub async fn set_volume_with_instance_id(
        &self,
        instance_id: u32,
        volume: u16,
    ) -> reqwest::Response {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:SetVolume xmlns:u="urn:schemas-upnp-org:service:RenderingControl:1">
                        <InstanceID>{}</InstanceID>
                        <Channel>Master</Channel>
                        <DesiredVolume>{}</DesiredVolume>
                    </u:SetVolume>
                </s:Body>
            </s:Envelope>"#,
            instance_id, volume
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

    pub async fn set_volume_db(&self, volume_db: i16) -> reqwest::Response {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:SetVolumeDB xmlns:u="urn:schemas-upnp-org:service:RenderingControl:1">
                        <InstanceID>0</InstanceID>
                        <Channel>Master</Channel>
                        <DesiredVolume>{}</DesiredVolume>
                    </u:SetVolumeDB>
                </s:Body>
            </s:Envelope>"#,
            volume_db
        );

        self.client
            .post(format!(
                "{}/services/RenderingControl/control",
                self.base_url
            ))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:RenderingControl:1#SetVolumeDB\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_volume_db(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetVolumeDB xmlns:u="urn:schemas-upnp-org:service:RenderingControl:1">
                        <InstanceID>0</InstanceID>
                        <Channel>Master</Channel>
                    </u:GetVolumeDB>
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
                "\"urn:schemas-upnp-org:service:RenderingControl:1#GetVolumeDB\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_volume_db_range(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetVolumeDBRange xmlns:u="urn:schemas-upnp-org:service:RenderingControl:1">
                        <InstanceID>0</InstanceID>
                        <Channel>Master</Channel>
                    </u:GetVolumeDBRange>
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
                "\"urn:schemas-upnp-org:service:RenderingControl:1#GetVolumeDBRange\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn list_presets(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:ListPresets xmlns:u="urn:schemas-upnp-org:service:RenderingControl:1">
                        <InstanceID>0</InstanceID>
                    </u:ListPresets>
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
                "\"urn:schemas-upnp-org:service:RenderingControl:1#ListPresets\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn select_preset(&self, preset_name: &str) -> reqwest::Response {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:SelectPreset xmlns:u="urn:schemas-upnp-org:service:RenderingControl:1">
                        <InstanceID>0</InstanceID>
                        <PresetName>{}</PresetName>
                    </u:SelectPreset>
                </s:Body>
            </s:Envelope>"#,
            preset_name
        );

        self.client
            .post(format!(
                "{}/services/RenderingControl/control",
                self.base_url
            ))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:RenderingControl:1#SelectPreset\"",
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

    /// Self-closing zero-argument form, matching how real clients (e.g.
    /// BubbleUPnP) actually send it on the wire.
    pub async fn get_protocol_info(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetProtocolInfo xmlns:u="urn:schemas-upnp-org:service:ConnectionManager:3"/>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!(
                "{}/services/ConnectionManager/control",
                self.base_url
            ))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:ConnectionManager:3#GetProtocolInfo\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    /// Same as [`Self::get_protocol_info`] but with a `:1` `SOAPAction` —
    /// exercises that a control point which only knows about
    /// `ConnectionManager:1` (this renderer used to only advertise `:1`)
    /// still gets served after the bump to `:3`.
    pub async fn get_protocol_info_v1(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetProtocolInfo xmlns:u="urn:schemas-upnp-org:service:ConnectionManager:1"/>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!(
                "{}/services/ConnectionManager/control",
                self.base_url
            ))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:ConnectionManager:1#GetProtocolInfo\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_current_connection_ids(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetCurrentConnectionIDs xmlns:u="urn:schemas-upnp-org:service:ConnectionManager:3"/>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!(
                "{}/services/ConnectionManager/control",
                self.base_url
            ))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:ConnectionManager:3#GetCurrentConnectionIDs\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_current_connection_info(&self, connection_id: i32) -> reqwest::Response {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetCurrentConnectionInfo xmlns:u="urn:schemas-upnp-org:service:ConnectionManager:3">
                        <ConnectionID>{}</ConnectionID>
                    </u:GetCurrentConnectionInfo>
                </s:Body>
            </s:Envelope>"#,
            connection_id
        );

        self.client
            .post(format!(
                "{}/services/ConnectionManager/control",
                self.base_url
            ))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:ConnectionManager:3#GetCurrentConnectionInfo\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn prepare_for_connection(
        &self,
        remote_protocol_info: &str,
        direction: &str,
    ) -> reqwest::Response {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:PrepareForConnection xmlns:u="urn:schemas-upnp-org:service:ConnectionManager:3">
                        <RemoteProtocolInfo>{}</RemoteProtocolInfo>
                        <PeerConnectionManager></PeerConnectionManager>
                        <PeerConnectionID>-1</PeerConnectionID>
                        <Direction>{}</Direction>
                    </u:PrepareForConnection>
                </s:Body>
            </s:Envelope>"#,
            remote_protocol_info, direction
        );

        self.client
            .post(format!(
                "{}/services/ConnectionManager/control",
                self.base_url
            ))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:ConnectionManager:3#PrepareForConnection\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn connection_complete(&self, connection_id: i32) -> reqwest::Response {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:ConnectionComplete xmlns:u="urn:schemas-upnp-org:service:ConnectionManager:3">
                        <ConnectionID>{}</ConnectionID>
                    </u:ConnectionComplete>
                </s:Body>
            </s:Envelope>"#,
            connection_id
        );

        self.client
            .post(format!(
                "{}/services/ConnectionManager/control",
                self.base_url
            ))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:ConnectionManager:3#ConnectionComplete\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }

    pub async fn get_feature_list(&self) -> reqwest::Response {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
            <s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
                <s:Body>
                    <u:GetFeatureList xmlns:u="urn:schemas-upnp-org:service:ConnectionManager:3"/>
                </s:Body>
            </s:Envelope>"#;

        self.client
            .post(format!(
                "{}/services/ConnectionManager/control",
                self.base_url
            ))
            .header("Content-Type", "text/xml")
            .header(
                "SOAPAction",
                "\"urn:schemas-upnp-org:service:ConnectionManager:3#GetFeatureList\"",
            )
            .body(body)
            .send()
            .await
            .unwrap()
    }
}
