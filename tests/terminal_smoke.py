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
    terminal.keys("Tab", "j")
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


def check_creation_defaults(terminal: Terminal) -> None:
    terminal.tmux("resize-window", "-t", "probe", "-x", "80", "-y", "14")
    for cancel in (False, True):
        terminal.start_test("cli::create::tests::defaults::interactive_defaults_walkthrough_probe")
        for title in ("Worktree", "Code", "Git changes"):
            terminal.wait(f"{title} (1/")
            terminal.wait("Tab defaults")
            terminal.keys("Enter")
        terminal.wait("Context (1/")
        terminal.keys("3")
        terminal.wait("Codex thread ID")
        terminal.keys("probe-context", "Enter")
        terminal.wait("Context > Size (1/2)")
        terminal.tmux("resize-window", "-t", "probe", "-x", "24", "-y", "5")
        terminal.wait("Tab defaults")
        terminal.keys("Down")
        terminal.wait("Context > Size (2/2)")
        terminal.tmux("resize-window", "-t", "probe", "-x", "80", "-y", "14")
        terminal.wait("Full context")
        terminal.keys("Tab")
        screen = terminal.wait("Create workspace (1/3)")
        assert "thread probe-context" in screen, screen
        assert "Tab defaults" not in screen, screen
        terminal.keys("Tab", "End")
        terminal.wait("Create workspace (3/3)")
        if not cancel:
            terminal.keys("Home")
        terminal.keys("Enter")
        terminal.wait("defaults-result=Err(" if cancel else "defaults-result=Ok(")
        terminal.wait("test result: ok.")
        assert terminal.flags() == "1:1", "creation did not restore the terminal"
    print("create: Tab applies remaining defaults, keeps context, and requires final approval")


def check_creation_navigation(terminal: Terminal) -> None:
    probe = "cli::create::tests::navigation::interactive_navigation_walkthrough_probe"
    terminal.tmux("resize-window", "-t", "probe", "-x", "80", "-y", "20")
    terminal.start_test(probe)
    terminal.wait("Workspace name")
    terminal.keys("probe/first", "Enter")
    terminal.wait("Worktree (1/4)")
    terminal.keys("2")
    terminal.wait("Branch name")
    assert terminal.flags() == "1:0", "text must show its caret without wrapping"
    terminal.tmux("set-buffer", "-b", "coco-input-test", "bad\nq\x1b[31m")
    terminal.tmux("paste-buffer", "-p", "-b", "coco-input-test", "-t", "probe")
    screen = terminal.wait("> bad q")
    assert "[31m" in screen, "pasted escape must remain harmless visible data"
    assert "\x1b[31m" not in terminal.screen(colored=True), "pasted escape set a terminal color"
    assert "Branch name" in terminal.screen(), "pasted newline submitted a field"
    terminal.keys("Escape")
    terminal.wait("Worktree (2/4)")
    terminal.keys("BTab")
    terminal.wait("> probe/first")
    terminal.keys("BTab")  # Root Shift-Tab is a no-op, not cancellation.
    terminal.wait("> probe/first")
    terminal.keys("C-u", "probe/ü界", "Left", "BSpace")
    terminal.wait("> probe/界")
    terminal.tmux("resize-window", "-t", "probe", "-x", "12", "-y", "4")
    terminal.wait("Enter cont")
    terminal.tmux("resize-window", "-t", "probe", "-x", "80", "-y", "20")
    terminal.keys("C-a", "C-k", "probe/second", "Enter")
    terminal.wait("Worktree (2/4)")
    terminal.keys("Home", "Enter")
    terminal.wait("Code (1/4)")
    terminal.keys("2")
    terminal.wait("Code > Branch")
    terminal.keys("Escape")
    terminal.wait("Code (2/4)")
    terminal.keys("4")
    terminal.wait("Git revision")
    terminal.keys("main", "Home", "DC")
    terminal.wait("> ain")
    terminal.keys("Escape")
    terminal.wait("Code (4/4)")
    terminal.keys("Home", "Enter")
    terminal.wait("Git changes")
    terminal.keys("Tab")
    terminal.wait("Create workspace (1/3)")
    terminal.tmux("set-buffer", "-b", "coco-input-test", "1\n")
    terminal.tmux("paste-buffer", "-p", "-b", "coco-input-test", "-t", "probe")
    terminal.keys("Down")
    terminal.wait("Create workspace (2/3)")
    terminal.keys("2")
    terminal.wait("Edit settings")
    terminal.keys("1")
    terminal.wait("> probe/second")
    terminal.keys("C-u", "probe/final", "Enter")
    screen = terminal.wait("Create workspace (1/3)")
    assert "branch probe/final" in screen, screen
    terminal.keys("BTab")
    terminal.wait("> probe/final")
    terminal.keys("Enter")
    terminal.wait("Create workspace (1/3)")
    terminal.keys("Enter")
    terminal.wait("navigation-result=Ok(")
    terminal.wait("test result: ok.")
    assert terminal.flags() == "1:1", "navigation did not restore terminal modes"
    for in_text in (False, True):
        terminal.start_test(probe, plain=True)
        terminal.wait("Workspace name")
        if not in_text:
            terminal.keys("draft", "Enter")
            terminal.wait("Worktree")
        terminal.keys("C-c")
        terminal.wait("navigation-result=Err(")
        terminal.wait("test result: ok.")
        assert terminal.flags() == "1:1", "Ctrl-C left a terminal mode active"
    print("create: Back, Shift-Tab, prefill, Unicode, safe paste, edit/review and Ctrl-C pass")


