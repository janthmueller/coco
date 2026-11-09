"""Model-free real-shell picker checks using an isolated RPC socket and PTY.

Run after `cargo build --bins`:
    python3 tests/navigation/terminal_smoke.py target/debug/coco

Requires Bash, Zsh and Fish on PATH. No daemon, Codex, user configuration or
user worktree is used. The fake daemon accepts only directory-navigation reads.
"""

import errno
import fcntl
import json
import os
import pty
import re
import select
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import termios
import time
from pathlib import Path


def run(binary: Path, shell: str, key: bytes, expected: int | None) -> None:
    executable = shutil.which(shell)
    if executable is None:
        raise RuntimeError(f"{shell} is required for this explicit smoke test")
    with tempfile.TemporaryDirectory(prefix="coco-navigation-") as temporary:
        root = Path(temporary).resolve()
        destinations = [root / "über 'quotes' $vars ; [glob]", root / "second worktree"]
        for destination in destinations:
            destination.mkdir()
        init = root / "init"
        init.write_bytes(subprocess.check_output([binary, "shell", "init", shell]))
        endpoint = root / "rpc.sock"
        with socket.socket(socket.AF_UNIX) as listener:
            listener.bind(str(endpoint))
            listener.listen()
            environment = {
                **os.environ,
                "PATH": str(binary.parent) + os.pathsep + os.environ.get("PATH", ""),
                "COCO_TEST_INIT": str(init),
                "COCO_SOCKET_PATH": str(endpoint),
                "COCO_DATA_DIR": str(root / "data"),
                "CODEX_HOME": str(root / "codex"),
                "XDG_CONFIG_HOME": str(root / "config"),
                "XDG_DATA_HOME": str(root / "data"),
                "NO_COLOR": "1",
                "TERM": "xterm-256color",
            }
            save = "set -l result $status" if shell == "fish" else "result=$?"
            script = (
                'source "$COCO_TEST_INIT"\ncoco cd -g\n'
                f"{save}\nprintf '\\nNAV-RESULT=%s\\nNAV-CWD=%s\\n' \"$result\" \"$PWD\"\n"
            )
            flags = {"bash": ["--noprofile", "--norc"], "zsh": ["-f"], "fish": ["--no-config"]}[shell]
            pid, terminal = pty.fork()
            if pid == 0:
                os.chdir(root)
                os.execve(executable, [executable, *flags, "-c", script], environment)
            fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 100, 0, 0))
            before = termios.tcgetattr(terminal)[3]
            calls: list[dict] = []
            try:
                screen = drive(listener, terminal, key, destinations, calls)
                _, status = os.waitpid(pid, 0)
                pid = 0
                assert os.waitstatus_to_exitcode(status) == 0, screen
                text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", screen).replace("\r", "")
                wanted = root if expected is None else destinations[expected]
                result = 1 if expected is None else 0
                assert f"NAV-RESULT={result}\n" in text, text
                assert f"NAV-CWD={wanted}\n" in text, text
                after = termios.tcgetattr(terminal)[3]
                assert before & (termios.ECHO | termios.ICANON) == after & (termios.ECHO | termios.ICANON)
                methods = [call["method"] for call in calls]
                assert methods == (["workspace.path.list"] if expected is None else ["workspace.path.list", "workspace.path.get"]), methods
                if expected is not None:
                    assert calls[-1]["params"]["workspace"] == f"workspace-{expected}", calls
            finally:
                if pid:
                    os.kill(pid, signal.SIGKILL)
                    os.waitpid(pid, 0)
                os.close(terminal)


def drive(listener: socket.socket, terminal: int, key: bytes, destinations: list[Path], calls: list[dict]) -> str:
    output = bytearray()
    sent = False
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        readable, _, _ = select.select([listener, terminal], [], [], 0.1)
        if listener in readable:
            with listener.accept()[0] as connection:
                connection.settimeout(2)
                request = json.loads(connection.makefile("rb").readline())
                calls.append(request)
                assert request["params"]["scope"] == {"kind": "allRepositories"}, request
                method = request["method"]
                if method == "workspace.path.list":
                    result = [
                        {"workspaceId": f"workspace-{index}", "name": name,
                         "repository": {"id": "repo", "displayName": "repo", "rootPath": "/repo"},
                         "path": str(destinations[index]), "branchName": name, "worktreeMode": "new_branch"}
                        for index, name in [(1, "feat/10"), (0, "feat/2")]
                    ]
                elif method == "workspace.path.get":
                    index = {"workspace-0": 0, "workspace-1": 1}[request["params"]["workspace"]]
                    result = str(destinations[index])
                else:
                    raise AssertionError(f"navigation queried {method}")
                connection.sendall((json.dumps({"id": request["id"], "result": result}) + "\n").encode())
        if terminal in readable:
            try:
                chunk = os.read(terminal, 65536)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                break
            if not chunk:
                break
            output.extend(chunk)
            if not sent and b"Workspace directory" in output:
                os.write(terminal, key)
                sent = True
    else:
        raise AssertionError(f"picker timed out: {output.decode(errors='replace')}")
    assert sent, output.decode(errors="replace")
    return output.decode(errors="replace")


if __name__ == "__main__":
    executable = Path(sys.argv[1]).resolve(strict=True)
    for name in ("bash", "zsh", "fish"):
        for pressed, target in ((b"\r", 0), (b"2", 1), (b"q", None)):
            run(executable, name, pressed, target)
        print(f"{name}: real CLI picker, number selection, cancellation, quoting and terminal restore pass")
