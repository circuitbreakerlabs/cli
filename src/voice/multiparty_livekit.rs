//! Native `LiveKit` adapter for the multiparty voice v2 exchange protocol.
use super::{
    Error, Result,
    config::Config,
    hooks::{
        self, Hooks, ProviderEvent, TargetEvent, V2Action as Action, V2BootstrapContext,
        V2SessionEvent, provider_event,
    },
    multiparty_protocol::{
        Audio, AudioHeader, Command, DeliveryMode, ExchangeInput, FRAME_BYTES, Input,
        MultipartyCapabilities, Output as WireOutput, QUEUE_FRAMES, SessionEvent,
    },
    session::{MultipartyOutput, MultipartySessionSpec, MultipartyVoiceSession},
};
use async_trait::async_trait;
use futures_util::StreamExt;
use livekit::{
    options::TrackPublishOptions,
    prelude::*,
    webrtc::{
        audio_frame::AudioFrame,
        audio_source::{AudioSourceOptions, RtcAudioSource, native::NativeAudioSource},
        audio_stream::native::{NativeAudioStream, NativeAudioStreamOptions},
    },
};
use serde_json::Value;
use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinSet,
    time::Instant,
};

pub async fn connect(
    config: &Config,
    spec: MultipartySessionSpec,
) -> Result<Box<dyn MultipartyVoiceSession>> {
    let nonce = format!(
        "{}-{}-{}",
        std::process::id(),
        spec.session_id,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| Error::Transport)?
            .as_nanos()
    );
    let context = V2BootstrapContext {
        base: config.context(spec.session_id, spec.max_exchanges)?,
        protocol_version: "2",
        conversation_id: spec.conversation_id,
        delivery_mode: DeliveryMode::SingleVirtualParticipant,
        participants: spec.participants.clone(),
        max_exchanges: spec.max_exchanges,
        session_nonce: nonce.clone(),
    };
    let connection = hooks::connect(config, context).await?;
    let hooks = Hooks::new(config)?;
    let (room, events) = Room::connect(&connection.url, &connection.token, RoomOptions::default())
        .await
        .map_err(|_| Error::Transport)?;
    Ok(Box::new(Session {
        room: Arc::new(room),
        events,
        config: config.clone(),
        hooks,
        spec,
        nonce,
        external_session_id: connection.external_session_id,
    }))
}

struct Session {
    room: Arc<Room>,
    events: mpsc::UnboundedReceiver<RoomEvent>,
    config: Config,
    hooks: Hooks,
    spec: MultipartySessionSpec,
    nonce: String,
    external_session_id: Option<String>,
}

#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
struct State {
    exchange: u64,
    input_sequence: u64,
    input_offset: u64,
    sequence: u64,
    offset: u64,
    capturing: bool,
    caller_started: bool,
    caller_generation: u64,
    ready: bool,
    exchange_ready: bool,
    input_ended: bool,
    caller_playing: bool,
    playback_complete: bool,
    response_started: bool,
    response_complete: bool,
    transcript: Option<String>,
    inputs: Vec<ExchangeInput>,
    completion_timeout: Duration,
    completion_deadline: Option<Instant>,
    end_at: Option<Instant>,
}

enum Media {
    Audio { pcm: Vec<u8>, generation: u64 },
    Published,
    Consumed,
    Failed,
}

enum Publish {
    Audio(Vec<u8>),
    End,
}

struct SessionMetadata {
    participant_identity: String,
    track_id: String,
}

#[async_trait]
impl MultipartyVoiceSession for Session {
    async fn run(
        mut self: Box<Self>,
        mut input: mpsc::Receiver<Input>,
        output: MultipartyOutput,
        mut stop: watch::Receiver<bool>,
    ) -> Result<()> {
        let source = NativeAudioSource::new(AudioSourceOptions::default(), 24_000, 1, 0);
        let mut tasks = JoinSet::new();
        let result = tokio::select! {
            biased;
            _ = stop.changed() => Ok(()),
            result = self.drive(&mut input, &output, &source, &mut tasks) => result,
        };
        source.clear_buffer();
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        let _ = tokio::time::timeout(Duration::from_secs(2), self.close_controls()).await;
        let _ = tokio::time::timeout(Duration::from_secs(5), self.room.close()).await;
        result
    }
}

