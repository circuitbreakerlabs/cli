# Customer-configured voice evaluations

The CLI stays connected during a voice evaluation, just as it does for text.
The API generates attacks, annotates and synthesizes speech, transcribes customer
responses, and scores and stores results. Customer authentication, endpoint URLs,
LiveKit connections, and JSON control messages stay on the customer's machine.

## Run an evaluation

Install a voice-enabled GNU Linux, macOS, or Windows release, or build with:

```sh
cargo build --locked --release --features voice
```

The musl Linux archive and default source/Nix builds support text only. The release
installer selects the GNU build on supported glibc systems and the text-only musl
build otherwise. Native build requirements are in `.github/actions/setup-voice`;
macOS also requires the Objective-C linker flag supplied in `.cargo/config.toml`.
The pinned LiveKit Rust SDK downloads its native WebRTC archive during compilation.

Copy `examples/voice/mock.toml` and its scripts, then configure the bootstrap URL.
With `CBL_API_KEY` in the environment:

```sh
cbl eval voice --threshold 0.5 --max-turns 4 --test-case-groups your-group \
  livekit --config ./voice-target.toml
cbl eval re-run voice --threshold 0.5 --max-turns 4 --evaluation-id 123 \
  livekit --config ./voice-target.toml
```

`--test-result-ids` is also available for re-runs. Global `--log-mode` and
`--output-file` work as for text. Results include target metadata and per-turn
voice metadata. API-generated audio artifacts remain on the API.

## Describe your endpoint

The public adapter is `livekit`. A configuration describes a customer integration,
not a server registry entry. For an endpoint with an existing token:

```toml
label = "staging-agent"
url = "wss://your-livekit-host"
token_env = "CUSTOMER_SESSION_TOKEN"
control_script = "control.rhai"
# Use these when more than one remote audio source is present:
# participant = "agent-identity"
# track = "agent-audio"
```

Alternatively, replace `url` and `token_env` with `bootstrap_script`. Scripts are
resolved relative to the configuration file. `[parameters]` supplies non-secret
JSON-compatible values; `[credentials]` maps script names to environment variable
names. Bootstrap runs once per conversation, including re-runs. For direct tokens,
the supplied token must permit every session the evaluation opens; use bootstrap
when each conversation needs fresh credentials or a separate room.

The `elevenlabs` example shows an existing hosted integration; `mock` shows the
local mock's handshake. These are editable examples, not compiled provider names.
A LiveKit URL alone does not define authentication or conversational turn control.
Version 1 requires explicit customer response-completion events. Silence-based turn
detection, arbitrary multistep bootstrap, standalone WebRTC/WebSocket transports,
greeting-aware evaluation, and deliberate interruption scenarios are not supported.

## Hook contract

Hooks are synchronous Rhai functions. Rust owns HTTP, media, queues, deadlines,
and connection cleanup. Audio and binary frames never enter the interpreter.
Scripts are trusted local code, bounded to 100,000 operations per invocation;
imports, print output, and debug output are disabled. Errors are sanitized.

- `build_connect_request(context)` returns `{method, url, headers?, body?}`. Methods
  are GET or POST. Rust performs one HTTP request, disallows redirects, limits the
  JSON response to 64 KiB, and applies a timeout. Use `url_encode(string)` for query
  values. Context contains `session_id`, `session_nonce`, `max_turns`, `parameters`,
  and explicitly configured `credentials`.
- `parse_connect_response(body, context)` returns `{url, token,
  external_session_id?}`. The optional identifier must be non-secret.
- `on_session_event(event, state)` receives `connected`, `utterance_start`, or
  `playback_complete`, or `closing`. Connected events include session identifiers, maximum turns,
  and parameters. Utterance events include `stream_id` and source `text`.
- `on_target_event(event, state)` receives `{message, topic}`, where `message` is
  parsed customer JSON. It never receives raw audio.

Both control hooks return `{state, actions}`. State starts as an empty map and is
isolated per session. At most 32 actions and 64 KiB of serialized output are allowed.
Actions are:

| Type | Additional fields | Meaning |
| --- | --- | --- |
| `send` | `message`, optional `topic` | Publish reliable customer JSON data |
| `session_ready` | — | Release the connection gate |
| `utterance_ready` | — | Release the current caller-audio gate |
| `capture_start` / `capture_stop` | — | Include/exclude customer audio from the response |
| `transcript` | `text` | Set optional comparison text, not the scored transcript |
| `response_end` | — | Mark an explicit customer response boundary |
| `close_ready` | — | Acknowledge optional customer hangup during cleanup |
| `error` | — | Fail the session without exposing customer error content |

On `closing`, return no actions to leave the room immediately, or send a customer
hangup message and return `close_ready` when its acknowledgement arrives. Cleanup
waits at most two seconds for the optional handshake, then closes the room.

Response completion retains a 200 ms media-drain interval because control and RTP
can arrive independently. This is a bounded transport accommodation, not silence
turn detection. Integrations should keep response capture enabled across this tail.
`playback_complete` means the CLI finished paced publication, not that a remote
speaker audibly played the audio. Timing metadata uses the CLI's session-local clock.

## API/CLI protocol v1

Routes are `/ws/multiturn_voice_evaluation` and
`/ws/multiturn_voice_rerun_evaluation`, under the normal API prefix. Initial JSON
contains `type` (`voice_request` or `voice_rerun_request`), `version: "1"`, existing
multi-turn `data`, and `target: {label, transport: "livekit", cli_version}`.
The API acknowledges `voice_ready` before opening sessions.

Commands: `session_open`, `utterance_start`, `utterance_end`, `playback_cancel`,
`session_close`. All carry `session_id`; utterance commands carry `stream_id`.
Session open carries `max_turns` and `timeout_ms`; utterance start also carries
`text` and `timeout_ms`.

CLI events: `session_ready` (capabilities and optional external identifier),
`utterance_ready`, `audio_consumed`, `playback_complete`, `response_start`,
`response_end` (sample count and optional transcript), `session_error`, and
`session_closed`. Media events carry a stream ID. Existing multi-turn progress
messages are reused; final messages are `voice_result` or sanitized `voice_error`.

Binary messages contain a four-byte big-endian JSON-header length, a UTF-8 header
`{session_id, stream_id, sequence, sample_offset}`, and 1–480 PCM samples. Audio is
24 kHz mono signed PCM16 little-endian; sequences and offsets start at zero per
stream. Both repositories consume the same fixture in `tests/fixtures/voice`.
At most 100 unacknowledged caller frames are sent. `audio_consumed` replenishes one
frame of credit. Pending CLI audio is bounded; overflow fails the conversation.
The API retains complete responses for transcription within the response limit.

Disconnects cancel active work and close customer rooms. No partial audio is
replayed automatically. Local publication cancellation closes the session; remote
agent interruption is not advertised. Existing voice timeout environment settings
remain API-owned. The registry-based REST voice endpoint is removed; text routes
are unchanged.

## Local end-to-end test

Build the CLI with voice, obtain a local `livekit-server`, and use the API virtualenv:

```sh
PYTHONPATH=../api/src:../api LIVEKIT_SERVER=/path/to/livekit-server \
  ../api/.venv/bin/python scripts/test_voice_e2e.py
```

This runs the actual CLI, API voice backend and mock customer through LiveKit, with
synthetic audio and deterministic model substitutes. It covers fresh/re-run flows,
concurrent conversations, multiple turns, normalized transcription, and result
metadata without cloud credentials. A live hosted-provider smoke test requires
separate designated test credentials.
