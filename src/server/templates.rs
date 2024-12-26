use askama::Template;

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