//! Multiparty voice protocol v2. Audio is 24 kHz mono signed little-endian PCM16.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::{
    Error, Result,
    common_protocol::{ProtocolVersion, SessionErrorCode, VoiceErrorCode, VoiceTarget},
};
use crate::{
    protocol_types::multi_party::MultiPartyVoiceResponse,
    tui::multiparty::MultiPartyProgressIndicatorMessage,
};

pub const FRAME_BYTES: usize = 960;
pub const QUEUE_FRAMES: usize = 100;
pub const MAX_CONTROL_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum GeminiVoice {
    Aoede,
    Charon,
    Fenrir,
    Kore,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryMode {
    SingleVirtualParticipant,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LogicalParticipant {
    pub id: String,
    pub display_name: String,
    pub voice: GeminiVoice,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExchangeInput {
    pub utterance_id: u64,
    pub speaker_id: String,
    pub display_name: String,
    pub voice: GeminiVoice,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AudioHeader {
    pub session_id: i32,
    pub exchange_id: u64,
    pub sequence: u64,
    pub sample_offset: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Audio {
    pub header: AudioHeader,
    pub pcm: Vec<u8>,
}

impl Audio {
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let header = serde_json::to_vec(&self.header).map_err(|_| Error::Protocol)?;
        if header.len() > 1024 {
            return Err(Error::Protocol);
        }
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
        if self.header.exchange_id == 0
            || self.pcm.is_empty()
            || self.pcm.len() > FRAME_BYTES
            || !self.pcm.len().is_multiple_of(2)
        {
            return Err(Error::Protocol);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    SessionOpen {
        session_id: i32,
        conversation_id: i32,
        delivery_mode: DeliveryMode,
        participants: Vec<LogicalParticipant>,
        max_exchanges: u32,
        timeout_ms: u64,
    },
    ExchangeStart {
        session_id: i32,
        exchange_id: u64,
        inputs: Vec<ExchangeInput>,
        timeout_ms: u64,
    },
    ExchangeEnd {
        session_id: i32,
        exchange_id: u64,
    },
    SessionClose {
        session_id: i32,
    },
}

impl Command {
    pub fn session_id(&self) -> i32 {
        match self {
            Self::SessionOpen { session_id, .. }
            | Self::ExchangeStart { session_id, .. }
            | Self::ExchangeEnd { session_id, .. }
            | Self::SessionClose { session_id } => *session_id,
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Self::SessionOpen {
                participants,
                max_exchanges,
                timeout_ms,
                ..
            } => {
                let valid_participants = (2..=4).contains(&participants.len())
                    && participants.iter().all(|participant| {
                        !participant.id.is_empty()
                            && participant.id.len() <= 64
                            && !participant.display_name.trim().is_empty()
                            && participant.display_name.len() <= 256
                    });
                let unique_participants = participants
                    .iter()
                    .map(|participant| participant.id.as_str())
                    .collect::<HashSet<_>>()
                    .len()
                    == participants.len();
                if !valid_participants
                    || !unique_participants
                    || *max_exchanges == 0
                    || *timeout_ms == 0
                {
                    return Err(Error::Protocol);
                }
            }
            Self::ExchangeStart {
                exchange_id,
                inputs,
                timeout_ms,
                ..
            } => {
                let valid_inputs = (1..=4).contains(&inputs.len())
                    && inputs.iter().all(|input| {
                        !input.speaker_id.is_empty()
                            && input.speaker_id.len() <= 64
                            && !input.display_name.trim().is_empty()
                            && input.display_name.len() <= 256
                            && !input.text.trim().is_empty()
                    });
                if *exchange_id == 0 || !valid_inputs || *timeout_ms == 0 {
                    return Err(Error::Protocol);
                }
            }
            Self::ExchangeEnd { exchange_id, .. } if *exchange_id == 0 => {
                return Err(Error::Protocol);
            }
            Self::ExchangeEnd { .. } | Self::SessionClose { .. } => {}
        }
        Ok(())
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
    MultipartyVoiceRequest,
    MultipartyVoiceRerunRequest,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[allow(clippy::enum_variant_names)]
pub enum EndpointEvent {
    VoiceReady {
        version: ProtocolVersion,
    },
    VoiceError {
        code: VoiceErrorCode,
        #[serde(default)]
        status: Option<u16>,
    },
    VoiceResult {
        data: MultiPartyVoiceResponse,
    },
}

impl EndpointEvent {
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
#[serde(untagged)]
pub enum ServerMessage {
    Endpoint(EndpointEvent),
    Progress(MultiPartyProgressIndicatorMessage),
    Command(Command),
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    SessionReady {
        session_id: i32,
        capabilities: MultipartyCapabilities,
        external_session_id: Option<String>,
        livekit_participant_identity: Option<String>,
        livekit_track_id: Option<String>,
    },
    SessionError {
        session_id: i32,
        code: SessionErrorCode,
    },
    SessionClosed {
        session_id: i32,
    },
    ExchangeReady {
        session_id: i32,
        exchange_id: u64,
    },
    AudioConsumed {
        session_id: i32,
        exchange_id: u64,
    },
    PlaybackComplete {
        session_id: i32,
        exchange_id: u64,
        elapsed_ms: f64,
    },
    ResponseStart {
        session_id: i32,
        exchange_id: u64,
        elapsed_ms: f64,
    },
    ResponseEnd {
        session_id: i32,
        exchange_id: u64,
        samples: u64,
        transcript: Option<String>,
        elapsed_ms: f64,
    },
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct MultipartyCapabilities {
    single_virtual_participant: bool,
    exchange_audio: bool,
    customer_audio: bool,
    response_end: bool,
}

impl MultipartyCapabilities {
    pub const REQUIRED: Self = Self {
        single_virtual_participant: true,
        exchange_audio: true,
        customer_audio: true,
        response_end: true,
    };
}

#[derive(Debug)]
pub enum Output {
    Event(SessionEvent),
    Audio(Audio),
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
    fn audio_round_trips() {
        let audio = Audio {
            header: AudioHeader {
                session_id: 7,
                exchange_id: 2,
                sequence: 3,
                sample_offset: 480,
            },
            pcm: vec![1, 0, 2, 0],
        };
        assert_eq!(Audio::decode(&audio.encode().unwrap()).unwrap(), audio);
    }

    #[test]
    fn validates_session_and_exchange_contracts() {
        let participants = vec![
            LogicalParticipant {
                id: "alice".into(),
                display_name: "Alice".into(),
                voice: GeminiVoice::Aoede,
            },
            LogicalParticipant {
                id: "bob".into(),
                display_name: "Bob".into(),
                voice: GeminiVoice::Charon,
            },
        ];
        assert!(
            Command::SessionOpen {
                session_id: 1,
                conversation_id: 1,
                delivery_mode: DeliveryMode::SingleVirtualParticipant,
                participants,
                max_exchanges: 2,
                timeout_ms: 1000,
            }
            .validate()
            .is_ok()
        );
        assert!(
            Command::ExchangeStart {
                session_id: 1,
                exchange_id: 0,
                inputs: vec![],
                timeout_ms: 0,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn v2_rejects_unknown_closed_protocol_values() {
        assert!(serde_json::from_str::<GeminiVoice>(r#""Unknown""#).is_err());
        assert!(serde_json::from_str::<DeliveryMode>(r#""one_participant_per_voice""#).is_err());
    }

    #[test]
    fn v2_session_events_serialize_with_typed_capabilities() {
        let event = SessionEvent::SessionReady {
            session_id: 4,
            capabilities: MultipartyCapabilities::REQUIRED,
            external_session_id: None,
            livekit_participant_identity: Some("agent".into()),
            livekit_track_id: Some("track".into()),
        };
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            serde_json::json!({
                "type": "session_ready",
                "session_id": 4,
                "capabilities": {
                    "single_virtual_participant": true,
                    "exchange_audio": true,
                    "customer_audio": true,
                    "response_end": true
                },
                "external_session_id": null,
                "livekit_participant_identity": "agent",
                "livekit_track_id": "track"
            })
        );
    }
}
