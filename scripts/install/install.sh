#!/bin/sh
#
# Installs brush from official GitHub releases of reubeno/brush.
#
# Usage:
#   curl --proto '=https' --tlsv1.2 -fsSL https://brush.sh/install.sh | sh
#
# To pass options, use `sh -s --`, e.g.:
#   curl ... | sh -s -- --version 0.4.0 --require-attestation
#
# Options:
#   --version <version>     Version to install (e.g. "0.4.0"); defaults to the latest release.
#   --dir <dir>             Directory to install into; defaults to $XDG_BIN_HOME, or ~/.local/bin.
#   --require-attestation   Fail if the build attestation can't be verified.
#
# The downloaded archive is always checked against its published SHA-256 checksum.
# If the GitHub CLI (gh) is installed and authenticated, the archive's build
# provenance attestation is also verified, confirming it was built by the official
# repository's release workflow for that release's tag.
#
# This script sticks to POSIX sh, so it runs under dash, busybox ash, bash, and zsh.
#

set -eu

REPO="reubeno/brush"
RELEASE_WORKFLOW="${REPO}/.github/workflows/cd.yaml"

say() {
    echo "brush-install: $*" >&2
}

die() {
    say "error: $*"
    exit 1
}

# curl with the settings every request here wants: https only (even across
# redirects), TLS 1.2 or newer, fail on HTTP errors, and retry transient failures.
fetch() {
    curl --proto '=https' --proto-redir '=https' --tlsv1.2 -fsSL --retry 3 "$@"
}

download() {
    url="$1"
    destination="$2"
    fetch -o "${destination}" "${url}" || die "failed to download ${url}"
}

# macOS ships shasum rather than sha256sum.
if ! command -v sha256sum >/dev/null; then
    sha256sum() {
        shasum -a 256 "$@"
    }
fi

# Removes whatever this script has left on disk. Runs when the script exits for
# any reason, including errors and Ctrl-C.
cleanup() {
    rm -rf "${tmp_dir}"
    if [ -n "${staged_binary}" ]; then
        rm -f "${staged_binary}"
    fi
}

parse_args() {
    version=""
    install_dir=""
    require_attestation=""

    while [ $# -gt 0 ]; do
        case "$1" in
            --version)
                [ -n "${2:-}" ] || die "--version requires a value"
                version="$2"
                shift
                ;;
            --dir)
                [ -n "${2:-}" ] || die "--dir requires a value"
                install_dir="$2"
                shift
                ;;
            --require-attestation)
                require_attestation=1
                ;;
            *)
                die "unknown option: $1 (options: --version <version>, --dir <dir>, --require-attestation)"
                ;;
        esac
        shift
    done

    # ~/.local/bin is where the XDG base directory spec puts user executables. The spec
    # has no variable for it, but XDG_BIN_HOME is a common extension (uv, among others).
    if [ -z "${install_dir}" ] && [ -n "${XDG_BIN_HOME:-}" ]; then
        install_dir="${XDG_BIN_HOME}"
    elif [ -z "${install_dir}" ]; then
        [ -n "${HOME:-}" ] || die "HOME is not set; use --dir <dir>"
        install_dir="${HOME}/.local/bin"
    fi
}

# Sets `target` to the name of the release build for this machine.
detect_target() {
    os="$(uname -s)"
    arch="$(uname -m)"

    case "${arch}" in
        x86_64 | amd64)
            arch="x86_64"
            ;;
        aarch64 | arm64)
            arch="aarch64"
            ;;
        *)
            die "unsupported architecture: ${arch}"
            ;;
    esac

    case "${os}" in
        Darwin)
            target="${arch}-apple-darwin"
            ;;
        Linux)
            # Prefer the glibc build, which needs glibc 2.34 or newer. Whenever that
            # can't be confirmed (no getconf, a non-glibc libc, a sort without -V),
            # fall back to the static musl build, which runs on any Linux.
            glibc_version="$(getconf GNU_LIBC_VERSION 2>/dev/null | cut -d' ' -f2)"
            # `sort -C` succeeds if its input is already in order; with -V that's a
            # version comparison, so this asks whether 2.34 <= glibc_version.
            if printf '2.34\n%s\n' "${glibc_version}" | sort -CV 2>/dev/null; then
                target="${arch}-unknown-linux-gnu"
            else
                target="${arch}-unknown-linux-musl"
            fi
            ;;
        *)
            die "unsupported OS: ${os}"
            ;;
    esac
}

# Sets `tag` to the git tag of the release to install.
resolve_release_tag() {
    if [ -n "${version}" ]; then
        tag="brush-shell-v${version#v}"
        return
    fi

    # Follow the "latest" redirect once, up front, so that every download and
    # check below refers to the same release.
    latest_url="$(fetch -I -o /dev/null -w '%{url_effective}' "https://github.com/${REPO}/releases/latest")" ||
        die "could not determine latest release"
    tag="${latest_url##*/}"
    case "${tag}" in
        brush-shell-v*) ;;
        *) die "could not determine latest release" ;;
    esac
}

