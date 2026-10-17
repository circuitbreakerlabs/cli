"""Exercise release feature selection without touching the checkout."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True

SCRIPT = Path(__file__).with_name("select-release-features.py").resolve()
spec = importlib.util.spec_from_file_location("release_features", SCRIPT)
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseFeatures(unittest.TestCase):
    def test_platform_features(self):
        for target in ["x86_64-unknown-linux-musl", "x86_64-unknown-linux-gnu",
                       "aarch64-apple-darwin", "x86_64-pc-windows-msvc"]:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as directory:
                config = Path(directory) / "dist-workspace.toml"
                config.write_text('[dist]\nfeatures = ["livekit"]\n')
                subprocess.run(["python3", str(SCRIPT)], cwd=directory, check=True,
                               env={**os.environ, "RELEASE_TARGETS": json.dumps([target])})
                expected = ["openai", "ollama", "custom"]
                if "musl" not in target:
                    expected.append("livekit")
                self.assertEqual(config.read_text(), '[dist]\nfeatures = ' + json.dumps(expected) + '\n')

    def test_rejects_mixed_or_empty_jobs(self):
        for targets in [[], ["x86_64-unknown-linux-musl", "x86_64-unknown-linux-gnu"]]:
            with self.assertRaises(ValueError):
                release.select_features(targets)


if __name__ == "__main__":
    unittest.main()
