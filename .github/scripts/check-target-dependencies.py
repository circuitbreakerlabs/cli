"""Check that disabled provider dependencies are absent from cargo tree output."""
import os
from pathlib import Path
import re

selection = os.environ["FEATURES"]
features = {"openai", "ollama", "custom"} if selection == "default" else set(selection.split(","))
tree = Path("dependencies.txt").read_text()
for package, enabled in [
    ("async-openai", "openai" in features),
    ("ollama-rs", "ollama" in features),
    ("rhai", bool(features & {"custom", "livekit"})),
    ("toml", "livekit" in features),
]:
    present = re.search(rf"\b{re.escape(package)} v[0-9]", tree) is not None
    assert present == enabled, f"{package}: present={present}, enabled={enabled}"
