"""Select Cargo features for a single-platform dist build job."""
import json
import os
from pathlib import Path
import re


def select_features(targets):
    if not targets or not all(isinstance(target, str) for target in targets):
        raise ValueError("release targets must be a nonempty list of target triples")
    musl = {"musl" in target for target in targets}
    if len(musl) != 1:
        raise ValueError("musl and native LiveKit releases must use separate build jobs")
    features = ["openai", "ollama", "custom"]
    if True not in musl:
        features.append("livekit")
    return features


def main():
    targets = json.loads(os.environ["RELEASE_TARGETS"])
    features = select_features(targets)
    config = Path("dist-workspace.toml")
    contents, count = re.subn(
        r"^features = \[.*\]$", "features = " + json.dumps(features),
        config.read_text(), flags=re.MULTILINE,
    )
    if count != 1:
        raise ValueError("expected exactly one dist feature list")
    config.write_text(contents)
    print(f"Release features for {targets}: {features}")


if __name__ == "__main__":
    main()
