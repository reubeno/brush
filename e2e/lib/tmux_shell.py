"""Drives an interactive shell inside tmux, for suites that assert on the rendered screen.

`pty_shell.py` matches a byte stream, which is all a prompt or a hook needs. A full-screen TUI
-- atuin's ctrl-r search -- can only be checked against a terminal that actually drew it, with
its cursor addressing and redraws resolved, so those suites come through here.

tmux itself is driven by libtmux, the binding tmuxp is built on; what is left here is only the
waiting, which is the part that is particular to testing a shell.
"""

import logging
import os
import re
import shlex
import time

from libtmux import Server


# Seconds to wait for any one screen state, as for the PTY suites.
TIMEOUT = int(os.environ.get("E2E_TEST_TIMEOUT", "10"))
LOG = logging.getLogger(__name__)


class TmuxShell:
    """An interactive shell in a detached tmux session, on a socket private to one test."""

    def __init__(self, socket, shell, rc, prompt, width=100, height=30):
        self.prompt = prompt
        self.server = Server(socket_path=str(socket))
        # tmux runs the command through `sh -c`, so the paths interpolated into it (the shell
        # binary, the rcfile) have to survive another round of word splitting.
        command = shlex.join([str(shell), "--noprofile", "--rcfile", str(rc)])
        session = self.server.new_session(window_command=command, x=width, y=height)
        self.pane = session.attached_pane
        self.wait_for_prompt()
        LOG.info("shell startup:\n%s", self.screen())

    def close(self):
        self.server.kill()

    def send_text(self, text):
        """Types literal text, without running it."""
        self.pane.send_keys(text, enter=False, literal=True)

    def send_key(self, key):
        """Sends one key as tmux names it: `Enter`, `C-r`, `Up`, `Escape`."""
        self.pane.send_keys(key, enter=False, literal=False)

    def type_line(self, line):
        """Types a command line and runs it, returning once the shell has drawn a *new* prompt.

        Waiting for "a prompt" would not do: a command that prints nothing and leaves the prompt
        unchanged (`bind -s > file`) leaves the screen exactly as it was, so the wait is satisfied
        by the prompt the command was typed at -- before the shell has even read the keys -- and
        the test then inspects an effect that has not happened yet.

        An empty line runs nothing and just waits for the next prompt, which is how a suite gives
        a precmd hook its chance to fire.
        """
        before = self.prompt_count()
        if line:
            self.send_text(line)
        self.send_key("Enter")
        self.wait_for_prompt_count(before + 1)
        LOG.info("$ %s\n%s", line, self.screen())

    def lines(self):
        """The visible screen, trailing blank lines removed."""
        screen = self.pane.capture_pane()
        while screen and not screen[-1]:
            screen.pop()
        return screen

    def screen(self):
        return "\n".join(self.lines())

    def prompt_count(self):
        """How many prompts the visible screen holds, including the one being typed at."""
        return sum(1 for line in self.lines() if line.startswith(self.prompt))

    def at_prompt(self):
        """Whether the last line on screen is a prompt."""
        screen = self.lines()
        return bool(screen) and screen[-1] == self.prompt

    def wait_for(self, pattern):
        """Waits until some screen line matches `pattern`."""
        self._wait(lambda: self._matches(pattern) > 0, f"a line matching /{pattern}/")

    def wait_for_prompt(self):
        """Waits for the shell to show a prompt as the last line."""
        self._wait(self.at_prompt, "a prompt on the last line")

    def wait_for_prompt_count(self, expected):
        """Waits until the screen holds at least `expected` prompts and the last line is one."""
        self._wait(
            lambda: self.at_prompt() and self.prompt_count() >= expected, f"{expected} prompt(s)"
        )

    def wait_for_screen_count(self, pattern, expected):
        """Waits until exactly `expected` screen lines match `pattern`, failing fast past it."""

        def counted():
            actual = self._matches(pattern)
            # More than asked for will never come back down, so stop rather than wait it out.
            assert actual <= expected, (
                f"expected {expected} line(s) matching /{pattern}/, found {actual}; "
                f"screen:\n{self.screen()}"
            )
            return actual == expected

        self._wait(counted, f"{expected} line(s) matching /{pattern}/")

    def _matches(self, pattern):
        return sum(1 for line in self.lines() if re.search(pattern, line))

    def _wait(self, ready, description):
        """Polls until `ready()`, failing with the screen so the failure explains itself."""
        deadline = time.monotonic() + TIMEOUT
        while not ready():
            assert time.monotonic() < deadline, (
                f"timed out after {TIMEOUT}s waiting for {description}; screen:\n{self.screen()}"
            )
            time.sleep(0.1)
