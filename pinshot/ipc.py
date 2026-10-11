"""Single-instance IPC via QLocalServer / QLocalSocket."""
import json, os
from PySide6.QtCore import QObject, Signal, QCoreApplication
from PySide6.QtNetwork import QLocalServer, QLocalSocket

if hasattr(os, "getuid"):
    SERVER_NAME = f"pinshot-{os.getuid()}"          # unix socket name (Linux/macOS)
else:                                              # Windows: named pipe, per user name
    from .platform.base import BasePlatform as _BP
    SERVER_NAME = _BP().instance_name()


class IpcServer(QObject):
    command = Signal(dict)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.server = QLocalServer(self)
        probe = QLocalSocket(); probe.connectToServer(SERVER_NAME)
        if probe.waitForConnected(200):          # a live instance owns it: never steal its socket
            probe.disconnectFromServer()
            raise RuntimeError("another PinShot instance owns " + SERVER_NAME)
        QLocalServer.removeServer(SERVER_NAME)   # stale socket from a crashed instance
        if not self.server.listen(SERVER_NAME):
            raise RuntimeError(self.server.errorString())
        self.server.newConnection.connect(self._accept)

    def _accept(self):
        while self.server.hasPendingConnections():
            sock = self.server.nextPendingConnection()
            sock.readyRead.connect(lambda s=sock: self._read(s))

    def _read(self, sock: QLocalSocket):
        data = bytes(sock.readAll())
        try:
            msg = json.loads(data.decode("utf-8"))
        except Exception:
            msg = {"cmd": "raise"}
        self.command.emit(msg if isinstance(msg, dict) else {"cmd": "raise"})
        sock.write(b'{"ok":true}'); sock.flush(); sock.disconnectFromServer()


def try_send(msg: dict, timeout_ms=800) -> bool:
    """If another instance owns the server, send msg and return True."""
    sock = QLocalSocket()
    sock.connectToServer(SERVER_NAME)
    if not sock.waitForConnected(timeout_ms):
        return False
    payload = json.dumps(msg).encode("utf-8")
    sock.write(payload); sock.flush()
    sock.waitForBytesWritten(timeout_ms)
    # Give the server a chance to answer if an event loop is running
    app = QCoreApplication.instance()
    if app is not None:
        for _ in range(20):
            if sock.bytesAvailable() > 0: break
            app.processEvents(); sock.waitForReadyRead(50)
    else:
        sock.waitForReadyRead(timeout_ms)
    sock.disconnectFromServer()
    return True
