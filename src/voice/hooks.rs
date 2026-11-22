use super::{
    Error, Result,
    common_protocol::BootstrapMethod,
    config::{BootstrapContext, Config, read_secret, validate_url},
    multiparty_protocol::{DeliveryMode, ExchangeInput, LogicalParticipant},
};
use rhai::{AST, Dynamic, Engine, Scope};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

pub struct Script {
    engine: Engine,
    ast: AST,
}
impl Script {
    pub fn load(path: &Path) -> Result<Self> {
        let source = std::fs::read_to_string(path)
            .map_err(|_| Error::Configuration("cannot read voice script"))?;
        Self::compile(&source)
    }
    fn compile(source: &str) -> Result<Self> {
        let mut engine = Engine::new();
        engine.set_max_operations(100_000);
        engine.set_max_call_levels(32);
        engine.set_max_expr_depths(64, 32);
        engine.set_max_string_size(65_536);
        engine.set_max_array_size(256);
        engine.set_max_map_size(256);
        // Never allow script prints, debug output or imports to expose credentials.
        engine.register_fn("url_encode", |value: &str| -> String {
            url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
        });
        engine.on_print(|_| {});
        engine.on_debug(|_, _, _| {});
        engine.set_module_resolver(rhai::module_resolvers::DummyModuleResolver::new());
        let ast = engine.compile(source).map_err(|_| Error::Hook)?;
        Ok(Self { engine, ast })
    }
    pub fn call(&self, name: &str, arguments: Vec<Value>) -> Result<Value> {
        let arguments = arguments
            .into_iter()
            .map(|v| rhai::serde::to_dynamic(v).map_err(|_| Error::Hook))
            .collect::<Result<Vec<_>>>()?;
        let value: Dynamic = self
            .engine
            .call_fn(&mut Scope::new(), &self.ast, name, arguments)
            .map_err(|_| Error::Hook)?;
        rhai::serde::from_dynamic(&value).map_err(|_| Error::Hook)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub url: String,
    pub token: String,
    pub external_session_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HttpRequest {
    method: BootstrapMethod,
    url: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    body: Option<Value>,
}

pub async fn connect<C>(config: &Config, context: C) -> Result<Connection>
where
    C: Serialize,
{
    let context = serde_json::to_value(context).map_err(|_| Error::Hook)?;
    let connection = if let Some(path) = &config.bootstrap_script {
        let script = Script::load(path)?;
        let request: HttpRequest =
            serde_json::from_value(script.call("build_connect_request", vec![context.clone()])?)
                .map_err(|_| Error::Hook)?;
        validate_url(&request.url, &["https", "http"])?;
        let method = request.method.into();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|_| Error::Transport)?;
        let mut builder = client.request(method, &request.url);
        for (key, value) in request.headers {
            builder = builder.header(key, value);
        }
        if let Some(body) = request.body {
            builder = builder.json(&body);
        }
        let mut response = builder.send().await.map_err(|_| Error::Transport)?;
        if matches!(response.status().as_u16(), 401 | 403) {
            return Err(Error::Authentication);
        }
        if !response.status().is_success() {
            return Err(Error::Transport);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if bytes.len() + chunk.len() > 65_536 {
                return Err(Error::Protocol);
            }
            bytes.extend(chunk);
        }
        let body: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        serde_json::from_value(script.call("parse_connect_response", vec![body, context])?)
            .map_err(|_| Error::Hook)?
    } else {
        Connection {
            url: config
                .url
                .clone()
                .ok_or(Error::Configuration("missing URL"))?,
            token: read_secret(
                config
                    .token_env
                    .as_deref()
                    .ok_or(Error::Configuration("missing token_env"))?,
            )?,
            external_session_id: None,
        }
    };
    validate_url(&connection.url, &["ws", "wss"])?;
    if connection.token.trim().is_empty() {
        return Err(Error::Authentication);
    }
    Ok(connection)
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum V1Action {
    Send {
        message: Value,
        topic: Option<String>,
    },
    SessionReady,
    CloseReady,
    UtteranceReady,
    CaptureStart,
    CaptureStop,
    ResponseEnd,
    Transcript {
        text: String,
    },
    Error,
}

impl V1Action {
    pub fn diagnostic_name(&self) -> &'static str {
        match self {
            Self::Send { .. } => "send",
            Self::SessionReady => "session_ready",
            Self::CloseReady => "close_ready",
            Self::UtteranceReady => "utterance_ready",
            Self::CaptureStart => "capture_start",
            Self::CaptureStop => "capture_stop",
            Self::ResponseEnd => "response_end",
            Self::Transcript { .. } => "transcript",
            Self::Error => "error",
        }
    }
}

pub use V1Action as Action;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum V2Action {
    Send {
        message: Value,
        topic: Option<String>,
    },
    SessionReady,
    CloseReady,
    ExchangeReady,
    CaptureStart,
    CaptureStop,
    ResponseEnd,
    Transcript {
        text: String,
    },
    Error,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HookOutput<A> {
    state: Value,
    actions: Vec<A>,
}

pub struct Hooks {
    script: Script,
    state: Value,
}
impl Hooks {
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            script: Script::load(&config.control_script)?,
            state: json!({}),
        })
    }
    pub fn event<E>(&mut self, target: bool, event: E) -> Result<Vec<V1Action>>
    where
        E: Serialize,
    {
        self.run_event(target, event)
    }

    pub fn multiparty_event<E>(&mut self, target: bool, event: E) -> Result<Vec<V2Action>>
    where
        E: Serialize,
    {
        self.run_event(target, event)
    }

    fn run_event<E, A>(&mut self, target: bool, event: E) -> Result<Vec<A>>
    where
        E: Serialize,
        A: DeserializeOwned,
    {
        let event = serde_json::to_value(event).map_err(|_| Error::Hook)?;
        let value = self.script.call(
            if target {
                "on_target_event"
            } else {
                "on_session_event"
            },
            vec![event, self.state.clone()],
        )?;
        if serde_json::to_vec(&value).map_err(|_| Error::Hook)?.len() > 65_536 {
            return Err(Error::Hook);
        }
        let output: HookOutput<A> = serde_json::from_value(value).map_err(|_| Error::Hook)?;
        if output.actions.len() > 32 {
            return Err(Error::Hook);
        }
        self.state = output.state;
        Ok(output.actions)
    }
}

