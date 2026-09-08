# The pytest half of the adapter contract (see ../README.md), shared because every adapter needs
# it identically: JUnit and a full log under /results, and the run's own exit status.
#
# Sourced by an entrypoint, which then calls `e2e_run_pytest <report name> [pytest args...]`.

e2e_run_pytest() {
    local report=$1
    shift
    mkdir -p /results/junit
    # For lib/e2e_skip.py, and for the suites that must consult the list before collection.
    E2E_SKIP_LIST=$(cat /e2e/skip-list.txt 2>/dev/null)
    export E2E_SKIP_LIST
    # Appended: an adapter may already have logged an earlier phase of its run (nvm).
    python3 -m pytest -q -p no:cacheprovider -p e2e_skip -o junit_logging=all \
        --log-file="/results/$report.log" --log-file-level=INFO --log-file-format='%(message)s' \
        --junitxml="/results/junit/$report.xml" "$@" /e2e/tests 2>&1 | tee -a /results/log.txt
    return "${PIPESTATUS[0]}"
}
