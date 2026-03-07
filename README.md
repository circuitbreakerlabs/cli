# Circuit Breaker Labs Command-Line Interface

<p align="center" width="100%">
  <img src="./assets/demo.gif" alt="TUI Demo" width="60%">
</p>
<p align="center">
  <i>Recorded with <a href="https://github.com/charmbracelet/vhs">vhs</i>💙</a>
</p>

## Installation

Pre-built executables and installation methods for Linux, Mac, and Windows are automatically generated and available with each [release](https://github.com/circuitbreakerlabs/cli/releases).

## Usage

### Flags and Options

You can see the available options and flags for `cbl` with `cbl help` or for a subcommand with `cbl <subcommand> help`.

### Syntax

The syntax for `cbl` is:

```sh
cbl --top-level-arg1 <evaluation_type> --evaluation-arg1 <provider> --provider-arg1
```

where `<evaluation_type>` and `<provider>` are subcommands.

The available evaluation types are `single-turn` and `multi-turn`. The available providers are `ollama`, `openai`, and `custom`.

#### Example

The following would run a single-turn evaluation against a custom OpenAI finetune, and save the results to `result.json`.

```sh
cbl \
    --output-file result.json \
    single-turn \  # evaluation type
    --threshold 0.3 \
    --variations 3 \
    --maximum-iteration-layers 2 \
    openai \       # provider
    --temperature 1.2 \
    --model $MY_FINETUNE_ID
```

### Integrating With a Custom Model Endpoint

For APIs that aren't already supported or OpenAI compatible, `cbl` supports scripting. The `custom` provider expects a [Rhai](https://rhai.rs/) script that defines the translation between `cbl`'s and the custom endpoint's request/response schema. Examples scripts are available in [`examples/providers/`](https://github.com/circuitbreakerlabs/cli/tree/main/examples/providers).