def check_creation_recovery(terminal: Terminal) -> None:
    terminal.start_test("cli::create::tests::navigation::safety::interactive_recovery_walkthrough_probe")
    terminal.wait("Worktree")
    terminal.keys("Tab")
    terminal.wait("read-only discovery temporarily unavailable")
    terminal.wait("Retry")
    terminal.keys("Escape")
    terminal.wait("Worktree")
    terminal.keys("Tab")
    terminal.wait("Create workspace (1/3)")
    terminal.keys("3")
    terminal.wait("recovery-result=Err(")
    terminal.wait("test result: ok.")
    assert terminal.flags() == "1:1", "recovery left terminal modes active"

    print("create: failed discovery supports Back/Retry")


def check_creation_layout_failure(binary: str) -> None:
    # Keep a deliberately unusable PTY isolated from subsequent input probes.
    # Fresh history also prevents matching an earlier cancellation's result.
    with tempfile.TemporaryDirectory(prefix="coco-terminal-layout-") as directory:
        terminal = Terminal(directory, binary)
        terminal.tmux(
            "-f", "/dev/null", "new-session", "-d", "-s", "probe",
            "-x", "80", "-y", "20",
            "env TERM=xterm-256color INPUTRC=/dev/null HISTFILE=/dev/null "
            "'PS1=probe-ready> ' bash --noprofile --norc -i",
        )
        try:
            terminal.start_test("cli::create::tests::navigation::interactive_navigation_walkthrough_probe")
            terminal.wait("Workspace name")
            terminal.tmux("resize-window", "-t", "probe", "-x", "80", "-y", "3")
            terminal.wait_until(
                lambda _: "test result: ok." in terminal.tmux("capture-pane", "-S", "-100", "-pt", "probe"),
                "the completed layout-error probe",
            )
            history = terminal.tmux("capture-pane", "-S", "-100", "-pt", "probe")
            assert "navigation-result=Err(" in history and "terminal is too small" in history, history
            assert terminal.flags() == "1:1", "input/layout failure left terminal modes active"
        finally:
            terminal.tmux("kill-server")
    print("create: isolated layout failure restores terminal modes")


def check_abandoned_creation_edit(terminal: Terminal) -> None:
    for back in ("Escape", "BTab"):
        terminal.start_test("cli::create::tests::navigation::interactive_navigation_walkthrough_probe")
        terminal.wait("Workspace name")
        terminal.keys("probe/final", "Enter")
        for title in ("Worktree", "Code", "Git changes"):
            terminal.wait(title + " (1/")
            terminal.keys("Enter")
        terminal.wait("Context (1/3)")
        terminal.keys("3")
        terminal.wait("Codex thread ID")
        terminal.keys("thread-old", "Enter")
        terminal.wait("Context > Size (1/2)")
        terminal.keys("1")
        terminal.wait("Profile (1/2)")
        terminal.keys("Tab")
        terminal.wait("Create workspace (1/3)")
        terminal.keys("2")
        terminal.wait("Edit settings")
        terminal.keys("5")
        terminal.wait("Context (4/4)")
        terminal.keys("4")
        terminal.wait("> thread-old")
        terminal.keys("C-u", "thread-abandoned", "Enter")
        terminal.wait("Context > Size")
        terminal.keys(back)
        terminal.wait("Context (4/4)")
        terminal.keys(back)
        terminal.wait("Edit settings")
        terminal.keys(back)
        screen = terminal.wait("Create workspace (1/3)")
        assert "thread thread-old" in screen and "thread-abandoned" not in screen, screen
        terminal.keys(back)
        terminal.wait("Profile (1/2)")  # Must leave the edit return mode.
        terminal.keys("Tab")
        terminal.wait("Create workspace (1/3)")
        terminal.keys("2")
        terminal.wait("Edit settings")
        terminal.keys("5")
        terminal.wait("Context (4/4)")
        terminal.keys("4")
        screen = terminal.wait("> thread-old")
        assert "thread-abandoned" not in screen, "abandoned input survived rollback"
        terminal.keys("C-c")
        terminal.wait("navigation-result=Err(")
        terminal.wait("test result: ok.")
        assert terminal.flags() == "1:1"
    print("create: abandoned edits restore accepted input and normal Back for Esc and Shift-Tab")


