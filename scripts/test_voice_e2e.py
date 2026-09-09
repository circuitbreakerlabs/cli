#!/usr/bin/env python3
"""Exercise the real CLI, API voice backend, and local LiveKit mock.

Run with the API repository's Python environment and source on PYTHONPATH:
  CBL_BINARY=target/debug/cbl LIVEKIT_SERVER=/path/to/livekit-server \
    ../api/.venv/bin/python scripts/test_voice_e2e.py
No cloud credentials or paid model calls are used.
"""
from __future__ import annotations

import asyncio
import json
import math
import os
from pathlib import Path
import signal
import socket
import struct
import tempfile
from types import SimpleNamespace
from unittest.mock import MagicMock

import uvicorn
from fastapi import FastAPI
from api.app.dependencies import get_websocket_evaluation_api_key
from api.app.endpoints.websockets import voice
from api.app.voice.mock_customer import create_app
from api.app.voice.protocols import TranscriptionResult
from database.type_definitions import Message, Role
from red_team.providers import GenerationConfig, GenerationResponse


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


class Synthesizer:
    async def stream(self, text, *, voice=None):
        for chunk in range(25):
            yield b"".join(struct.pack("<h", int(3000 * math.sin(2 * math.pi * 440 * (chunk * 480 + i) / 24000))) for i in range(480))


class Annotator:
    async def annotate(self, text):
        return text


class Transcriber:
    async def transcribe(self, audio, **kwargs):
        assert kwargs["sample_rate"] == 24000
        assert len(audio) >= 12000, f"truncated response: {len(audio)} bytes"
        return TranscriptionResult(text="deterministic customer response")


async def serve(app, port):
    server = uvicorn.Server(uvicorn.Config(app, host="127.0.0.1", port=port, log_level="error"))
    task = asyncio.create_task(server.serve())
    for _ in range(200):
        if server.started:
            return server, task
        if task.done():
            await task
        await asyncio.sleep(0.05)
    raise RuntimeError("test server did not start")


