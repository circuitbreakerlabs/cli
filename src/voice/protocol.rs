//! Voice protocol v1. Audio is 24 kHz mono signed little-endian PCM16.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use super::{
    Error, Result,
    common_protocol::{ProtocolVersion, SessionErrorCode, VoiceErrorCode, VoiceTarget},
};
use crate::protocol_types::{
    common::{ConversationComplete, ConversationError},
    multi_turn::{MultiTurnEvaluationStart, MultiTurnResponse},
    single_turn::{IterationComplete, IterationStart, SingleTurnResponse},
};

pub const FRAME_BYTES: usize = 960;
pub const QUEUE_FRAMES: usize = 100;
pub const MAX_CONTROL_BYTES: usize = 65_536;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AudioHeader {
    pub session_id: i32,
    pub stream_id: u64,
    pub sequence: u64,
    pub sample_offset: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Audio {
    pub header: AudioHeader,
    pub pcm: Vec<u8>,
}

impl Audio {
    // Outbound audio is produced by adapters; portable builds currently have none.
    #[cfg_attr(not(any(test, feature = "livekit")), allow(dead_code))]
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let header = serde_json::to_vec(&self.header).map_err(|_| Error::Protocol)?;
        let mut data = u32::try_from(header.len())
            .map_err(|_| Error::Protocol)?
            .to_be_bytes()
            .to_vec();
        data.extend(header);
        data.extend(&self.pcm);
        Ok(data)
    }

    pub fn decode(data: &[u8]) -> Result<Self> {
        let prefix = data.get(..4).ok_or(Error::Protocol)?;
        let size = u32::from_be_bytes(prefix.try_into().map_err(|_| Error::Protocol)?) as usize;
        if size > 1024 || data.len() > 4 + 1024 + FRAME_BYTES {
            return Err(Error::Protocol);
        }
        let header = data.get(4..4 + size).ok_or(Error::Protocol)?;
        let audio = Self {
            header: serde_json::from_slice(header).map_err(|_| Error::Protocol)?,
            pcm: data.get(4 + size..).ok_or(Error::Protocol)?.to_vec(),
        };
        audio.validate()?;
        Ok(audio)
    }

    fn validate(&self) -> Result<()> {
        if self.pcm.is_empty() || self.pcm.len() > FRAME_BYTES || !self.pcm.len().is_multiple_of(2)
        {
            return Err(Error::Protocol);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    SessionOpen {
        session_id: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        conversation_id: Option<i32>,
        max_turns: u32,
        timeout_ms: u64,
    },
    UtteranceStart {
        session_id: i32,
        stream_id: u64,
        text: String,
        timeout_ms: u64,
    },
    UtteranceEnd {
        session_id: i32,
        stream_id: u64,
    },
    PlaybackCancel {
        session_id: i32,
    },
    SessionClose {
        session_id: i32,
    },
}

impl Command {
    pub fn session_id(&self) -> i32 {
        match self {
            Self::SessionOpen { session_id, .. }
            | Self::UtteranceStart { session_id, .. }
            | Self::UtteranceEnd { session_id, .. }
            | Self::PlaybackCancel { session_id }
            | Self::SessionClose { session_id } => *session_id,
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Self::SessionOpen {
                max_turns,
                timeout_ms,
                ..
            } if *max_turns == 0 || *timeout_ms == 0 => Err(Error::Protocol),
            Self::UtteranceStart {
                timeout_ms, text, ..
            } if *timeout_ms == 0 || text.len() > 65_536 => Err(Error::Protocol),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequestEnvelope<T> {
    pub version: ProtocolVersion,
    #[serde(rename = "type")]
    pub kind: RequestKind,
    pub data: T,
    pub target: VoiceTarget,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    VoiceRequest,
    VoiceRerunRequest,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[allow(clippy::enum_variant_names)]
pub enum EndpointEvent<R> {
    VoiceReady {
        version: ProtocolVersion,
    },
    VoiceError {
        code: VoiceErrorCode,
        #[serde(default)]
        status: Option<u16>,
    },
    VoiceResult {
        data: R,
    },
}

impl<R> EndpointEvent<R> {
    pub fn validate(&self) -> Result<()> {
        if let Self::VoiceError {
            status: Some(status),
            ..
        } = self
            && !(100..600).contains(status)
        {
            return Err(Error::Protocol);
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum SingleTurnProgressEvent {
    IterationStart(IterationStart),
    IterationComplete(IterationComplete),
    ConversationComplete(ConversationComplete),
    ConversationError(ConversationError),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum MultiTurnProgressEvent {
    MultiTurnEvaluationStart(MultiTurnEvaluationStart),
    ConversationComplete(ConversationComplete),
    ConversationError(ConversationError),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ServerMessage<R, P> {
    Endpoint(EndpointEvent<R>),
    Progress(P),
    Command(Command),
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    SessionReady {
        session_id: i32,
        capabilities: Capabilities,
        external_session_id: Option<String>,
    },
    SessionError {
        session_id: i32,
        code: SessionErrorCode,
    },
    SessionClosed {
        session_id: i32,
    },
    UtteranceReady {
        session_id: i32,
        stream_id: u64,
    },
    AudioConsumed {
        session_id: i32,
        stream_id: u64,
    },
    PlaybackComplete {
        session_id: i32,
        stream_id: u64,
        elapsed_ms: u128,
    },
    ResponseStart {
        session_id: i32,
        stream_id: u64,
        elapsed_ms: u128,
    },
    ResponseEnd {
        session_id: i32,
        stream_id: u64,
        samples: u64,
        transcript: Option<String>,
        elapsed_ms: u128,
    },
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Capabilities {
    caller_audio: bool,
    customer_audio: bool,
    response_end: bool,
    remote_interrupt: bool,
}

impl Capabilities {
    pub const REQUIRED: Self = Self {
        caller_audio: true,
        customer_audio: true,
        response_end: true,
        remote_interrupt: false,
    };
}

#[derive(Debug)]
pub enum Output {
    Event(SessionEvent),
    Audio(Audio),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceTurnMetadata {
    pub turn_index: u64,
    pub original_text: String,
    pub tagged_text: String,
    pub transcription: String,
    pub tts_model: Option<String>,
    pub stt_model: Option<String>,
    pub tts_duration_seconds: f64,
    pub response_duration_seconds: f64,
    pub input_sample_rate: u32,
    pub output_sample_rate: u32,
    pub comparison_transcript: Option<String>,
    pub caller_audio_path: Option<String>,
    pub customer_audio_path: Option<String>,
    #[serde(default)]
    pub stage_timings: BTreeMap<String, f64>,
    #[serde(default)]
    pub transport_metadata: BTreeMap<String, Value>,
    pub artifact_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnVoiceMetadata {
    #[serde(flatten)]
    pub turn: VoiceTurnMetadata,
    pub attempt_index: u64,
    pub session_id: i32,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleTurnVoiceResponse {
    #[serde(flatten)]
    pub result: SingleTurnResponse,
    pub target: VoiceTarget,
    pub voice_metadata: BTreeMap<String, Vec<SingleTurnVoiceMetadata>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTurnVoiceResponse {
    #[serde(flatten)]
    pub result: MultiTurnResponse,
    pub target: VoiceTarget,
    pub voice_metadata: BTreeMap<String, Vec<VoiceTurnMetadata>>,
}

#[derive(Debug)]
pub enum Input {
    Control(Command),
    Audio(Audio),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_round_trips_and_rejects_invalid_frames() {
        let audio = Audio {
            header: AudioHeader {
                session_id: 7,
                stream_id: 1,
                sequence: 0,
                sample_offset: 0,
            },
            pcm: vec![0, 1, 2, 3],
        };
        assert_eq!(Audio::decode(&audio.encode().unwrap()).unwrap(), audio);
        for invalid in [vec![], vec![0, 0, 255, 255], vec![0; 2000]] {
            assert!(Audio::decode(&invalid).is_err());
        }
        let mut invalid = audio;
        invalid.pcm.push(0);
        assert!(invalid.encode().is_err());
    }
    #[test]
    fn matches_shared_python_audio_fixture() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/voice/audio.json")).unwrap();
        let wire: Vec<u8> = fixture["wire_hex"]
            .as_str()
            .unwrap()
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|hex| u8::from_str_radix(std::str::from_utf8(hex).unwrap(), 16).unwrap())
            .collect();
        let audio = Audio::decode(&wire).unwrap();
        assert_eq!(
            serde_json::to_value(&audio.header).unwrap(),
            fixture["header"]
        );
        assert_eq!(audio.pcm, [0, 1, 2, 3]);
        assert_eq!(audio.encode().unwrap(), wire);
    }

    #[test]
    fn session_open_accepts_optional_conversation_id() {
        let command: Command = serde_json::from_value(serde_json::json!({
            "type": "session_open",
            "session_id": 17,
            "conversation_id": 4,
            "max_turns": 1,
            "timeout_ms": 1000
        }))
        .expect("session open should deserialize");
        assert!(matches!(
            command,
            Command::SessionOpen {
                session_id: 17,
                conversation_id: Some(4),
                max_turns: 1,
                timeout_ms: 1000
            }
        ));
    }

    #[test]
    fn v1_session_events_keep_the_legacy_wire_contract() {
        let event = SessionEvent::SessionReady {
            session_id: 17,
            capabilities: Capabilities::REQUIRED,
            external_session_id: Some("provider-session".into()),
        };
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            serde_json::json!({
                "type": "session_ready",
                "session_id": 17,
                "capabilities": {
                    "caller_audio": true,
                    "customer_audio": true,
                    "response_end": true,
                    "remote_interrupt": false
                },
                "external_session_id": "provider-session"
            })
        );
        let playback_cancel: Command = serde_json::from_value(serde_json::json!({
            "type": "playback_cancel",
            "session_id": 17
        }))
        .unwrap();
        assert!(matches!(playback_cancel, Command::PlaybackCancel { .. }));
    }
}