impl Session {
    async fn close_controls(&mut self) -> Result<()> {
        let mut actions = self
            .hooks
            .multiparty_event(false, V2SessionEvent::Closing)?;
        if actions.is_empty() {
            return Ok(());
        }
        loop {
            for action in std::mem::take(&mut actions) {
                match action {
                    Action::CloseReady => return Ok(()),
                    Action::Send { message, topic } => {
                        self.room
                            .local_participant()
                            .publish_data(DataPacket {
                                payload: serde_json::to_vec(&message).map_err(|_| Error::Hook)?,
                                topic,
                                reliable: true,
                                ..Default::default()
                            })
                            .await
                            .map_err(|_| Error::TransportStage("closing control"))?;
                    }
                    _ => return Err(Error::Hook),
                }
            }
            actions = match self.events.recv().await {
                Some(RoomEvent::DataReceived { payload, topic, .. }) => {
                    if payload.len() > 65_536 {
                        return Err(Error::Protocol);
                    }
                    let Ok(message) = serde_json::from_slice::<Value>(&payload) else {
                        continue;
                    };
                    self.hooks.multiparty_event(
                        true,
                        TargetEvent {
                            message,
                            topic,
                            caller_started: None,
                            exchange_id: None,
                        },
                    )?
                }
                None | Some(RoomEvent::Disconnected { .. }) => return Ok(()),
                _ => Vec::new(),
            };
        }
    }

