# Circuit Breaker Labs Command-Line Interface

<p align="center" width="100%">
  <img src="./assets/demo.gif" alt="TUI Demo" width="60%">
</p>
<p align="center">
  <i>Recorded with <a href="https://github.com/charmbracelet/vhs">vhs</i>💙</a>
</p>

## Quickstart

1. Copy the installation command for your preferred shell from the [releases page](https://github.com/circuitbreakerlabs/cli/releases).
2. Verify `cbl` installed correctly with `cbl help`
3. Set the Circuit Breaker Labs API key environment variable:
   * **macOS/Linux:** `export CBL_API_KEY="<your_api_key_here>"`
   * **Windows (PowerShell):** `$env:CBL_API_KEY="<your_api_key_here>"`
4. Set the OpenAI API key environment variable (required for this example):
   * **macOS/Linux:** `export OPENAI_API_KEY="<your_api_key_here>"`
   * **Windows (PowerShell):** `$env:OPENAI_API_KEY="<your_api_key_here>"`


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

## Usage

### Flags and Options

You can see the available options and flags for `cbl` with `cbl help`, for evaluation commands with `cbl eval help`, or for a specific evaluation type with `cbl eval <evaluation_type> help`.

### Syntax

The syntax for `cbl` is:

```sh
cbl --top-level-arg1 eval <evaluation_type> --evaluation-arg1 <provider> --provider-arg1
```

where `eval`, `<evaluation_type>`, and `<provider>` are subcommands.

The available evaluation types are `single-turn` and `multi-turn`. The available providers are `ollama`, `openai`, and `custom`.

#### Example

The following would run a single-turn evaluation against a custom OpenAI finetune, and save the results to `result.json`. If you haven't already, set the `CBL_API_KEY` and `OPENAI_API_KEY` environment variables.

```sh
cbl \
    --output-file result.json \
    eval \
    single-turn \  # evaluation type
    --threshold 0.3 \
    --variations 3 \
    --maximum-iteration-layers 2 \
    --test-case-groups suicidal_ideation \
    openai \       # provider
    --temperature 1.2 \
    --model $MY_FINETUNE_ID
```

### Integrating With a Custom Model Endpoint

For APIs that aren't already supported or OpenAI compatible, `cbl` supports scripting. The `custom` provider expects a [Rhai](https://rhai.rs/) script that defines the translation between `cbl`'s and the custom endpoint's request/response schema. Examples scripts are available in [`examples/providers/`](https://github.com/circuitbreakerlabs/cli/tree/main/examples/providers).

### Configuration Reference

#### Safety Threshold

The `--threshold` flag accepts a value from `0` to `1`. Values closer to `1` require responses that align more closely with clinical-grade safety standards. Lower values allow more permissive responses.

#### Maximum Turns

`--max-turns` must be an even number because each turn pairs one user message with one model response. The upper limit depends on your system configuration. [Contact us](mailto:team@circuitbreakerlabs.ai) if you need a higher limit for your environment.

#### Test Packs

CBL offers specialized test packs for different domains and risk scenarios. Pass one or more pack names with `--test-case-groups`, separated by commas.

---

Questions? Feedback? Reach us at [team@circuitbreakerlabs.ai](mailto:team@circuitbreakerlabs.ai).
