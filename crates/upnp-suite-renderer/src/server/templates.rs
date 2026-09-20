use askama::Template;

/// The subset of a loaded [`crate::server::DeviceIcon`] the `device.xml`
/// template needs — kept separate so the template doesn't reach into the
/// icon's raw bytes.
pub struct DeviceIconTemplateData {
    pub url: String,
    pub mimetype: &'static str,
    pub width: u32,
    pub height: u32,
}

#[derive(Template)]
#[template(path = "media_renderer/device.xml")]
pub struct MediaRendererDeviceTemplate {
    pub friendly_name: String,
    pub manufacturer: String,
    pub manufacturer_url: String,
    pub model_description: String,
    pub model_name: String,
    pub model_number: String,
    pub model_url: String,
    pub serial_number: String,
    pub uuid: String,
    pub icon: Option<DeviceIconTemplateData>,
}

#[derive(Template)]
#[template(path = "media_renderer/services/av_transport.xml")]
pub struct MediaRendererAvTransportTemplate;

#[derive(Template)]
#[template(path = "media_renderer/services/rendering_control.xml")]
pub struct MediaRendererRenderingControlTemplate;

#[derive(Template)]
#[template(path = "media_renderer/services/connection_manager.xml")]
pub struct MediaRendererConnectionManagerTemplate;