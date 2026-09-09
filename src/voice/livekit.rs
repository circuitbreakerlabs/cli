//! Native media and lifecycle; customer-specific control is confined to hooks.
use super::{
    Error, Result,
    config::Config,
    hooks::{self, Action, Hooks},
    protocol::{Audio, AudioHeader, Command, FRAME_BYTES, Input, QUEUE_FRAMES},
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
use serde_json::{Value, json};
use std::{borrow::Cow, sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, watch},
    task::JoinSet,
    time::Instant,
};
use tokio_tungstenite::tungstenite::Message;

pub type Output = mpsc::Sender<Message>;

#[async_trait]
pub trait VoiceSessionProvider: Send + Sync {
    async fn connect(&self, session_id: i32, max_turns: u32) -> Result<Box<dyn VoiceSession>>;
}
#[async_trait]
pub trait VoiceSession: Send {
    async fn run(
        self: Box<Self>,
        input: mpsc::Receiver<Input>,
        output: Output,
        stop: watch::Receiver<bool>,
    ) -> Result<()>;
}

pub struct Provider(pub Config);
#[async_trait]
impl VoiceSessionProvider for Provider {
    async fn connect(&self, session_id: i32, max_turns: u32) -> Result<Box<dyn VoiceSession>> {
        let nonce = format!(
            "{}-{}-{}",
            std::process::id(),
            session_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| Error::Transport)?
                .as_nanos()
        );
        let mut context = self.0.context(session_id, max_turns)?;
        context["session_nonce"] = json!(nonce);
        let connection = hooks::connect(&self.0, context).await?;
        let hooks = Hooks::new(&self.0)?;
        let (room, events) =
            Room::connect(&connection.url, &connection.token, RoomOptions::default())
                .await
                .map_err(|_| Error::Transport)?;
        Ok(Box::new(Session {
            room: Arc::new(room),
            events,
            config: self.0.clone(),
            hooks,
            id: session_id,
            max_turns,
            nonce,
            external_session_id: connection.external_session_id,
        }))
    }
}

struct Session {
    room: Arc<Room>,
    events: mpsc::UnboundedReceiver<RoomEvent>,
    config: Config,
    hooks: Hooks,
    id: i32,
    max_turns: u32,
    external_session_id: Option<String>,
    nonce: String,
}

pub async fn emit(output: &Output, event: Value) -> Result<()> {
    output
        .send(Message::Text(event.to_string().into()))
        .await
        .map_err(|_| Error::Transport)
}

#[derive(Default)]
#[allow(clippy::struct_excessive_bools)] // Independent observed lifecycle milestones.
struct State {
    stream: u64,
    input_sequence: u64,
    input_offset: u64,
    sequence: u64,
    offset: u64,
    capturing: bool,
    published: bool,
    started: bool,
    ready: bool,
    utterance_ready: bool,
    transcript: Option<String>,
    end_at: Option<Instant>,
}

enum Media {
    Audio(Vec<u8>),
    Published,
    Consumed,
    Failed,
}
enum Publish {
    Audio(Vec<u8>),
    End,
}

#[async_trait]
impl VoiceSession for Session {
    async fn run(
        mut self: Box<Self>,
        mut input: mpsc::Receiver<Input>,
        output: Output,
        mut stop: watch::Receiver<bool>,
    ) -> Result<()> {
        let source = NativeAudioSource::new(AudioSourceOptions::default(), 24_000, 1, 0);
        let mut tasks = JoinSet::new();
        let result = tokio::select! {
            result = self.drive(&mut input, &output, &source, &mut tasks) => result,
            _ = stop.changed() => Ok(()),
        };
        source.clear_buffer();
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        let _ = tokio::time::timeout(Duration::from_secs(2), self.close_controls()).await;
        let _ = tokio::time::timeout(Duration::from_secs(5), self.room.close()).await;
        if result.is_ok() {
            let _ = emit(
                &output,
                json!({"type":"session_closed", "session_id":self.id}),
            )
            .await;
        }
        result
    }
}

