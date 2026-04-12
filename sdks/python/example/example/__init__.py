"""Example usage of smelter-sdk side channel client."""

import sys

from smelter import SideChannelKind, SideChannelManager


def main():
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <socket_dir>")
        sys.exit(1)

    manager = SideChannelManager(sys.argv[1])
    channels = manager.list()

    if not channels:
        print("No side channel sockets found.")
        sys.exit(0)

    print("Available channels:")
    for i, info in enumerate(channels):
        print(f"  [{i}] {info.kind.value} input_id={info.input_id}")

    choice = input(f"Select channel [0-{len(channels) - 1}]: ")
    try:
        idx = int(choice)
        info = channels[idx]
    except (ValueError, IndexError):
        print("Invalid selection.")
        sys.exit(1)

    print(f"Connecting to {info.kind.value} channel for input '{info.input_id}'...")

    with manager.connect(info) as conn:
        try:
            while True:
                msg = conn.recv()
                if info.kind == SideChannelKind.VIDEO:
                    plane_sizes = [len(p) for p in msg.planes]
                    print(
                        f"Video: {msg.width}x{msg.height} "
                        f"pts={msg.pts_nanos / 1e9:.3f}s "
                        f"format={msg.format} "
                        f"planes={plane_sizes}"
                    )
                else:
                    channels_str = "mono" if msg.channel_count == 1 else "stereo"
                    print(
                        f"Audio: pts={msg.start_pts_nanos / 1e9:.3f}s "
                        f"rate={msg.sample_rate} "
                        f"{channels_str} "
                        f"samples={msg.sample_count}"
                    )
        except ConnectionError:
            print("Connection closed")
