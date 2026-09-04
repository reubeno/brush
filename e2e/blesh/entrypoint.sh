#!/bin/bash
# Contract: $SHELL_UNDER_TEST names the shell binary; results go to /results
# (junit XML + full log); exit status = test status.
set -uo pipefail
# ble.sh --test runs under whatever shell invokes it, so no PATH shim is needed.
shell=${SHELL_UNDER_TEST:-bash}
timeout=${BLESH_TEST_TIMEOUT:-180}
rm -rf /results/progress
mkdir -p /results/junit /results/progress
: > /results/log.txt
case $timeout in
    ''|*[!0-9]*)
        echo "BLESH_TEST_TIMEOUT must be a non-negative integer number of seconds, got '$timeout'" | tee -a /results/log.txt >&2
        cat > /results/junit/blesh.xml <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="blesh" tests="1" failures="1">
  <testcase classname="configuration" name="BLESH_TEST_TIMEOUT">
    <failure message="invalid BLESH_TEST_TIMEOUT">BLESH_TEST_TIMEOUT must be a non-negative integer number of seconds</failure>
  </testcase>
</testsuite>
EOF
        exit 2
        ;;
esac
timeout=$((10#$timeout))
# Diagnostics for crashes and hangs: ble.sh keeps a per-run progress file (one "test TITLE" line as
# each test starts, then "pass"/"fail") in its runtime directory and deletes it on a clean exit. Put
# that directory under /results so the file survives a panic or our SIGKILL; each run's leftovers are
# copied to /results/progress/ below, where the last "test" line names the test that was running.
mkdir -p -m 700 /results/run
export XDG_RUNTIME_DIR=/results/run RUST_BACKTRACE=1
cd /blesh/out
# Upstream's `make check` is `bash out/ble.sh --test`, which sources every lib/test-*.sh in one
# process. We run the files one at a time, each under a timeout, so a hang or crash in one is
# reported as such and the rest still run. Arguments select files (default: all, minus skip-list.txt).
all=(bash main util canvas decode edit syntax complete keymap.vi)
if (($#)); then
    files=("$@")
else
    mapfile -t files < <(printf '%s\n' "${all[@]}" | grep -vxFf <(grep -v '^\s*\(#\|$\)' /e2e/skip-list.txt))
fi
log() { echo "$@" | tee -a /results/log.txt; }
for f in "${files[@]}"; do
    log "==> test-$f"
    rm -rf /results/run/blesh/*.test
    fifo=/results/run/test-$f.fifo
    rm -f "$fifo"
    mkfifo "$fifo"
    # Keep a second handle for the log copier so it can be reaped even when the shell
    # process has already exited but a child is still writing to stdout/stderr.
    tee -a /results/log.txt <"$fifo" &
    tee_pid=$!
    # setsid: own session, so the whole process tree can be listed and killed.
    setsid "$shell" ble.sh --test "$f" </dev/null >"$fifo" 2>&1 &
    pid=$!
    start=$SECONDS deadline=$((SECONDS + timeout))
    while { kill -0 "$pid" 2>/dev/null || kill -0 "$tee_pid" 2>/dev/null; } && ((SECONDS < deadline)); do sleep 1; done
    if { kill -0 "$pid" 2>/dev/null || kill -0 "$tee_pid" 2>/dev/null; }; then
        log "==> test-$f: timed out after ${timeout}s; process tree:"
        ps -s "$pid" -o pid,stat,etime,wchan:24,args --forest | tee -a /results/log.txt
        # SIGKILL, not TERM: ble.sh's exit trap would delete the progress file.
        pkill -KILL -s "$pid" 2>/dev/null
        kill -KILL "$tee_pid" 2>/dev/null || true
    fi
    wait "$pid" 2>/dev/null; exit=$?
    if kill -0 "$tee_pid" 2>/dev/null; then
        kill -TERM "$tee_pid" 2>/dev/null || true
        wait "$tee_pid" 2>/dev/null || true
    fi
    rm -f "$fifo"
    for p in /results/run/blesh/*.test/*; do
        case ${p##*/} in *[!0-9]*|'') continue ;; esac  # the section file is named by BASHPID; skip diff temp files
        mkdir -p "/results/progress/test-$f" && cp "$p" "/results/progress/test-$f/"
        log "==> test-$f: saved progress file $p (last started: $(grep '^test ' "$p" | tail -n1))"
    done
    log "==> test-$f ($((SECONDS - start))s) exit=$exit"
done
# JUnit: one testcase per upstream "[section]" summary line (its failure body is the diff output
# logged above it); a file that ends without any summary, or exits non-zero without a failed
# section, is reported as one crashed/timed-out testcase whose body is whatever it logged last.
sed 's/\x1b\[[0-9;]*[a-zA-Z]//g' /results/log.txt | gawk '
function esc(s) {
    gsub(/[\x00-\x08\x0B\x0C\x0E-\x1F]/, "", s)
    gsub(/&/, "\\&amp;", s); gsub(/</, "\\&lt;", s); gsub(/"/, "\\&quot;", s); return s
}
function testcase(name, failed, msg,   body) {
    total++
    body = "  <testcase classname=\"" esc(file) "\" name=\"" esc(name) "\""
    if (!failed) { cases = cases body "/>\n"; return }
    failures++; filefail = 1
    cases = cases body ">\n    <failure message=\"" esc(msg) "\">" esc(buf) "</failure>\n  </testcase>\n"
}
/^==> test-[^ ]+$/ { file = $2; buf = ""; seen = 0; filefail = 0; timedout = 0; next }
/^==> test-[^ ]+: timed out/ { timedout = 1 }
/^==> test-[^ ]+ \([0-9]+s\) exit=/ {
    sub(/.*exit=/, "")
    if (!seen || timedout || ($0 != 0 && ($0 != 1 || !filefail))) testcase(file, 1, (timedout ? "timed out" : "crashed") " (exit " $0 ")")
    next
}
/\[section\] / && match($0, /([0-9]+)\/([0-9]+) \(([0-9]+) fail, ([0-9]+) crash, ([0-9]+) skip\)$/, m) {
    name = $0; sub(/.*\[section\] /, "", name); sub(/: [0-9]+\/[0-9]+ \(.*/, "", name)
    testcase(name, m[1] != m[2], m[3] " fail, " m[4] " crash")
    seen = 1; buf = ""; next
}
{ buf = buf $0 "\n" }
END {
    print "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"blesh\" tests=\"" total+0 "\" failures=\"" failures+0 "\">"
    printf "%s", cases
    print "</testsuite>"
    exit failures > 0
}' > /results/junit/blesh.xml
