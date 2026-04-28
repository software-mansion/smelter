"""Tests for socket-directory discovery and wait_for_channel."""

import os
import threading
import time
from pathlib import Path

import pytest
from smelter import (
    ChannelNotFound,
    Context,
    SideChannelKind,
    list_channels,
)
from smelter._discovery import filter_channels
from smelter.context import ENV_SOCKET_DIR
from smelter.sync import wait_for_channel


def _touch(path: Path) -> None:
    path.write_bytes(b"")


def _ctx(p: Path) -> Context:
    return Context(socket_dir=p)


def test_list_channels_parses_video_and_audio(tmp_path: Path):
    _touch(tmp_path / "video_input1.sock")
    _touch(tmp_path / "audio_input1.sock")
    _touch(tmp_path / "video_cam2.sock")
    _touch(tmp_path / "ignore_me.sock")
    _touch(tmp_path / "not_a_socket.txt")

    channels = list_channels(ctx=_ctx(tmp_path))
    by_id = {(c.kind, c.input_id): c for c in channels}
    assert (SideChannelKind.VIDEO, "input1") in by_id
    assert (SideChannelKind.AUDIO, "input1") in by_id
    assert (SideChannelKind.VIDEO, "cam2") in by_id
    assert len(channels) == 3
    assert by_id[(SideChannelKind.VIDEO, "input1")].path == tmp_path / "video_input1.sock"


def test_list_channels_missing_dir(tmp_path: Path):
    assert list_channels(ctx=_ctx(tmp_path / "nonexistent")) == []


def test_filter_channels(tmp_path: Path):
    _touch(tmp_path / "video_a.sock")
    _touch(tmp_path / "audio_a.sock")
    _touch(tmp_path / "video_b.sock")
    all_ch = list_channels(ctx=_ctx(tmp_path))

    only_video = filter_channels(all_ch, kind=SideChannelKind.VIDEO, input_id=None)
    assert {c.input_id for c in only_video} == {"a", "b"}

    only_a = filter_channels(all_ch, kind=None, input_id="a")
    assert {c.kind for c in only_a} == {SideChannelKind.VIDEO, SideChannelKind.AUDIO}


def test_wait_for_channel_returns_existing(tmp_path: Path):
    _touch(tmp_path / "video_x.sock")
    info = wait_for_channel(ctx=_ctx(tmp_path), kind=SideChannelKind.VIDEO, timeout=0.1)
    assert info.input_id == "x"


def test_wait_for_channel_times_out(tmp_path: Path):
    with pytest.raises(ChannelNotFound):
        wait_for_channel(
            ctx=_ctx(tmp_path),
            kind=SideChannelKind.VIDEO,
            timeout=0.05,
            poll_interval=0.01,
        )


def test_wait_for_channel_picks_up_late_arrival(tmp_path: Path):
    def create_late():
        time.sleep(0.05)
        _touch(tmp_path / "audio_late.sock")

    threading.Thread(target=create_late, daemon=True).start()
    info = wait_for_channel(
        ctx=_ctx(tmp_path),
        kind=SideChannelKind.AUDIO,
        timeout=1.0,
        poll_interval=0.02,
    )
    assert info.input_id == "late"


def test_context_uses_env_var(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    _touch(tmp_path / "video_envtest.sock")
    monkeypatch.setenv(ENV_SOCKET_DIR, str(tmp_path))
    # Default context (no args) should pick up the env var.
    info = wait_for_channel(kind=SideChannelKind.VIDEO, timeout=0.1)
    assert info.input_id == "envtest"


def test_context_falls_back_to_cwd(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
):
    monkeypatch.delenv(ENV_SOCKET_DIR, raising=False)
    monkeypatch.chdir(tmp_path)
    _touch(tmp_path / "audio_cwdtest.sock")
    info = wait_for_channel(kind=SideChannelKind.AUDIO, timeout=0.1)
    assert info.input_id == "cwdtest"


def test_context_repr_and_eq(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv(ENV_SOCKET_DIR, str(tmp_path))
    a = Context()
    b = Context(socket_dir=tmp_path)
    assert a == b
    assert hash(a) == hash(b)
    assert "Context(" in repr(a)


def test_context_explicit_overrides_env(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv(ENV_SOCKET_DIR, "/nonsense")
    ctx = Context(socket_dir=tmp_path)
    assert ctx.socket_dir == Path(tmp_path)
    # Sanity: it really is the test path, not the env var.
    assert os.fspath(ctx.socket_dir) != "/nonsense"
