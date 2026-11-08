#!/usr/bin/env python3
"""Small local differential spike suite. No network or external writes."""
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parent.parent
brush = root / 'target/debug/brush'
cases = [
    ('builtin-builtin', 'echo hello | { read x; echo "<$x>"; }'),
    ('builtin-external', 'echo hello | /bin/cat'),
    ('external-builtin', '/bin/echo hello | { read x; echo "<$x>"; }'),
    ('external-external', '/bin/echo hello | /bin/cat'),
    ('function-late-exec', 'f() { /bin/cat; }; echo hello | f'),
    ('eval-late-exec', "echo hello | eval '/bin/cat'"),
    ('exec-late', 'echo hello | { exec /bin/cat; }'),
    ('fd-dup', 'echo hello | { /bin/cat <&0; }'),
    ('dev-fd', 'echo hello | /bin/cat /dev/fd/0'),
    ('three-stage', 'echo hello | /bin/cat | /bin/cat'),
    ('backpressure', '/usr/bin/seq 1 100000 | /bin/cat | /usr/bin/wc -l'),
    ('brokenpipe', '/usr/bin/yes | /usr/bin/head -c 1000'),
    ('lastpipe', 'shopt -s lastpipe; echo hello | read x; echo "$x"'),
    ('pipestatus', 'false | true; echo "${PIPESTATUS[@]}"'),
    ('pipefail', 'set -o pipefail; false | true; echo $?'),
    ('command-substitution', 'x=$(echo hello | /bin/cat); echo "$x"'),
    ('process-substitution', '/bin/cat <(echo hello)'),
    ('heredoc', '/bin/cat <<EOF\nhello\nEOF'),
    ('binary', "printf 'a\\0b\\377' | /bin/cat | /usr/bin/od -An -t x1"),
]
failures = []
for name, script in cases:
    try:
        expected = subprocess.run(['/bin/bash', '--noprofile', '--norc', '-c', script], capture_output=True, timeout=15)
        actual = subprocess.run([str(brush), '--noprofile', '--norc', '-c', script], capture_output=True, timeout=15)
        if (actual.returncode, actual.stdout) != (expected.returncode, expected.stdout):
            raise AssertionError(f'expected {(expected.returncode, expected.stdout)!r}; got {(actual.returncode, actual.stdout)!r}; stderr={actual.stderr!r}')
        print(f'PASS {name}')
    except Exception as error:
        failures.append(name)
        print(f'FAIL {name}: {error}')
print(f'{len(cases)-len(failures)} passed; {len(failures)} failed')
raise SystemExit(bool(failures))
