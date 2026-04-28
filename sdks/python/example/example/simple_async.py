"""Minimal asyncio side-channel subscriber.

Run a smelter server with side channels enabled on one or more inputs,
export `SMELTER_SIDE_CHANNEL_SOCKET_DIR` to its socket directory, then::

    uv run example-simple-async

Prints one line per video frame and per audio batch from every published
side channel.
"""

import asyncio
import contextlib

from smelter import SideChannelInfo, SideChannelKind
from smelter.aio import connect_audio, connect_video, list_channels


async def _consume_video(info: SideChannelInfo) -> None:
    tag = f"video[{info.input_id}]"
    conn = await connect_video(info)
    async with conn:
        async for frame in conn:
            print(f"{tag}: {frame.width}x{frame.height} pts={frame.pts_seconds:.3f}s")


async def _consume_audio(info: SideChannelInfo) -> None:
    tag = f"audio[{info.input_id}]"
    conn = await connect_audio(info)
    async with conn:
        async for batch in conn:
            print(
                f"{tag}: {batch.sample_count} samples x {batch.channels}ch "
                f"@ {batch.sample_rate}Hz pts={batch.start_pts_seconds:.3f}s"
            )


async def _run() -> None:
    channels = await list_channels()
    if not channels:
        print("No side channels found (set SMELTER_SIDE_CHANNEL_SOCKET_DIR).")
        return

    print(f"Found {len(channels)} channel(s):")
    for info in channels:
        print(f"  - {info.kind} for input '{info.input_id}'")

    async with asyncio.TaskGroup() as tg:
        for info in channels:
            target = _consume_video if info.kind is SideChannelKind.VIDEO else _consume_audio
            tg.create_task(target(info))


def main() -> None:
    with contextlib.suppress(KeyboardInterrupt):
        asyncio.run(_run())