def check_creation_history(terminal: Terminal) -> None:
    terminal.start_test("cli::create::tests::navigation::interactive_navigation_walkthrough_probe")
    terminal.wait("Workspace name")
    terminal.keys("probe/final", "Enter")
    for title in ("Worktree", "Code"):
        terminal.wait(title + " (1/")
        terminal.keys("Enter")
    terminal.wait("Git changes (1/2)")
    terminal.keys("Tab")
    terminal.wait("Create workspace (1/3)")
    terminal.keys("2")
    terminal.wait("Edit settings")
    terminal.keys("2")
    terminal.wait("Worktree (1/4)")
    terminal.keys("3")
    terminal.wait("Worktree > Branch (1/1)")
    terminal.keys("Enter")
    terminal.wait("Create workspace (1/3)")
    terminal.keys("Escape")
    terminal.wait("Worktree (3/4)")
    terminal.keys("Escape")
    terminal.wait("Git changes (1/2)")
    terminal.keys("Escape")
    terminal.wait("Worktree (3/4)")  # Historical Code no longer applies.
    terminal.keys("C-c")
    terminal.wait("navigation-result=Err(")
    terminal.wait("test result: ok.")
    assert terminal.flags() == "1:1"
    print("create: Back skips inapplicable code history after existing-branch adoption")


def check_clean_creation_policy(terminal: Terminal) -> None:
    terminal.start_test("cli::create::tests::navigation::carry::interactive_clean_checkout_walkthrough_probe")
    for title in ("Worktree", "Code"):
        terminal.wait(title + " (1/")
        terminal.keys("Enter")
    terminal.wait("Git changes (1/2)")
    terminal.keys("2")
    terminal.wait("Context (1/3)")
    terminal.keys("Tab")
    screen = terminal.wait("Create workspace (1/3)")
    assert re.search(r"Git changes\s+don't copy", screen), screen
    terminal.keys("2")
    terminal.wait("Edit settings")
    terminal.keys("4")
    terminal.wait("Git changes (1/2)")
    terminal.keys("Enter")
    screen = terminal.wait("Create workspace (1/3)")
    assert re.search(r"Git changes\s+don't copy", screen), screen
    terminal.keys("Enter")
    terminal.wait("clean-policy-result=ok")
    terminal.wait("test result: ok.")
    assert terminal.flags() == "1:1"
    print("create: clean checkout reset remains Don't copy when editing and pressing Enter")


def check_git_tracking(terminal: Terminal) -> None:
    probe = "cli::prompt::tests::trailing::interactive_tracking_probe"
    for plain in (False, True):
        terminal.start_test(probe, plain)
        screen = terminal.wait("Git tracking (1/2)")
        assert screen.count("feat/login") == 1, screen
        assert "Working · ↑2 ↓1" in screen and "5m ago" in screen, screen
        colored = terminal.screen(colored=True)
        yellow_badge = re.search(
            r"\x1b\[(?:[0-9;]*33|38;5;11)m(?:\x1b\[[0-9;]*m)*↑2 ↓1", colored
        )
        assert bool(yellow_badge) != plain, colored
        terminal.keys("Enter")
        terminal.wait("tracking-result=0")
        terminal.wait("test result: ok.")
        assert terminal.flags() == "1:1", "Git picker did not restore terminal state"
    print("git tracking: semantic badge colors, NO_COLOR, recency and terminal cleanup")


def check_follow(terminal: Terminal) -> None:
    terminal.start_test("cli::follow::tests::interactive_follow_terminal_probe")
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
            check_creation_defaults(terminal)
            check_creation_navigation(terminal)
            check_creation_recovery(terminal)
            check_abandoned_creation_edit(terminal)
            check_creation_history(terminal)
            check_clean_creation_policy(terminal)
            check_confirmations(terminal)
            check_git_tracking(terminal)
            check_follow(terminal)
        finally:
            terminal.tmux("kill-server")
    check_creation_layout_failure(sys.argv[1])


if __name__ == "__main__":
    main()
