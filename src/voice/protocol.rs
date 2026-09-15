//! Voice protocol v1. Audio is 24 kHz mono signed little-endian PCM16.
use serde::{Deserialize, Serialize};

use super::{Error, Result};

pub const VERSION: &str = "1";
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
            .chunks_exact(2)
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
}
