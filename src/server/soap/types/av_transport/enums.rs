use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransportState {
    Stopped,
    Playing,
    Transitioning,
    PausedPlayback,
    PausedRecording,
    Recording,
    NoMediaPresent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransportStatus {
    OK,
    ErrorOccurred,
}

/// `CurrentPlayMode` (AVTransport:1 §5.2.9). Only the values meaningful
/// without a real playlist/queue engine are supported — `RANDOM`,
/// `DIRECT_1` and `INTRO` are left out of the SCPD's `allowedValueList`
/// rather than accepted and silently ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlayMode {
    #[default]
    Normal,
    Shuffle,
    RepeatOne,
    RepeatAll,
}

impl PlayMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "NORMAL" => Some(Self::Normal),
            "SHUFFLE" => Some(Self::Shuffle),
            "REPEAT_ONE" => Some(Self::RepeatOne),
            "REPEAT_ALL" => Some(Self::RepeatAll),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Shuffle => "SHUFFLE",
            Self::RepeatOne => "REPEAT_ONE",
            Self::RepeatAll => "REPEAT_ALL",
        }
    }
}
