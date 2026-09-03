#!/bin/bash
# Run a containerized end-to-end suite of a real application against a shell.
# Usage: e2e/run.sh [--shell PATH|bash] [--results DIR] APP [test-runner args...]
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
shell=""; results=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --shell) shell=$2; shift 2 ;;
        --results) results=$2; shift 2 ;;
        --help|-h) sed -n 2,3p "$0"; exit 0 ;;
        *) break ;;
    esac
done
app=${1:?APP required}; shift
[[ -f $here/$app/Dockerfile ]] || { echo "unknown app: $app" >&2; exit 2; }
for p in release debug; do [[ -n $shell || ! -x $here/../target/$p/brush ]] || shell=$here/../target/$p/brush; done
[[ -n $shell ]] || { echo "no brush binary found; build first or pass --shell" >&2; exit 2; }
results=${results:-$here/../target/e2e/$app}
mkdir -p "$results"; results=$(cd "$results" && pwd)
docker=${DOCKER:-docker}
image=brush-e2e-$app
$docker build -q -t "$image" -f "$here/$app/Dockerfile" "$here"
mount=(); env=()
if [[ $shell != bash ]]; then
    shell=$(realpath "$shell")
    mount=(-v "$shell:/shell/$(basename "$shell"):ro")
    env=(-e "SHELL_UNDER_TEST=/shell/$(basename "$shell")")
fi
if [[ -n ${BLESH_TEST_TIMEOUT:-} ]]; then
    env+=(-e "BLESH_TEST_TIMEOUT=$BLESH_TEST_TIMEOUT")
fi
echo "==> $app against ${shell}; results in $results"
$docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -v "$results:/results" "${mount[@]}" "${env[@]}" "$image" "$@"
