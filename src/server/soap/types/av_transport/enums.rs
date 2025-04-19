use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum TransportState {
    STOPPED,
    PLAYING,
    TRANSITIONING,
    PAUSED_PLAYBACK,
    PAUSED_RECORDING,
    RECORDING,
    NO_MEDIA_PRESENT,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum TransportStatus {
    OK,
    ERROR_OCCURRED,
}
