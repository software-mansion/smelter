"""Exception types raised by the smelter SDK."""


class SmelterError(Exception):
    """Base class for all errors raised by this SDK."""


class ConnectionClosed(SmelterError):
    """The peer closed the side-channel socket.

    Raised by ``recv()`` when the smelter server stops sending — typically when
    the input ends, the pipeline is torn down, or the server process exits.
    Iteration over a connection swallows this and stops cleanly.
    """


class ProtocolError(SmelterError):
    """A message from the server did not match the expected wire format.

    Indicates a version mismatch between the SDK and the smelter server, or a
    corrupted stream. Always a bug somewhere — never a normal end-of-stream.
    """


class ChannelNotFound(SmelterError, TimeoutError):
    """``wait_for_channel`` timed out before a matching socket appeared."""


class RecvTimeout(SmelterError, TimeoutError):
    """``recv()`` exceeded its configured timeout without receiving a message."""
