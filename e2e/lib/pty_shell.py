import logging
import os
import time

import pexpect


CURSOR_POSITION_QUERY = "\x1b[6n"
COMMAND_DONE = "__BRUSH_E2E_COMMAND_DONE__"
# Seconds to wait for any one expected string. The overall wait is bounded separately, since a
# shell that keeps asking where the cursor is would otherwise refresh this on every reply.
TIMEOUT = int(os.environ.get("E2E_TEST_TIMEOUT", "10"))
LOG = logging.getLogger(__name__)


def spawn(command, args, env, prompt):
    child = pexpect.spawn(
        command,
        args,
        dimensions=(30, 100),
        encoding="utf-8",
        env=env | {"BRUSH_E2E_COMMAND_DONE": COMMAND_DONE},
        timeout=TIMEOUT,
    )
    LOG.info("shell startup:\n%s", expect_output(child, prompt))
    return child


def expect_output(shell, expected):
    output = []
    deadline = time.monotonic() + TIMEOUT
    while True:
        try:
            matched = shell.expect_exact([expected, CURSOR_POSITION_QUERY])
        except pexpect.exceptions.ExceptionPexpect:
            output.append(shell.before)
            LOG.exception("PTY error waiting for %r:\n%s", expected, "".join(output))
            raise
        output.append(shell.before)
        if matched == 0:
            return "".join(output)
        if time.monotonic() >= deadline:
            LOG.error("PTY gave up waiting for %r:\n%s", expected, "".join(output))
            raise pexpect.exceptions.TIMEOUT(
                f"still answering cursor-position queries after {TIMEOUT}s waiting for {expected!r}"
            )
        # Claim the cursor is at the top left. Some prompts (starship) query it at every redraw
        # and wait for the reply before printing anything.
        shell.send("\x1b[1;1R")


def run(shell, prompt, command):
    shell.sendline(
        f"{command}; __brush_e2e_status=$?; "
        'printf "\\n%s\\n" "$BRUSH_E2E_COMMAND_DONE"; '
        '(exit "$__brush_e2e_status")'
    )
    output = expect_output(shell, COMMAND_DONE)
    output += expect_output(shell, prompt)
    LOG.info("$ %s\n%s", command, output)
    return output
