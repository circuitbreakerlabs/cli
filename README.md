# Circuit Breaker Labs Command-Line Interface

<p align="center" width="100%">
  <img src="./assets/demo.gif" alt="Single-turn and multi-turn evaluation demo" width="60%">
</p>
<p align="center">
  <i>Recorded with <a href="https://github.com/charmbracelet/vhs">vhs</a>💙</i>
</p>

## Quickstart

1. Copy the installation command for your preferred shell from the [releases page](https://github.com/circuitbreakerlabs/cli/releases).
2. Verify `cbl` installed correctly with `cbl help`
3. Set the Circuit Breaker Labs API key environment variable:
   - **macOS/Linux:** `export CBL_API_KEY="<your_api_key_here>"`
   - **Windows (PowerShell):** `$env:CBL_API_KEY="<your_api_key_here>"`
4. Set the OpenAI API key environment variable (required for this example):
   - **macOS/Linux:** `export OPENAI_API_KEY="<your_api_key_here>"`
   - **Windows (PowerShell):** `$env:OPENAI_API_KEY="<your_api_key_here>"`

Try a single-turn evaluation:

```sh
cbl eval single-turn \
    --threshold 0.75 \
    --variations 2 \
    --maximum-iteration-layers 2 \
    --test-case-groups suicidal_ideation \
    openai --model gpt-4.1-nano
```

Try a multi-turn evaluation:

```sh
cbl eval multi-turn \
    --threshold 0.95 \
    --max-turns 8 \
    --test-case-groups suicidal_ideation \
    openai --model gpt-4.1-nano
```

## Installation

Pre-built executables and installation methods for Linux, Mac, and Windows are automatically generated and available with each [release](https://github.com/circuitbreakerlabs/cli/releases).

Click [here](mailto:team@circuitbreakerlabs.ai?subject=Getting%20Set%20Up&body=I%27m%20interested%20in%20using%20the%20Circuit%20Breaker%20Labs%20CLI%20tool%20for%20autonomous%20red-teaming%20and%20would%20like%20to%20request%20an%20API%20access%20key.) to get an access key.

## Usage reference

See the [CLI reference](https://api.circuitbreakerlabs.ai/v1/docs)
in the API documentation for command syntax, evaluation options, custom model
integration, and LiveKit setup and reruns.

## Build features

Default builds enable the `openai`, `ollama`, and `custom` target features. Each
feature controls its target's implementation, dependencies, and CLI commands.
Disabled targets are absent from help and rejected by the command parser.

```sh
cargo build --release                            # All three text targets
cargo build --release --features livekit          # Text targets plus LiveKit
cargo build --release --no-default-features --features openai
cargo build --release --no-default-features --features livekit
cargo build --release --no-default-features       # API-only CLI, without eval
```

The `livekit` feature is incompatible with musl: builds combining them fail,
including `--all-features`. Build musl binaries with the text features only.
Release builds enable all four targets on GNU Linux, macOS, and Windows, and
only the three text targets on musl.

---

Questions? Feedback? Reach us at [team@circuitbreakerlabs.ai](mailto:team@circuitbreakerlabs.ai).