#[derive(Clone, Serialize)]
pub struct V1BootstrapContext {
    #[serde(flatten)]
    pub base: BootstrapContext,
    pub session_nonce: String,
}

#[derive(Clone, Serialize)]
pub struct V2BootstrapContext {
    #[serde(flatten)]
    pub base: BootstrapContext,
    pub protocol_version: &'static str,
    pub conversation_id: i32,
    pub delivery_mode: DeliveryMode,
    pub participants: Vec<LogicalParticipant>,
    pub max_exchanges: u32,
    pub session_nonce: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum V1SessionEvent {
    Connected {
        session_id: i32,
        max_turns: u32,
        parameters: BTreeMap<String, Value>,
        session_nonce: String,
        playback_ticks: bool,
    },
    Closing,
    PlaybackTick {
        stream_id: u64,
    },
    UtteranceStart {
        stream_id: u64,
        text: String,
    },
    PlaybackComplete {
        stream_id: u64,
        text: String,
    },
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum V2SessionEvent {
    Connected {
        protocol_version: &'static str,
        session_id: i32,
        conversation_id: i32,
        delivery_mode: DeliveryMode,
        participants: Vec<LogicalParticipant>,
        max_exchanges: u32,
        parameters: BTreeMap<String, Value>,
        session_nonce: String,
        playback_ticks: bool,
    },
    Closing,
    PlaybackTick {
        exchange_id: u64,
    },
    ExchangeStart {
        exchange_id: u64,
        inputs: Vec<ExchangeInput>,
    },
    ExchangeEnd {
        exchange_id: u64,
        inputs: Vec<ExchangeInput>,
    },
    PlaybackComplete {
        exchange_id: u64,
        inputs: Vec<ExchangeInput>,
    },
}

#[derive(Clone, Serialize)]
pub struct TargetEvent {
    pub message: Value,
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_started: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exchange_id: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderEvent {
    Audio,
    AgentResponse {
        agent_response_event: Option<ProviderEventId>,
    },
    AgentResponseComplete {
        agent_response_complete_event: Option<ProviderEventId>,
    },
    Interruption {
        interruption_event: Option<ProviderEventId>,
    },
    Ping {
        ping_event: Option<ProviderEventId>,
    },
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ProviderEventId {
    pub event_id: Option<u64>,
}

impl ProviderEvent {
    pub fn event_id(&self) -> Option<u64> {
        match self {
            Self::AgentResponse {
                agent_response_event: event,
            }
            | Self::AgentResponseComplete {
                agent_response_complete_event: event,
            }
            | Self::Interruption {
                interruption_event: event,
            }
            | Self::Ping { ping_event: event } => event.as_ref().and_then(|event| event.event_id),
            Self::Audio | Self::Other => None,
        }
    }
}

pub fn provider_event(value: &Value) -> ProviderEvent {
    serde_json::from_value(value.clone()).unwrap_or(ProviderEvent::Other)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initialized_elevenlabs_hooks() -> Hooks {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/voice");
        let config = Config::load(&root.join("elevenlabs.toml")).unwrap();
        let mut hooks = Hooks::new(&config).unwrap();
        hooks.event(false, json!({"type":"connected"})).unwrap();
        hooks
            .event(
                true,
                json!({"message":{"type":"conversation_initiation_metadata"}}),
            )
            .unwrap();
        hooks
    }

    #[test]
    fn elevenlabs_protects_pauses_only_in_the_current_caller_turn() {
        let mut hooks = initialized_elevenlabs_hooks();
        hooks
            .event(false, json!({"type":"connected","playback_ticks":true}))
            .unwrap();
        let tick = |stream_id| json!({"type":"playback_tick","stream_id":stream_id});
        assert!(hooks.event(false, tick(1)).unwrap().is_empty());
        for stream_id in 1..=2 {
            let actions = hooks
                .event(
                    false,
                    json!({"type":"utterance_start","stream_id":stream_id}),
                )
                .unwrap();
            assert!(matches!(actions.as_slice(),
                [Action::Send { message, topic: None }, Action::CaptureStart, Action::UtteranceReady]
                if message == &json!({"type":"user_activity"})));
            assert!(hooks.event(false, tick(stream_id + 1)).unwrap().is_empty());
            // Refresh across a pause longer than the provider's two-second hold.
            for _ in 0..6 {
                let actions = hooks.event(false, tick(stream_id)).unwrap();
                assert!(
                    matches!(actions.as_slice(), [Action::Send { message, topic: None }]
                    if message == &json!({"type":"user_activity"}))
                );
            }
            hooks
                .event(
                    false,
                    json!({"type":"playback_complete","stream_id":stream_id + 1}),
                )
                .unwrap();
            assert!(!hooks.event(false, tick(stream_id)).unwrap().is_empty());
            hooks
                .event(
                    false,
                    json!({"type":"playback_complete","stream_id":stream_id}),
                )
                .unwrap();
            assert!(hooks.event(false, tick(stream_id)).unwrap().is_empty());
            let actions = hooks
                .event(
                    true,
                    json!({"caller_started":true,"message":{"type":"agent_response_complete"}}),
                )
                .unwrap();
            assert!(matches!(actions.as_slice(), [Action::ResponseEnd]));
        }
        hooks
            .event(false, json!({"type":"utterance_start","stream_id":3}))
            .unwrap();
        hooks.event(false, json!({"type":"closing"})).unwrap();
        assert!(hooks.event(false, tick(3)).unwrap().is_empty());
    }

    #[test]
    fn elevenlabs_activity_can_be_disabled_for_baseline_runs() {
        let mut hooks = initialized_elevenlabs_hooks();
        hooks
            .event(false, json!({"type":"connected","playback_ticks":false}))
            .unwrap();
        let actions = hooks
            .event(false, json!({"type":"utterance_start","stream_id":1}))
            .unwrap();
        assert!(matches!(
            actions.as_slice(),
            [Action::CaptureStart, Action::UtteranceReady]
        ));
        assert!(
            hooks
                .event(false, json!({"type":"playback_tick","stream_id":1}))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn calls_hooks_with_separate_arguments() {
        let script = Script::compile("fn add(a, b) { a + b }").unwrap();
        assert_eq!(
            script.call("add", vec![json!(2), json!(3)]).unwrap(),
            json!(5)
        );
    }
    #[test]
    fn runaway_and_throwing_hooks_have_sanitized_errors() {
        for source in ["fn run() { loop {} }", "fn run() { throw \"secret\"; }"] {
            let script = Script::compile(source).unwrap();
            let error = script.call("run", vec![]).unwrap_err();
            assert!(!error.to_string().contains("secret"));
        }
    }
    #[test]
    fn examples_bootstrap_and_control_without_provider_specific_rust() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/voice");
        for name in ["elevenlabs", "mock"] {
            let config = Config::load(&root.join(format!("{name}.toml"))).unwrap();
            let script = Script::load(config.bootstrap_script.as_ref().unwrap()).unwrap();
            let context = json!({"session_id":7,"session_nonce":"nonce","max_turns":2,
                "parameters":config.parameters,"credentials":{"api_key":"test-secret","agent_id":"test-agent"}});
            let request = script
                .call("build_connect_request", vec![context.clone()])
                .unwrap();
            assert!(request["url"].as_str().unwrap().starts_with("http"));
            let body = json!({"url":"ws://localhost:7880","token":"test-token","caller_token":"test-token"});
            let connection: Connection = serde_json::from_value(
                script
                    .call("parse_connect_response", vec![body, context])
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(connection.token, "test-token");
            let mut hooks = Hooks::new(&config).unwrap();
            let actions = hooks.event(false, json!({"type":"connected","session_id":7,"session_nonce":"nonce","max_turns":2})).unwrap();
            assert!(!actions.is_empty());
            hooks
                .event(
                    false,
                    json!({"type":"utterance_start","stream_id":1,"text":"hello"}),
                )
                .unwrap();
            if name == "mock" {
                let actions = hooks
                    .event(
                        true,
                        json!({"message":{"type":"utterance.ready","turn_index":1}}),
                    )
                    .unwrap();
                assert!(matches!(actions.as_slice(), [Action::UtteranceReady]));
                let actions = hooks
                    .event(
                        true,
                        json!({"message":{"type":"response.start","turn_index":2}}),
                    )
                    .unwrap();
                assert!(matches!(
                    actions.as_slice(),
                    [Action::CaptureStart, Action::Send { .. }]
                ));
            }
        }
    }

    #[test]
    fn multiparty_example_uses_v2_exchange_actions() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/voice");
        let config = Config::load(&root.join("elevenlabs-multiparty.toml")).unwrap();
        let mut hooks = Hooks::new(&config).unwrap();
        hooks
            .multiparty_event(
                false,
                json!({
                    "type":"connected",
                    "playback_ticks":true,
                    "participants":[
                        {"id":"alice","display_name":"Alice","voice":"Aoede"},
                        {"id":"bob","display_name":"Bob","voice":"Charon"}
                    ]
                }),
            )
            .unwrap();
        assert!(matches!(
            hooks
                .multiparty_event(
                    true,
                    json!({"message":{"type":"conversation_initiation_metadata"}}),
                )
                .unwrap()
                .as_slice(),
            [V2Action::SessionReady]
        ));
        let actions = hooks
            .multiparty_event(
                false,
                json!({
                    "type":"exchange_start",
                    "exchange_id":1,
                    "inputs":[{"speaker_id":"alice","text":"hello"}]
                }),
            )
            .unwrap();
        assert!(matches!(
            actions.as_slice(),
            [
                V2Action::Send { .. },
                V2Action::CaptureStart,
                V2Action::ExchangeReady
            ]
        ));
        hooks
            .multiparty_event(false, json!({"type":"playback_complete","exchange_id":1}))
            .unwrap();
        assert!(hooks
            .multiparty_event(
                true,
                json!({"caller_started":true,"message":{"type":"agent_response","agent_response_event":{"agent_response":"reply"}}}),
            )
            .unwrap()
            .iter()
            .any(|action| matches!(action, V2Action::Transcript { .. })));
        assert!(matches!(
            hooks
                .multiparty_event(
                    true,
                    json!({"caller_started":true,"message":{"type":"agent_response_complete"}}),
                )
                .unwrap()
                .as_slice(),
            [V2Action::ResponseEnd]
        ));
    }

    #[test]
    fn elevenlabs_waits_for_initialization_and_completes_each_turn_once() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/voice");
        let config = Config::load(&root.join("elevenlabs.toml")).unwrap();
        let mut hooks = Hooks::new(&config).unwrap();
        let actions = hooks.event(false, json!({"type":"connected"})).unwrap();
        assert!(matches!(actions.as_slice(), [Action::Send { message, .. }]
            if message["type"] == "conversation_initiation_client_data"));
        for expected_ready in [true, false] {
            let actions = hooks
                .event(
                    true,
                    json!({"message":{"type":"conversation_initiation_metadata"}}),
                )
                .unwrap();
            assert_eq!(
                matches!(actions.as_slice(), [Action::SessionReady]),
                expected_ready
            );
        }
        for _ in 0..2 {
            let actions = hooks
                .event(false, json!({"type":"utterance_start"}))
                .unwrap();
            assert!(matches!(
                actions.as_slice(),
                [Action::CaptureStart, Action::UtteranceReady]
            ));
            assert!(hooks
                .event(
                    true,
                    json!({"caller_started":false,"message":{"type":"agent_response","agent_response_event":{"agent_response":"greeting"}}}),
                )
                .unwrap()
                .is_empty());
            assert!(hooks
                .event(
                    true,
                    json!({"caller_started":false,"message":{"type":"agent_response_complete"}}),
                )
                .unwrap()
                .is_empty());
            let actions = hooks
                .event(true, json!({"message":{"type":"agent_response_complete"}}))
                .unwrap();
            assert!(actions.is_empty());
            let actions = hooks
                .event(false, json!({"type":"playback_complete"}))
                .unwrap();
            assert!(actions.is_empty());
            assert!(hooks
                .event(
                    true,
                    json!({"caller_started":true,"message":{"type":"agent_response","agent_response_event":{"agent_response":"reply"}}}),
                )
                .unwrap()
                .iter()
                .any(|action| matches!(action, Action::Transcript { .. })));
            let actions = hooks
                .event(
                    true,
                    json!({"caller_started":true,"message":{"type":"agent_response_complete"}}),
                )
                .unwrap();
            assert!(matches!(actions.as_slice(), [Action::ResponseEnd]));
            assert!(
                hooks
                    .event(true, json!({"message":{"type":"agent_response_complete"}}))
                    .unwrap()
                    .is_empty()
            );
        }
        let actions = hooks
            .event(
                true,
                json!({"message":{"type":"ping","ping_event":{"event_id":42}}}),
            )
            .unwrap();
        assert!(matches!(actions.as_slice(), [Action::Send { message, .. }]
            if message == &json!({"type":"pong","event_id":42})));
    }

    #[test]
    fn elevenlabs_completes_uninterrupted_turns_without_transcripts() {
        let mut hooks = initialized_elevenlabs_hooks();
        for stream_id in 1..=2 {
            hooks
                .event(
                    false,
                    json!({"type":"utterance_start","stream_id":stream_id}),
                )
                .unwrap();
            hooks
                .event(
                    false,
                    json!({"type":"playback_complete","stream_id":stream_id}),
                )
                .unwrap();
            let complete =
                json!({"caller_started":true,"message":{"type":"agent_response_complete"}});
            let actions = hooks.event(true, complete.clone()).unwrap();
            assert!(matches!(actions.as_slice(), [Action::ResponseEnd]));
            assert!(hooks.event(true, complete).unwrap().is_empty());
        }
    }

    #[test]
    fn elevenlabs_ignores_agent_response_without_text_payload() {
        let mut hooks = initialized_elevenlabs_hooks();
        hooks
            .event(false, json!({"type":"utterance_start","stream_id":1}))
            .unwrap();
        assert!(
            hooks
                .event(
                    true,
                    json!({"caller_started":true,"message":{"type":"agent_response"}}),
                )
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn elevenlabs_keeps_interrupted_turn_open_until_recovery() {
        let mut hooks = initialized_elevenlabs_hooks();
        let actions = hooks
            .event(false, json!({"type":"utterance_start"}))
            .unwrap();
        assert!(matches!(
            actions.as_slice(),
            [Action::CaptureStart, Action::UtteranceReady]
        ));
        assert!(hooks
            .event(
                true,
                json!({"caller_started":true,"message":{"type":"agent_response","agent_response_event":{"agent_response":"partial"}}}),
            )
            .unwrap()
            .iter()
            .any(|action| matches!(action, Action::Transcript { .. })));
        assert!(
            hooks
                .event(
                    true,
                    json!({"caller_started":true,"message":{"type":"interruption"}}),
                )
                .unwrap()
                .is_empty()
        );
        assert!(
            hooks
                .event(
                    true,
                    json!({"caller_started":true,"message":{"type":"agent_response_complete"}}),
                )
                .unwrap()
                .is_empty()
        );
        hooks
            .event(false, json!({"type":"playback_complete"}))
            .unwrap();
        assert!(
            hooks
                .event(
                    true,
                    json!({"caller_started":true,"message":{"type":"agent_response_complete"}}),
                )
                .unwrap()
                .is_empty()
        );
        hooks
            .event(
                true,
                json!({"caller_started":true,"message":{"type":"agent_response","agent_response_event":{"agent_response":"recovered"}}}),
            )
            .unwrap();
        let actions = hooks
            .event(
                true,
                json!({"caller_started":true,"message":{"type":"agent_response_complete"}}),
            )
            .unwrap();
        assert!(matches!(actions.as_slice(), [Action::ResponseEnd]));
    }

    #[test]
    fn elevenlabs_ignores_completion_during_caller_playback() {
        let mut hooks = initialized_elevenlabs_hooks();
        hooks
            .event(false, json!({"type":"utterance_start"}))
            .unwrap();
        assert!(hooks
            .event(
                true,
                json!({"caller_started":true,"message":{"type":"agent_response","agent_response_event":{"agent_response":"complete soon"}}}),
            )
            .unwrap()
            .iter()
            .any(|action| matches!(action, Action::Transcript { .. })));
        assert!(
            hooks
                .event(
                    true,
                    json!({"caller_started":true,"message":{"type":"agent_response_complete"}}),
                )
                .unwrap()
                .is_empty()
        );
        hooks
            .event(false, json!({"type":"playback_complete"}))
            .unwrap();
        assert!(
            hooks
                .event(
                    true,
                    json!({"caller_started":true,"message":{"type":"agent_response_complete"}}),
                )
                .unwrap()
                .is_empty()
        );
        hooks
            .event(
                true,
                json!({"caller_started":true,"message":{"type":"agent_response","agent_response_event":{"agent_response":"recovered"}}}),
            )
            .unwrap();
        let actions = hooks
            .event(
                true,
                json!({"caller_started":true,"message":{"type":"agent_response_complete"}}),
            )
            .unwrap();
        assert!(matches!(actions.as_slice(), [Action::ResponseEnd]));
    }

    #[test]
    fn elevenlabs_resets_recovery_state_for_each_turn() {
        let mut hooks = initialized_elevenlabs_hooks();
        hooks
            .event(false, json!({"type":"utterance_start","stream_id":1}))
            .unwrap();
        hooks
            .event(
                true,
                json!({"caller_started":true,"message":{"type":"agent_response","agent_response_event":{"agent_response":"partial"}}}),
            )
            .unwrap();
        hooks
            .event(
                true,
                json!({"caller_started":true,"message":{"type":"interruption"}}),
            )
            .unwrap();
        hooks
            .event(false, json!({"type":"playback_complete","stream_id":1}))
            .unwrap();

        hooks
            .event(false, json!({"type":"utterance_start","stream_id":2}))
            .unwrap();
        hooks
            .event(false, json!({"type":"playback_complete","stream_id":2}))
            .unwrap();
        let actions = hooks
            .event(
                true,
                json!({"caller_started":true,"message":{"type":"agent_response_complete"}}),
            )
            .unwrap();
        assert!(matches!(actions.as_slice(), [Action::ResponseEnd]));
    }
}