impl Session {
    async fn close_controls(&mut self) -> Result<()> {
        let mut actions = self.hooks.event(false, json!({"type":"closing"}))?;
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
                    self.hooks
                        .event(true, json!({"message":message,"topic":topic}))?
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
        output: &Output,
    ) -> Result<()> {
        for action in actions {
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
                    emit(output, json!({"type":"session_ready", "session_id":self.id,
                        "capabilities":{"caller_audio":true,"customer_audio":true,"response_end":true,"remote_interrupt":false},
                        "external_session_id":self.external_session_id})).await?;
                }
                Action::UtteranceReady => {
                    if state.stream == 0 || state.utterance_ready {
                        return Err(Error::Hook);
                    }
                    state.utterance_ready = true;
                    emit(output, json!({"type":"utterance_ready", "session_id":self.id,"stream_id":state.stream})).await?;
                }
                Action::CaptureStart => state.capturing = true,
                Action::CaptureStop => state.capturing = false,
                Action::ResponseEnd => {
                    if state.stream == 0 || state.end_at.is_some() {
                        return Err(Error::Hook);
                    }
                    // Control and RTP are independent. Keep a short bounded media drain
                    // after the explicit end event; this is not silence turn detection.
                    state.end_at = Some(Instant::now() + Duration::from_millis(200));
                }
                Action::Transcript { text } => state.transcript = Some(text),
                Action::Error | Action::CloseReady => return Err(Error::Hook),
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // One select loop owns all session state transitions.
    async fn drive(
        &mut self,
        input: &mut mpsc::Receiver<Input>,
        output: &Output,
        source: &NativeAudioSource,
        tasks: &mut JoinSet<()>,
    ) -> Result<()> {
        let track = LocalAudioTrack::create_audio_track(
            "cbl-caller",
            RtcAudioSource::Native(source.clone()),
        );
        self.room
            .local_participant()
            .publish_track(LocalTrack::Audio(track), TrackPublishOptions::default())
            .await
            .map_err(|_| Error::TransportStage("track publication"))?;
        let (media_tx, mut media_rx) = mpsc::channel(QUEUE_FRAMES);
        let (publish_tx, mut publish_rx) = mpsc::channel(QUEUE_FRAMES);
        let publisher_source = source.clone();
        let publisher_events = media_tx.clone();
        tasks.spawn(async move {
            while let Some(command) = publish_rx.recv().await {
                match command {
                    Publish::Audio(pcm) => {
                        // LiveKit's unbuffered source requires exactly 10 ms,
                        // independently of the 20 ms evaluation wire framing.
                        for samples in native_frames(&pcm) {
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
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                        if publisher_events.send(Media::Consumed).await.is_err() {
                            break;
                        }
                    }
                    Publish::End => {
                        if publisher_events.send(Media::Published).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let mut state = State::default();
        let mut selected = None;
        let mut text = String::new();
        let started = Instant::now();
        let initial = self.hooks.event(false, json!({"type":"connected","session_id":self.id,"max_turns":self.max_turns,"parameters":self.config.parameters,"session_nonce":self.nonce}))?;
        self.actions(initial, &mut state, output).await?;
        loop {
            let end_at = state
                .end_at
                .unwrap_or_else(|| Instant::now() + Duration::from_hours(1));
            tokio::select! {
                () = tokio::time::sleep_until(end_at), if state.end_at.is_some() => {
                    emit(output, json!({"type":"response_end","session_id":self.id,"stream_id":state.stream,
                        "transcript":state.transcript,"samples":state.offset,"elapsed_ms":started.elapsed().as_millis()})).await?;
                    state.end_at = None;
                    state.capturing = false;
                }
                message = input.recv() => match message.ok_or(Error::Transport)? {
                    Input::Control(Command::SessionClose { .. }) => return Ok(()),
                    Input::Control(Command::PlaybackCancel { .. }) => { source.clear_buffer(); return Err(Error::Cancelled); }
                    Input::Control(Command::UtteranceStart { stream_id, text: original, .. }) => {
                        if !state.ready || stream_id <= state.stream { return Err(Error::Protocol); }
                        state = State { stream:stream_id, ready:true, published:state.published, ..State::default() };
                        text = original;
                        let actions = self.hooks.event(false, json!({"type":"utterance_start","stream_id":stream_id,"text":text}))?;
                        self.actions(actions, &mut state, output).await?;
                    }
                    Input::Control(Command::UtteranceEnd { stream_id, .. }) => {
                        if stream_id != state.stream { return Err(Error::Protocol); }
                        publish_tx.send(Publish::End).await.map_err(|_| Error::Transport)?;
                    }
                    Input::Control(Command::SessionOpen { .. }) => return Err(Error::Protocol),
                    Input::Audio(audio) => {
                        if !state.utterance_ready || audio.header.stream_id != state.stream || audio.header.sequence != state.input_sequence || audio.header.sample_offset != state.input_offset { return Err(Error::Protocol); }
                        state.input_sequence += 1;
                        state.input_offset += (audio.pcm.len() / 2) as u64;
                        state.published = true;
                        publish_tx
                            .send(Publish::Audio(audio.pcm))
                            .await
                            .map_err(|_| Error::Transport)?;
                    }
                },
                event = self.events.recv() => match event.ok_or(Error::Transport)? {
                    RoomEvent::Disconnected { .. } => return Err(Error::TransportStage("room disconnected")),
                    RoomEvent::DataReceived { payload, topic, participant, .. } => {
                        if payload.len() > 65_536 { return Err(Error::Protocol); }
                        if let Some(expected) = &self.config.participant
                            && participant.as_ref().is_none_or(|p| p.identity().to_string() != *expected) { continue; }
                        let Ok(message) = serde_json::from_slice::<Value>(&payload) else { continue; };
                        let actions = self.hooks.event(true, json!({"message":message,"topic":topic}))?;
                        self.actions(actions, &mut state, output).await?;
                    }
                    RoomEvent::TrackSubscribed { track:RemoteTrack::Audio(track), participant, publication } => {
                        if self.config.participant.as_ref().is_some_and(|p| *p != participant.identity().to_string()) || self.config.track.as_ref().is_some_and(|name| *name != publication.name()) { continue; }
                        if selected.is_some() { return Err(Error::Configuration("multiple eligible remote audio tracks; select participant/track")); }
                        selected = Some(track.sid());
                        let tx = media_tx.clone();
                        tasks.spawn(async move {
                            let mut stream = NativeAudioStream::with_options(track.rtc_track(), 24_000, 1, NativeAudioStreamOptions { queue_size_frames: Some(200) });
                            while let Some(frame) = stream.next().await {
                                let bytes: Vec<u8> = frame.data.iter().flat_map(|sample| sample.to_le_bytes()).collect();
                                for pcm in bytes.chunks(FRAME_BYTES) {
                                    if tx.try_send(Media::Audio(pcm.to_vec())).is_err() {
                                        let _ = tx.send(Media::Failed).await;
                                        stream.close();
                                        return;
                                    }
                                }
                            }
                            let _ = tx.send(Media::Failed).await;
                        });
                    }
                    _ => {}
                },
                media = media_rx.recv() => match media.ok_or(Error::Transport)? {
                    Media::Failed => return Err(Error::TransportStage("media stream")),
                    Media::Consumed => emit(output, json!({"type":"audio_consumed","session_id":self.id,"stream_id":state.stream})).await?,
                    Media::Published => {
                        emit(output, json!({"type":"playback_complete","session_id":self.id,"stream_id":state.stream,"elapsed_ms":started.elapsed().as_millis()})).await?;
                        let actions = self.hooks.event(false, json!({"type":"playback_complete","stream_id":state.stream,"text":text}))?;
                        self.actions(actions, &mut state, output).await?;
                    }
                    Media::Audio(pcm) => {
                        if state.capturing && !state.published && pcm.iter().any(|b| *b != 0) { return Err(Error::Configuration("customer audio preceded first caller playback")); }
                        if !state.capturing { continue; }
                        if !state.started {
                            state.started = true;
                            emit(output, json!({"type":"response_start","session_id":self.id,"stream_id":state.stream,"elapsed_ms":started.elapsed().as_millis()})).await?;
                        }
                        let audio = Audio { header:AudioHeader { session_id:self.id, stream_id:state.stream, sequence:state.sequence, sample_offset:state.offset }, pcm };
                        state.sequence += 1;
                        state.offset += (audio.pcm.len()/2) as u64;
                        output.send(Message::Binary(audio.encode()?.into())).await.map_err(|_| Error::Transport)?;
                    }
                }
            }
        }
    }
}

fn native_frames(pcm: &[u8]) -> Vec<Vec<i16>> {
    pcm.chunks(480)
        .map(|chunk| {
            let mut samples: Vec<i16> = chunk
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .collect();
            samples.resize(240, 0);
            samples
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::native_frames;
    #[test]
    fn native_capture_uses_ten_ms_and_pads_only_final_frame() {
        let pcm = vec![1; 962];
        let frames = native_frames(&pcm);
        assert_eq!(frames.len(), 3);
        assert!(frames.iter().all(|frame| frame.len() == 240));
        assert!(frames[0].iter().all(|sample| *sample == 257));
        assert_eq!(frames[2][0], 257);
        assert!(frames[2][1..].iter().all(|sample| *sample == 0));
    }
}
