#!/bin/bash
# Contract: $SHELL_UNDER_TEST names the shell binary; results go to /results
# (junit XML + full log); exit status = test status.
set -uo pipefail
source /e2e/lib/run_pytest.sh
shell="${SHELL_UNDER_TEST:-bash}"
export NVM_TEST_SHELL="$shell"
mkdir -p /results/junit
suite="/nvm/test/fast/Listing versions"
urchin -f -s "$shell" "$suite" 2>&1 | tee /results/log.txt
upstream_status=${PIPESTATUS[0]}

# urchin has no JUnit reporter, but it does write one "<test> passed" / "<test> failed" line per
# test to .urchin.log in the suite directory. That is ANSI-free and unambiguous, so synthesize the
# report from it rather than from the decorated stdout above; a single testcase for the whole
# suite would hide which upstream test regressed.
python3 - "$suite/.urchin.log" /results/junit/nvm-upstream.xml <<'PY'
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

log, out = (Path(arg) for arg in sys.argv[1:])
suite = ET.Element("testsuite", name="nvm-upstream")
# A missing log means urchin died before running anything. The empty report that leaves behind is
# what reports it: a JUnit file with no cases in it cannot establish a passing run.
for line in log.read_text(errors="replace").splitlines() if log.exists() else []:
    name, _, status = line.rpartition(" ")
    if status not in ("passed", "failed"):
        continue
    case = ET.SubElement(suite, "testcase", classname="nvm-upstream", name=name)
    if status == "failed":
        ET.SubElement(case, "failure", message="see log.txt for this test's output")
suite.set("tests", str(len(suite)))
suite.set("failures", str(len(suite.findall("testcase/failure"))))
ET.ElementTree(suite).write(out, encoding="utf-8", xml_declaration=True)
PY

# urchin's own exit status still decides: a crash before any test would leave an empty report.
(( upstream_status == 0 )) || echo "==> urchin exited $upstream_status" >> /results/log.txt

e2e_run_pytest nvm-interactive "$@"
interactive_status=$?
exit "$((upstream_status || interactive_status))"