async def main():
    root = Path(__file__).resolve().parents[1]
    binary = Path(os.environ.get("CBL_BINARY", root / "target/debug/cbl")).resolve()
    livekit_binary = os.environ["LIVEKIT_SERVER"]
    rtc_port, api_port, mock_port = free_port(), free_port(), free_port()
    with tempfile.TemporaryDirectory(prefix="cbl-voice-e2e-") as directory:
        work = Path(directory)
        log = (work / "livekit.log").open("wb")
        server_config = work / "livekit.yaml"
        test_secret = "cbl-e2e-not-a-production-secret-123456"
        server_config.write_text(json.dumps({"port":rtc_port, "rtc":{"tcp_port":free_port(), "udp_port":free_port(), "node_ip":"127.0.0.1", "use_external_ip":False}, "keys":{"cbl-e2e":test_secret}}))
        livekit = await asyncio.create_subprocess_exec(livekit_binary, "--dev", "--bind", "127.0.0.1", "--config", str(server_config), stdout=log, stderr=log)
        servers = []
        try:
            for _ in range(100):
                try:
                    reader, writer = await asyncio.open_connection("127.0.0.1", rtc_port)
                    writer.close()
                    await writer.wait_closed()
                    break
                except OSError:
                    await asyncio.sleep(0.1)
            mock = create_app(generator=lambda _: "deterministic customer response", synthesizer=Synthesizer(),
                livekit_url=f"ws://127.0.0.1:{rtc_port}", livekit_api_key="cbl-e2e", livekit_api_secret=test_secret)
            servers.append(await serve(mock, mock_port))
            voice.ModalProsodyAnnotator = Annotator
            voice.ModalSpeechTranscriber = Transcriber
            voice.build_synthesizer = lambda _: Synthesizer()
            calls = []
            cancel_mode = False
            active = asyncio.Event()
            cancelled = asyncio.Event()
            async def evaluate(**kwargs):
                assert kwargs["apply_algorithmic_augmentation"] is False
                backend = kwargs["provider"]
                async def conversation(identifier):
                    for turn in range(2):
                        result = await backend.generate_text(messages=[Message(role=Role.USER, content=f"caller {identifier} turn {turn}")], config=GenerationConfig(), conversation_id=identifier)
                        assert isinstance(result, GenerationResponse), f"{type(result).__name__}: {result}"
                        assert result.text == "deterministic customer response"
                        if cancel_mode:
                            active.set()
                            try:
                                await asyncio.Event().wait()
                            finally:
                                cancelled.set()
                    await backend.close_conversation(identifier)
                try:
                    await asyncio.gather(conversation(1), conversation(2))
                except asyncio.CancelledError:
                    cancelled.set()
                    raise
                except Exception:
                    import traceback
                    traceback.print_exc()
                    raise
                calls.append(kwargs["test_case_sources"])
                return SimpleNamespace(results=[SimpleNamespace(conversation_id=i) for i in (1, 2)],
                    model_dump=lambda **_: {"evaluation_id":1,"total_passed":2,"total_failed":0,"results":[]})
            voice.run_multi_turn_evaluation = evaluate
            async def sources(*args, **kwargs):
                return ["historic-source"]
            voice.resolve_multi_turn_sources = sources
            app = FastAPI()
            app.include_router(voice.router, prefix="/v1")
            app.state.attacker_provider = MagicMock()
            app.state.openrouter_api_key = "unused"
            app.state.db_session_factory = MagicMock()
            app.dependency_overrides[get_websocket_evaluation_api_key] = lambda: SimpleNamespace(api_key="test", user=SimpleNamespace(id=1))
            servers.append(await serve(app, api_port))
            config = work / "target.toml"
            config.write_text(f'label = "e2e"\nbootstrap_script = "{root}/examples/voice/mock-bootstrap.rhai"\ncontrol_script = "{root}/examples/voice/mock-control.rhai"\n[parameters]\nbootstrap_url = "http://127.0.0.1:{mock_port}"\n')
            for rerun in (False, True):
                output = work / f"result-{rerun}.json"
                arguments = [str(binary), "--cbl-api-key", "test", "--cbl-api-base-url", f"ws://127.0.0.1:{api_port}/v1", "--log-mode", "--output-file", str(output), "eval"]
                if rerun:
                    arguments += ["re-run"]
                arguments += ["voice", "--threshold", "0.5", "--max-turns", "4"]
                arguments += ["--evaluation-id", "1"] if rerun else ["--test-case-groups", "test"]
                arguments += ["livekit", "--config", str(config)]
                process = await asyncio.create_subprocess_exec(*arguments, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
                try:
                    stdout, stderr = await asyncio.wait_for(process.communicate(), 60)
                finally:
                    if process.returncode is None:
                        process.kill()
                        await process.wait()
                assert process.returncode == 0, (stdout + stderr).decode()
                result = json.loads(output.read_text())
                assert result["target"]["label"] == "e2e"
                assert len(result["voice_metadata"]) == 2
                assert all(len(turns) == 2 for turns in result["voice_metadata"].values())
                assert "secret" not in output.read_text()
            assert calls == [None, ["historic-source"]]
            for _ in range(100):
                if not mock.state.sessions:
                    break
                await asyncio.sleep(0.05)
            assert not mock.state.sessions, "completed sessions leaked on mock customer"
            cancel_mode = True
            cancelled_output = work / "cancelled.json"
            arguments = [str(binary), "--cbl-api-key", "test", "--cbl-api-base-url", f"ws://127.0.0.1:{api_port}/v1", "--log-mode", "--output-file", str(cancelled_output),
                "eval", "voice", "--threshold", "0.5", "--max-turns", "4", "--test-case-groups", "test", "livekit", "--config", str(config)]
            process = await asyncio.create_subprocess_exec(*arguments, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
            try:
                await asyncio.wait_for(active.wait(), 30)
                process.send_signal(signal.SIGINT)
                stdout, stderr = await asyncio.wait_for(process.communicate(), 15)
                assert process.returncode != 0
                assert not cancelled_output.exists()
                await asyncio.wait_for(cancelled.wait(), 5)
                assert b"secret" not in stdout + stderr
            finally:
                if process.returncode is None:
                    process.kill()
                    await process.wait()
            for _ in range(100):
                if not mock.state.sessions:
                    break
                await asyncio.sleep(0.05)
            assert not mock.state.sessions, "cancelled sessions leaked on mock customer"
            print("PASS: fresh/re-run, two concurrent conversations, two turns each, cancellation, and customer cleanup")
        finally:
            try:
                for server, task in reversed(servers):
                    server.should_exit = True
                    try:
                        await asyncio.wait_for(task, 10)
                    except (asyncio.TimeoutError, asyncio.CancelledError):
                        task.cancel()
            finally:
                if livekit.returncode is None:
                    livekit.terminate()
                    await livekit.wait()
                log.close()


if __name__ == "__main__":
    asyncio.run(main())
