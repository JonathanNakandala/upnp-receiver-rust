use thiserror::Error;

#[derive(Error, Debug)]
pub enum DiscoveryServiceError {
    #[error("missing required field: {0}")]
    MissingField(String),
    #[error("invalid value: {0}")]
    InvalidValue(String),
    #[error("network error: {0}")]
    NetworkError(#[from] std::io::Error),
    #[error("protocol error: {0}")]
    ProtocolError(String),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("xml error: {0}")]
    Xml(#[from] quick_xml::de::DeError),
}