    async fn actions(
        &mut self,
        actions: Vec<Action>,
        state: &mut State,
        output: &MultipartyOutput,
        metadata: &SessionMetadata,
    ) -> Result<()> {
        for action in actions {
            tracing::info!(
                session_id = self.spec.session_id,
                exchange_id = state.exchange,
                action = action.diagnostic_name(),
                "voice_diag: multiparty hook action"
            );
            match action {
                Action::Send { message, topic } => {
                    self.room
                        .local_participant()
                        .publish_data(DataPacket {
                            payload: serde_json::to_vec(&message).map_err(|_| Error::Hook)?,
                            topic,
                            reliable: true,
                            ..Default::default()
                        })
                        .await
                        .map_err(|_| Error::TransportStage("control publication"))?;
                }
                Action::SessionReady => {
                    if state.ready {
                        return Err(Error::Hook);
                    }
                    state.ready = true;
                    emit(
                        output,
                        SessionEvent::SessionReady {
                            session_id: self.spec.session_id,
                            capabilities: MultipartyCapabilities::REQUIRED,
                            external_session_id: self.external_session_id.clone(),
                            livekit_participant_identity: Some(
                                metadata.participant_identity.clone(),
                            ),
                            livekit_track_id: Some(metadata.track_id.clone()),
                        },
                    )
                    .await?;
                }
                Action::ExchangeReady => {
                    if !state.ready || state.exchange == 0 || state.exchange_ready {
                        return Err(Error::Hook);
                    }
                    state.exchange_ready = true;
                    emit(
                        output,
                        SessionEvent::ExchangeReady {
                            session_id: self.spec.session_id,
                            exchange_id: state.exchange,
                        },
                    )
                    .await?;
                }
                Action::CaptureStart => state.capturing = true,
                Action::CaptureStop => state.capturing = false,
                Action::ResponseEnd => {
                    if state.exchange == 0 || state.end_at.is_some() {
                        return Err(Error::Hook);
                    }
                    state.end_at = Some(Instant::now() + Duration::from_millis(200));
                    state.completion_deadline = None;
                }
                Action::Transcript { text } => state.transcript = Some(text),
                Action::Error | Action::CloseReady => {
                    return Err(Error::Hook);
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn drive(
        &mut self,
        input: &mut mpsc::Receiver<Input>,
        output: &MultipartyOutput,
        source: &NativeAudioSource,
        tasks: &mut JoinSet<()>,
    ) -> Result<()> {
        let track = LocalAudioTrack::create_audio_track(
            "cbl-multiparty-voice",
            RtcAudioSource::Native(source.clone()),
        );
        let publication = self
            .room
            .local_participant()
            .publish_track(LocalTrack::Audio(track), TrackPublishOptions::default())
            .await
            .map_err(|_| Error::TransportStage("track publication"))?;
        let metadata = SessionMetadata {
            participant_identity: self.room.local_participant().identity().to_string(),
            track_id: publication.sid().to_string(),
        };
        tracing::info!(
            session_id = self.spec.session_id,
            participant_identity = %metadata.participant_identity,
            track_id = %metadata.track_id,
            "voice_diag: multiparty caller track published"
        );
        let (media_tx, mut media_rx) = mpsc::channel(QUEUE_FRAMES);
        let (publish_tx, mut publish_rx) = mpsc::channel(QUEUE_FRAMES);
        let publisher_source = source.clone();
        let publisher_events = media_tx.clone();
        let caller_generation = Arc::new(AtomicU64::new(0));
        let reset_tx = {
            let (tx, _rx) = watch::channel(0_u64);
            tx
        };
        let publisher_generation = Arc::clone(&caller_generation);
        let publisher_reset = reset_tx.clone();
        tasks.spawn(async move {
            let mut first_frame = true;
            while let Some(command) = publish_rx.recv().await {
                match command {
                    Publish::Audio(pcm) => {
                        for (index, samples) in native_frames(&pcm).into_iter().enumerate() {
                            let frame = AudioFrame {
                                data: Cow::Owned(samples),
                                sample_rate: 24_000,
                                num_channels: 1,
                                samples_per_channel: 240,
                            };
                            if publisher_source.capture_frame(&frame).await.is_err() {
                                let _ = publisher_events.send(Media::Failed).await;
                                return;
                            }
                            if index == 0 && first_frame {
                                first_frame = false;
                                let generation =
                                    publisher_generation.fetch_add(1, Ordering::AcqRel) + 1;
                                publisher_reset.send_replace(generation);
                            }
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                        if publisher_events.send(Media::Consumed).await.is_err() {
                            break;
                        }
                    }
                    Publish::End => {
                        first_frame = true;
                        if publisher_events.send(Media::Published).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        let mut state = State::default();
        let mut caller_gate_rx = reset_tx.subscribe();
        let mut selected = None;
        let started = Instant::now();
        let mut playback_tick = tokio::time::interval(Duration::from_millis(500));
        playback_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let initial = self.hooks.multiparty_event(
            false,
            V2SessionEvent::Connected {
                protocol_version: "2",
                session_id: self.spec.session_id,
                conversation_id: self.spec.conversation_id,
                delivery_mode: DeliveryMode::SingleVirtualParticipant,
                participants: self.spec.participants.clone(),
                max_exchanges: self.spec.max_exchanges,
                parameters: self.config.parameters.clone(),
                session_nonce: self.nonce.clone(),
                playback_ticks: self.config.playback_ticks,
            },
        )?;
        self.actions(initial, &mut state, output, &metadata).await?;

        loop {
            let end_at = state
                .end_at
                .unwrap_or_else(|| Instant::now() + Duration::from_hours(1));
            let completion_deadline = state
                .completion_deadline
                .unwrap_or_else(|| Instant::now() + Duration::from_hours(1));
            tokio::select! {
                _ = playback_tick.tick(), if self.config.playback_ticks && state.caller_playing => {
                    let actions = self.hooks.multiparty_event(false, V2SessionEvent::PlaybackTick { exchange_id: state.exchange })?;
                    self.actions(actions, &mut state, output, &metadata).await?;
                }
                changed = caller_gate_rx.changed() => {
                    if changed.is_err() { return Err(Error::Transport); }
                    state.caller_started = true;
                    state.caller_generation = *caller_gate_rx.borrow();
                    tracing::info!(
                        session_id = self.spec.session_id,
                        exchange_id = state.exchange,
                        elapsed_ms = started.elapsed().as_millis(),
                        "voice_diag: multiparty first caller frame published"
                    );
                }
                () = tokio::time::sleep_until(end_at), if state.end_at.is_some() => {
                    if !state.response_started || state.offset == 0 { return Err(Error::Protocol); }
                    emit(output, SessionEvent::ResponseEnd {
                        session_id: self.spec.session_id,
                        exchange_id: state.exchange,
                        transcript: state.transcript.clone(),
                        samples: state.offset,
                        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                    }).await?;
                    state.end_at = None;
                    state.capturing = false;
                    state.response_complete = true;
                }
                () = tokio::time::sleep_until(completion_deadline), if state.completion_deadline.is_some() => {
                    return Err(Error::Timeout);
                }
                message = input.recv() => match message.ok_or(Error::Transport)? {
                    Input::Control(Command::ExchangeStart { exchange_id, inputs, timeout_ms, .. }) => {
                        if !state.ready
                            || (state.exchange != 0 && !state.response_complete)
                            || exchange_id != state.exchange + 1
                            || exchange_id > u64::from(self.spec.max_exchanges)
                        {
                            return Err(Error::Protocol);
                        }
                        validate_manifest(&self.spec, &inputs)?;
                        state = State {
                            exchange: exchange_id,
                            ready: true,
                            caller_playing: true,
                            inputs,
                            completion_timeout: Duration::from_millis(timeout_ms.min(3_300_000)),
                            ..State::default()
                        };
                        let actions = self.hooks.multiparty_event(false, V2SessionEvent::ExchangeStart {
                            exchange_id: state.exchange,
                            inputs: state.inputs.clone(),
                        })?;
                        self.actions(actions, &mut state, output, &metadata).await?;
                        playback_tick.reset();
                    }
                    Input::Control(Command::ExchangeEnd { exchange_id, .. }) => {
                        if exchange_id != state.exchange
                            || !state.exchange_ready
                            || state.input_ended
                            || state.input_offset == 0
                        {
                            return Err(Error::Protocol);
                        }
                        let actions = self.hooks.multiparty_event(false, V2SessionEvent::ExchangeEnd {
                            exchange_id: state.exchange,
                            inputs: state.inputs.clone(),
                        })?;
                        self.actions(actions, &mut state, output, &metadata).await?;
                        state.input_ended = true;
                        publish_tx.send(Publish::End).await.map_err(|_| Error::Transport)?;
                    }
                    Input::Control(Command::SessionOpen { .. } | Command::SessionClose { .. }) => return Err(Error::Protocol),
                    Input::Audio(audio) => {
                        if !state.exchange_ready
                            || state.input_ended
                            || audio.header.exchange_id != state.exchange
                            || audio.header.sequence != state.input_sequence
                            || audio.header.sample_offset != state.input_offset
                        {
                            return Err(Error::Protocol);
                        }
                        state.input_sequence += 1;
                        state.input_offset += (audio.pcm.len() / 2) as u64;
                        publish_tx.send(Publish::Audio(audio.pcm)).await.map_err(|_| Error::Transport)?;
                    }
                },
                event = self.events.recv() => match event.ok_or(Error::Transport)? {
                    RoomEvent::Disconnected { .. } => return Err(Error::TransportStage("room disconnected")),
                    RoomEvent::DataReceived { payload, topic, participant, .. } => {
                        if payload.len() > 65_536 { return Err(Error::Protocol); }
                        if let Some(expected) = &self.config.participant
                            && participant.as_ref().is_none_or(|participant| participant.identity().to_string() != *expected) { continue; }
                        let Ok(message) = serde_json::from_slice::<Value>(&payload) else { continue; };
                        let event_type = provider_event(&message);
                        let top_level_keys = message
                            .as_object()
                            .map(|object| object.keys().map(String::as_str).collect::<Vec<_>>())
                            .unwrap_or_default();
                        tracing::info!(
                            session_id = self.spec.session_id,
                            exchange_id = state.exchange,
                            event_type = ?event_type,
                            ?top_level_keys,
                            topic = ?topic,
                            caller_started = state.caller_started,
                            elapsed_ms = started.elapsed().as_millis(),
                            "voice_diag: multiparty provider event"
                        );
                        if matches!(event_type, ProviderEvent::Audio) { continue; }
                        let actions = self.hooks.multiparty_event(true, TargetEvent {
                            message,
                            topic,
                            caller_started: Some(state.caller_started),
                            exchange_id: Some(state.exchange),
                        })?;
                        self.actions(actions, &mut state, output, &metadata).await?;
                    }
                    RoomEvent::TrackSubscribed { track:RemoteTrack::Audio(track), participant, publication } => {
                        if self.config.participant.as_ref().is_some_and(|expected| *expected != participant.identity().to_string())
                            || self.config.track.as_ref().is_some_and(|expected| *expected != publication.name()) { continue; }
                        if selected.is_some() { return Err(Error::Configuration("multiple eligible remote audio tracks; select participant/track")); }
                        tracing::info!(
                            session_id = self.spec.session_id,
                            exchange_id = state.exchange,
                            participant_identity = %participant.identity(),
                            track_name = %publication.name(),
                            track_id = %track.sid(),
                            "voice_diag: multiparty remote audio track subscribed"
                        );
                        selected = Some(track.sid());
                        let tx = media_tx.clone();
                        let mut reset_rx = reset_tx.subscribe();
                        tasks.spawn(async move {
                            let mut generation = *reset_rx.borrow();
                            let mut stream = NativeAudioStream::with_options(
                                track.rtc_track(), 24_000, 1,
                                NativeAudioStreamOptions { queue_size_frames: Some(200) },
                            );
                            loop {
                                tokio::select! {
                                    biased;
                                    changed = reset_rx.changed() => {
                                        if changed.is_err() { break; }
                                        generation = *reset_rx.borrow();
                                        stream.close();
                                        stream = NativeAudioStream::with_options(
                                            track.rtc_track(), 24_000, 1,
                                            NativeAudioStreamOptions { queue_size_frames: Some(200) },
                                        );
                                    }
                                    frame = stream.next() => {
                                        let Some(frame) = frame else { break; };
                                        let bytes: Vec<u8> = frame.data.iter().flat_map(|sample| sample.to_le_bytes()).collect();
                                        for pcm in bytes.chunks(FRAME_BYTES) {
                                            if tx.try_send(Media::Audio { pcm: pcm.to_vec(), generation }).is_err() {
                                                let _ = tx.send(Media::Failed).await;
                                                stream.close();
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                            stream.close();
                        });
                    }
                    _ => {}
                },
                media = media_rx.recv() => match media.ok_or(Error::Transport)? {
                    Media::Failed => return Err(Error::TransportStage("media stream")),
                    Media::Consumed => emit(output, SessionEvent::AudioConsumed {
                        session_id: self.spec.session_id,
                        exchange_id: state.exchange,
                    }).await?,
                    Media::Published => {
                        if !state.input_ended { return Err(Error::Protocol); }
                        state.caller_playing = false;
                        state.playback_complete = true;
                        tracing::info!(
                            session_id = self.spec.session_id,
                            exchange_id = state.exchange,
                            elapsed_ms = started.elapsed().as_millis(),
                            "voice_diag: multiparty playback complete"
                        );
                        emit(output, SessionEvent::PlaybackComplete {
                            session_id: self.spec.session_id,
                            exchange_id: state.exchange,
                            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                        }).await?;
                        let actions = self.hooks.multiparty_event(false, V2SessionEvent::PlaybackComplete {
                            exchange_id: state.exchange,
                            inputs: state.inputs.clone(),
                        })?;
                        self.actions(actions, &mut state, output, &metadata).await?;
                    }
                    Media::Audio { pcm, generation } => {
                        if !customer_audio_allowed(
                            state.capturing,
                            state.caller_started,
                            state.caller_generation,
                            generation,
                            state.playback_complete,
                        ) { continue; }
                        if !state.response_started {
                            state.response_started = true;
                            tracing::info!(
                                session_id = self.spec.session_id,
                                exchange_id = state.exchange,
                                elapsed_ms = started.elapsed().as_millis(),
                                "voice_diag: multiparty first response frame received"
                            );
                            emit(output, SessionEvent::ResponseStart {
                                session_id: self.spec.session_id,
                                exchange_id: state.exchange,
                                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                            }).await?;
                        }
                        if state.end_at.is_none() {
                            state.completion_deadline = Some(Instant::now() + state.completion_timeout);
                        }
                        let audio = Audio {
                            header: AudioHeader {
                                session_id:self.spec.session_id,
                                exchange_id:state.exchange,
                                sequence:state.sequence,
                                sample_offset:state.offset,
                            },
                            pcm,
                        };
                        state.sequence += 1;
                        state.offset += (audio.pcm.len() / 2) as u64;
                        output.send(WireOutput::Audio(audio)).await.map_err(|_| Error::Transport)?;
                    }
                }
            }
        }
    }
}

fn validate_manifest(spec: &MultipartySessionSpec, inputs: &[ExchangeInput]) -> Result<()> {
    let participants: HashMap<_, _> = spec
        .participants
        .iter()
        .map(|participant| (participant.id.as_str(), participant.voice))
        .collect();
    if inputs
        .iter()
        .any(|input| participants.get(input.speaker_id.as_str()).copied() != Some(input.voice))
    {
        return Err(Error::Protocol);
    }
    Ok(())
}

async fn emit(output: &MultipartyOutput, event: SessionEvent) -> Result<()> {
    output
        .send(WireOutput::Event(event))
        .await
        .map_err(|_| Error::Transport)
}

fn native_frames(pcm: &[u8]) -> Vec<Vec<i16>> {
    pcm.chunks(480)
        .map(|chunk| {
            let mut samples: Vec<i16> = chunk
                .as_chunks::<2>()
                .0
                .iter()
                .map(|bytes| i16::from_le_bytes(*bytes))
                .collect();
            samples.resize(240, 0);
            samples
        })
        .collect()
}

fn customer_audio_allowed(
    capturing: bool,
    caller_started: bool,
    caller_generation: u64,
    frame_generation: u64,
    playback_complete: bool,
) -> bool {
    capturing && caller_started && frame_generation >= caller_generation && playback_complete
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_audio_waits_for_playback_completion() {
        assert!(!customer_audio_allowed(true, true, 1, 1, false));
        assert!(customer_audio_allowed(true, true, 1, 1, true));
    }

    #[test]
    fn manifest_must_match_server_supplied_participant_voice() {
        let spec = MultipartySessionSpec {
            session_id: 1,
            conversation_id: 1,
            participants: vec![super::super::multiparty_protocol::LogicalParticipant {
                id: "alice".into(),
                display_name: "Alice".into(),
                voice: super::super::multiparty_protocol::GeminiVoice::Aoede,
            }],
            max_exchanges: 1,
        };
        let input = ExchangeInput {
            utterance_id: 1,
            speaker_id: "alice".into(),
            display_name: "Alice".into(),
            voice: super::super::multiparty_protocol::GeminiVoice::Charon,
            text: "hello".into(),
        };
        assert!(validate_manifest(&spec, &[input]).is_err());
    }
}