# Downloads the release archive into `tmp_dir` and checks it against its
# published SHA-256 checksum.
download_archive() {
    archive="brush-${target}.tar.gz"
    release_url="https://github.com/${REPO}/releases/download/${tag}"

    say "downloading ${release_url}/${archive}"
    download "${release_url}/${archive}" "${tmp_dir}/${archive}"
    download "${release_url}/brush-${target}.sha256" "${tmp_dir}/${archive}.sha256"

    (cd "${tmp_dir}" && sha256sum -c "${archive}.sha256" >/dev/null) || die "checksum mismatch for ${archive}"
    say "verified SHA-256 checksum"
}

# Verifies the archive's build provenance attestation, when that's possible here.
# The attestation must come from the official release workflow, running on
# GitHub-hosted runners, for this release's tag.
verify_attestation() {
    skip_reason=""

    if ! command -v gh >/dev/null; then
        skip_reason="GitHub CLI (gh) is not installed"
    elif ! gh attestation verify --help 2>/dev/null | grep -q -- --source-ref; then
        skip_reason="GitHub CLI (gh) is too old (2.68 or newer is needed)"
    else
        gh_status=0
        gh attestation verify "${tmp_dir}/${archive}" \
            --repo "${REPO}" \
            --signer-workflow "${RELEASE_WORKFLOW}" \
            --source-ref "refs/tags/${tag}" \
            --deny-self-hosted-runners \
            >/dev/null 2>"${tmp_dir}/gh.err" || gh_status=$?

        if [ "${gh_status}" -eq 4 ]; then
            # gh exits with 4 when it has no credentials; anything else is a real failure.
            skip_reason="GitHub CLI (gh) is not authenticated"
        elif [ "${gh_status}" -ne 0 ]; then
            cat "${tmp_dir}/gh.err" >&2
            die "attestation verification failed for ${archive}"
        fi
    fi

    if [ -z "${skip_reason}" ]; then
        say "verified GitHub build attestation"
    elif [ -n "${require_attestation}" ]; then
        die "cannot verify build attestation: ${skip_reason}"
    else
        say "note: ${skip_reason}, so the build attestation wasn't checked; an authenticated gh (2.68+) would also verify this build came from the official release workflow"
    fi
}

# Extracts the binary and moves it into place.
install_binary() {
    tar -xzf "${tmp_dir}/${archive}" -C "${tmp_dir}" brush || die "failed to extract ${archive}"

    # Stage the binary next to its destination so that the final step is a rename.
    # A rename is atomic, so an interrupted install never leaves a partial binary
    # behind, and it works even while an older brush at that path is running
    # (overwriting a running executable in place fails on Linux). Checking that the
    # staged binary runs first means a build that can't run here never replaces a
    # working one.
    mkdir -p "${install_dir}" || die "cannot write to ${install_dir}"
    staged_binary="$(mktemp "${install_dir}/.brush.tmp.XXXXXX")" || die "cannot write to ${install_dir}"
    cp "${tmp_dir}/brush" "${staged_binary}" || die "cannot write to ${install_dir}"
    chmod 755 "${staged_binary}"

    installed_version="$("${staged_binary}" --version)" ||
        die "the ${target} build failed to run on this system; see https://brush.sh/getting-started/install/ for other ways to install"

    mv -f "${staged_binary}" "${install_dir}/brush" || die "cannot write to ${install_dir}"
    say "installed ${installed_version} to ${install_dir}/brush"
}

# Lets the user know if running `brush` won't run what was just installed.
check_path() {
    # Nothing above has run `brush` by name, so this is a fresh PATH search.
    found="$(command -v brush)" || true

    # shellcheck disable=SC3013 # -ef isn't POSIX, but every sh we support has it.
    if [ -z "${found}" ]; then
        say "note: ${install_dir} is not in your PATH"
    elif [ ! "${found}" -ef "${install_dir}/brush" ]; then
        say "note: running 'brush' will run ${found}, not ${install_dir}/brush"
    fi
}

main() {
    parse_args "$@"
    detect_target
    resolve_release_tag

    staged_binary=""
    tmp_dir="$(mktemp -d)"
    trap cleanup EXIT
    # Some shells (dash, busybox ash) skip the EXIT trap when killed by a signal;
    # exiting from the signal handler makes sure cleanup runs everywhere. The exit
    # codes are the usual 128 + signal number.
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM

    download_archive
    verify_attestation
    install_binary
    check_path
}

# Wrapped in a function so that a partially downloaded script never runs.
main "$@"
