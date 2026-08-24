#!/usr/bin/env bash
# Simulated multi-target deployment script.
#
# A deliberately "typical" operations script: option parsing, directory
# juggling, declarations, formatted reporting. Deterministic by construction
# (no timestamps, no randomness) so two shells can be diffed byte-for-byte.
#
# Builtin-parse density comes from the per-artifact loop: every iteration
# re-parses getopts-style option handling plus declare/local/printf/test calls.

set -euo pipefail

TARGET=""
JOBS=4
VERBOSE=0
DRY_RUN=no
COMPRESS=gzip
TAG="latest"
VERSION=2

usage() {
    printf 'usage: %s [-v] [-n] [-j jobs] [-c compressor] [-t tag] target\n' "$0" >&2
}

die() {
    printf 'deploy: error: %s\n' "$1" >&2
    exit 2
}

[ $# -gt 0 ] || { usage; exit 2; }

while getopts ":vnj:c:t:h-" opt; do
    case "$opt" in
        v) VERBOSE=$((VERBOSE + 1)) ;;
        n) DRY_RUN=yes ;;
        j) JOBS=$OPTARG
           case "$JOBS" in (*[!0-9]*|'') die "-j expects a number" ;; esac ;;
        c) COMPRESS=$OPTARG ;;
        t) TAG=$OPTARG ;;
        h) usage; exit 0 ;;
        -) break ;;
        \?) die "invalid option: -$OPTARG" ;;
    esac
done
shift $((OPTIND - 1))

TARGET=${1:?missing target argument}
case "$TARGET" in
    staging|production|canary) ;;
    *) die "unknown target '$TARGET'" ;;
esac

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

mkdir "$WORK/pkg" "$WORK/out" "$WORK/cache"

pushd "$WORK" > /dev/null
trap 'popd > /dev/null; rm -rf "$WORK"' EXIT

ARTIFACTS=(
    "core:1.0.${VERSION}:stable"
    "cli:1.2.${VERSION}:stable"
    "daemon:0.9.${VERSION}:beta"
    "web:2.3.${VERSION}:beta"
    "tools:0.0.${VERSION}:experimental"
)

report_row() {
    local name=$1 version=$2 channel=$3 size=$4 status=$5
    printf '| %-8s | %-8s | %-12s | %6d KiB | %-8s |\n' \
        "$name" "$version" "$channel" "$size" "$status"
}

build_artifact() {
    local spec=$1
    local name=${spec%%:*}
    local rest=${spec#*:}
    local version=${rest%%:*}
    local channel=${rest##*:}

    # Local scope + string manipulation per artifact.
    local pkg_dir="$WORK/pkg/$name"
    mkdir -p "$pkg_dir"
    printf '%s %s (%s)\nbuilt for %s with %s\n' \
        "$name" "$version" "$channel" "$TARGET" "$COMPRESS" \
        > "$pkg_dir/README"

    local size=0
    while read -r line; do
        size=$((size + ${#line}))
    done < "$pkg_dir/README"

    if [ "$DRY_RUN" = yes ]; then
        status=skipped
    else
        tar -cf - -C "$pkg_dir" . 2>/dev/null | wc -c > /dev/null
        status=built
    fi

    report_row "$name" "$version" "$channel" "$size" "$status"
}

printf 'deploy plan for %s (jobs=%d, dry-run=%s, verbosity=%d)\n' \
    "$TARGET" "$JOBS" "$DRY_RUN" "$VERBOSE"
printf '+----------+----------+--------------+-----------+----------+\n'
printf '| name     | version  | channel      |      size | status   |\n'
printf '+----------+----------+--------------+-----------+----------+\n'

for spec in "${ARTIFACTS[@]}"; do
    build_artifact "$spec"
done

printf '+----------+----------+--------------+-----------+----------+\n'

# Directory bookkeeping round-trip.
for d in pkg out cache; do
    pushd "$d" > /dev/null
    pwd > /dev/null
    popd > /dev/null
done

# Export checks: environment visible to a fresh shell.
export DEPLOY_TARGET="$TARGET" DEPLOY_TAG="$TAG"
env | grep '^DEPLOY_' | LC_ALL=C sort
unset DEPLOY_TARGET DEPLOY_TAG

echo "done"
