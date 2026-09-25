#!/bin/sh
#
# Installs brush from official GitHub releases of reubeno/brush, or (with --canary) from the
# canary builds that CI publishes for every push to its main branch.
#
# Usage:
#   curl --proto '=https' --tlsv1.2 -fsSL https://brush.sh/install.sh | sh
#
# To pass options, use `sh -s --`, e.g.:
#   curl ... | sh -s -- --version 0.4.0 --require-attestation
#
# Options:
#   --version <version>     Version to install (e.g. "0.4.0"); defaults to the latest release.
#   --canary                Install the newest canary build of main instead of a release. Canary
#                           builds are unreleased and may break.
#   --commit <sha>          Install the canary build of this commit of main (a full 40-character
#                           hash); builds of the last ~100 pushes are kept.
#   --dir <dir>             Directory to install into; defaults to $XDG_BIN_HOME, or ~/.local/bin.
#   --require-attestation   Fail if the build attestation can't be verified.
#
# The downloaded archive is always checked against its published SHA-256 checksum.
# If the GitHub CLI (gh) is installed and authenticated, the archive's build
# provenance attestation is also verified, confirming it was built by the official
# repository's release workflow for that release's tag (or, for canary builds, for
# its main branch).
#
# This script sticks to POSIX sh, so it runs under dash, busybox ash, bash, and zsh.
#

set -eu

REPO="reubeno/brush"
RELEASE_WORKFLOW="${REPO}/.github/workflows/cd.yaml"
# Where the release workflow publishes canary builds, on ghcr.io.
CANARY_PACKAGE="reubeno/brush-canary"

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
    canary=""
    commit=""
    install_dir=""
    require_attestation=""

    while [ $# -gt 0 ]; do
        case "$1" in
            --version)
                [ -n "${2:-}" ] || die "--version requires a value"
                version="$2"
                shift
                ;;
            --canary)
                canary=1
                ;;
            --commit)
                [ -n "${2:-}" ] || die "--commit requires a value"
                commit="$2"
                canary=1
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
                die "unknown option: $1 (options: --version <version>, --canary, --commit <sha>, --dir <dir>, --require-attestation)"
                ;;
        esac
        shift
    done

    if [ -n "${canary}" ] && [ -n "${version}" ]; then
        die "--version can't be combined with --canary or --commit"
    fi
    # The hash becomes part of a URL, so accept nothing but a full, lowercase commit hash.
    if [ -n "${commit}" ]; then
        case "${commit}" in
            *[!0-9a-f]*) commit="" ;; # fails the length check below
        esac
        [ "${#commit}" -eq 40 ] || die "--commit needs a full 40-character commit hash (see \`git rev-parse\`)"
    fi

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

# Sets `tag` to the git tag of the release to install, and `source_ref` to its full ref.
resolve_release_tag() {
    if [ -n "${version}" ]; then
        tag="brush-shell-v${version#v}"
        source_ref="refs/tags/${tag}"
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
    source_ref="refs/tags/${tag}"
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

# Downloads the newest canary build (or the one for `commit`) into `tmp_dir`, and checks it
# against its digest in the registry, which is the SHA-256 of its content.
download_canary_archive() {
    archive="brush-${target}.tar.gz"
    image_tag="${target}${commit:+-${commit}}"
    registry="https://ghcr.io/v2/${CANARY_PACKAGE}"
    source_ref="refs/heads/main"

    # Even public packages need a token to download, but an anonymous one does. ghcr.io refuses
    # one for a package that doesn't exist or isn't public.
    token="$(fetch "https://ghcr.io/token?scope=repository:${CANARY_PACKAGE}:pull" |
        sed -n 's/.*"token":"\([^"]*\)".*/\1/p')"
    [ -n "${token}" ] || die "could not access canary builds at ghcr.io/${CANARY_PACKAGE}"

    manifest="$(fetch -H "Authorization: Bearer ${token}" \
        -H "Accept: application/vnd.oci.image.manifest.v1+json" \
        "${registry}/manifests/${image_tag}")" ||
        die "no canary build of brush found for ${target}${commit:+ at commit ${commit}}"

    # The archive is the manifest's only layer.
    digest="$(printf '%s' "${manifest}" | tr -d ' \t\r\n' |
        sed -n 's/.*"layers":\[{[^}]*"digest":"sha256:\([0-9a-f]\{64\}\)".*/\1/p')"
    [ -n "${digest}" ] || die "unexpected manifest for ghcr.io/${CANARY_PACKAGE}:${image_tag}"

    say "downloading ghcr.io/${CANARY_PACKAGE}:${image_tag} (${archive})"
    fetch -H "Authorization: Bearer ${token}" -o "${tmp_dir}/${archive}" \
        "${registry}/blobs/sha256:${digest}" || die "failed to download ${archive}"

    echo "${digest}  ${archive}" >"${tmp_dir}/${archive}.sha256"
    (cd "${tmp_dir}" && sha256sum -c "${archive}.sha256" >/dev/null) || die "checksum mismatch for ${archive}"
    say "verified SHA-256 checksum"
}

# Verifies the archive's build provenance attestation, when that's possible here.
# The attestation must come from the official release workflow, running on
# GitHub-hosted runners, for this release's tag (or, for canary builds, main).
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
            --source-ref "${source_ref}" \
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
    if [ -z "${canary}" ]; then
        resolve_release_tag
    fi

    staged_binary=""
    tmp_dir="$(mktemp -d)"
    trap cleanup EXIT
    # Some shells (dash, busybox ash) skip the EXIT trap when killed by a signal;
    # exiting from the signal handler makes sure cleanup runs everywhere. The exit
    # codes are the usual 128 + signal number.
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM

    if [ -n "${canary}" ]; then
        download_canary_archive
    else
        download_archive
    fi
    verify_attestation
    install_binary
    check_path
}

# Wrapped in a function so that a partially downloaded script never runs.
main "$@"
