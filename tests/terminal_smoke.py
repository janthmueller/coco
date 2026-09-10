"""Opt-in tmux checks for the real inline picker; never use the user's tmux server.

Build the library tests with `cargo test -j1 --locked --lib --no-run`, then pass
the printed test executable to this script. No daemon, repository, or model
is involved. tmux and bash must be available on PATH.
"""

import re
import shlex
import subprocess
import sys
import tempfile
import time
from pathlib import Path


class Terminal:
    def __init__(self, directory: str, binary: str):
        self.socket = str(Path(directory) / "test.sock")
        self.binary = str(Path(binary).resolve(strict=True))

    def tmux(self, *arguments: str) -> str:
        result = subprocess.run(
            ["tmux", "-S", self.socket, *arguments],
            check=True,
            capture_output=True,
            text=True,
            timeout=10,
        )
        return result.stdout

    def screen(self, colored: bool = False) -> str:
        return self.tmux("capture-pane", "-ept" if colored else "-pt", "probe")

    def keys(self, *keys: str) -> None:
        self.tmux("send-keys", "-t", "probe", *keys)

    def wait(self, text: str) -> str:
        return self.wait_until(lambda screen: text in screen, repr(text))

    def wait_until(self, predicate, description: str) -> str:
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            screen = self.screen()
            if predicate(screen):
                return screen
            time.sleep(0.03)
        raise AssertionError(f"terminal did not show {description}:\n{screen}")

    def start(self, name: str, plain: bool = False) -> None:
        self.start_test(f"cli::prompt::tests::{name}", plain)

    def start_test(self, test: str, plain: bool = False) -> None:
        self.wait_until(
            lambda screen: screen.rstrip().endswith("probe-ready>"),
            "an idle shell prompt",
        )
        self.keys(r"printf '\033[H\033[2J'", "Enter")
        self.wait_until(
            lambda screen: screen.strip() == "probe-ready>",
            "a cleared shell prompt",
        )
        command = ["env", "-u", "NO_COLOR", "TERM=xterm-256color"]
        if plain:
            command.append("NO_COLOR=1")
        command.extend([
            self.binary, "--ignored", "--exact",
            test, "--nocapture",
        ])
        self.keys(shlex.join(command), "Enter")

    def flags(self) -> str:
        return self.tmux("display-message", "-pt", "probe", "#{cursor_flag}:#{wrap_flag}").strip()

    def finish_picker(self, key: str, expected: str) -> None:
        self.keys(key)
        self.wait(f"picker-result={expected}")
        self.wait("test result: ok.")
        assert self.flags() == "1:1", "cursor or wrapping was not restored"
        assert "workspace-" not in self.screen(), "picker left stale rows behind"


def picker_rows(screen: str) -> list[str]:
    return re.findall(r"^[ ›] +\d+  workspace-\d+", screen, re.MULTILINE)


def assert_selection(screen: str, selected: int, visible: int) -> None:
    rows = picker_rows(screen)
    assert len(rows) == visible, screen
    assert [row for row in rows if row.startswith("›")] == [
        f"› {selected:2}  workspace-{selected:02}"
    ], screen


def check_picker(terminal: Terminal) -> None:
    terminal.start("interactive_terminal_probe")
    screen = terminal.wait("Choose a workspace (1/12)")
    assert "history-29" in screen, "picker erased preceding output"
    for index in range(1, 10):
        assert screen.count(f"workspace-{index:02}") == 1, screen
    assert_selection(screen, 1, 9)
    assert terminal.flags() == "0:0", "picker did not hide the hardware cursor"
    colored = terminal.screen(colored=True)
    assert "\x1b[" in colored and "[workspace-01]" not in colored
    terminal.keys("j")
    screen = terminal.wait("Choose a workspace (2/12)")
    assert_selection(screen, 2, 9)
    assert screen.count("workspace-09") == 1, "last row was duplicated"
    terminal.tmux("resize-window", "-t", "probe", "-x", "80", "-y", "5")
    terminal.wait("Choose a workspace (2/12)")
    terminal.keys("End")
    screen = terminal.wait("Choose a workspace (12/12)")
    assert screen.count("workspace-12") == 1, screen
    assert_selection(screen, 12, 3)
    terminal.keys("PageUp")
    assert_selection(terminal.wait("Choose a workspace (9/12)"), 9, 3)
    terminal.tmux("resize-window", "-t", "probe", "-x", "24", "-y", "14")
    terminal.keys("Home")
    screen = terminal.wait_until(
        lambda screen: "›  1  workspace-01" in screen
        and len(picker_rows(screen)) == 9,
        "the resized nine-row picker",
    )
    assert_selection(screen, 1, 9)
    terminal.finish_picker("3", "Ok(2)")
    terminal.tmux("resize-window", "-t", "probe", "-x", "80", "-y", "14")
    for key in ("q", "Escape", "C-c"):
        terminal.start("interactive_terminal_probe", plain=True)
        screen = terminal.wait("›  1  workspace-01")
        assert_selection(screen, 1, 9)
        terminal.keys("j")
        assert_selection(terminal.wait("›  2  workspace-02"), 2, 9)
        terminal.finish_picker(key, "Err(")
    print("picker: bottom margin, navigation, resize, color, cancellation, and cursor restore pass")


def check_confirmations(terminal: Terminal) -> None:
    for answer, expected in (("", "false"), ("n", "false"), ("y", "true"), ("YES", "true")):
        terminal.start("interactive_confirmation_probe")
        terminal.wait("Proceed? [y/N]")
        assert terminal.flags() == "1:1", "line input must retain the visible cursor"
        if answer:
            terminal.keys(answer)
        terminal.keys("Enter")
        terminal.wait(f"confirmation-result=Ok({expected})")
    terminal.start("interactive_confirmation_probe")
    terminal.wait("Proceed? [y/N]")
    terminal.keys("maybe", "Enter")
    terminal.wait("Please enter y or n.")
    terminal.keys("n", "Enter")
    terminal.wait("confirmation-result=Ok(false)")
    terminal.start("interactive_confirmation_probe")
    terminal.wait("Proceed? [y/N]")
    terminal.keys("C-d")
    terminal.wait("confirmation-result=Err(input ended before confirmation)")
    print("confirmation: Enter defaults to No; y/n, retries, and EOF pass")


def check_follow(terminal: Terminal) -> None:
    terminal.start_test("cli::status::tests::interactive_follow_terminal_probe")
    terminal.wait("follow-result=ok")
    terminal.wait("test result: ok.")
    screen = terminal.screen()
    assert screen.count("probe/test Waiting") == 1, screen
    assert "probe/test Working" not in screen, screen
    assert "probe/test Ready" not in screen, screen
    assert terminal.flags() == "1:1", "follow did not restore cursor or wrapping"
    print("follow: bottom-margin frames replace in place and restore terminal state")


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: python3 tests/terminal_smoke.py <library-test-executable>")
    with tempfile.TemporaryDirectory(prefix="coco-terminal-check-") as directory:
        terminal = Terminal(directory, sys.argv[1])
        terminal.tmux(
            "-f", "/dev/null", "new-session", "-d", "-s", "probe",
            "-x", "80", "-y", "14",
            "env TERM=xterm-256color INPUTRC=/dev/null HISTFILE=/dev/null "
            "'PS1=probe-ready> ' bash --noprofile --norc -i",
        )
        try:
            check_picker(terminal)
            check_confirmations(terminal)
            check_follow(terminal)
        finally:
            terminal.tmux("kill-server")


if __name__ == "__main__":
    main()
