#!/bin/sh
set -euo pipefail

println_gray() {
    echo -e "\e[90m$1\e[0m"
}

println_red() {
    echo -e "\e[91m$1\e[0m"
}

log_error() {
    println_red "[error] $1" >&2
}

log_debug() {
    if [ "${verbose:-0}" -ne 0 ]; then
        println_gray "[debug] $1" >&2
    fi
}

print_usage() {
    cat <<EOF
Usage: $(basename "$0") [options]

Options:
    -b <path>                Specify the installation path
    -c|--channel <channel>   Specify the release channel (stable or ci). Default is stable.
    -h, --help               Display this help message and exit
    -v, --verbose            Enable verbose output
EOF
}

validate_channel() {
    case "$1" in
        stable|ci)
            ;;
        *)
            log_error "invalid channel: $1; valid options are: stable, ci."
            exit 1
            ;;
    esac
}

select_artifact_base_name() {
    case "$(uname -s)" in
        Linux)
            local ostype=unknown-linux-gnu
            ;;
        Darwin)
            local ostype=apple-darwin
            ;;
        *)
            log_error "unsupported OS: $(uname -s)"
            exit 1
            ;;
    esac

    local arch=$(uname -m)

    echo "brush-$arch-$ostype"
}

get_artifact_hash_name() {
    local artifact_name=$1
    echo "${artifact_name%%.$artifact_ext}.$hash_type"
}

download_file() {
    local url=$1
    local dest=$2

    log_debug "Downloading file from ${url} to ${dest}..."
    curl --proto '=https' --tlsv1.2 -sSfL -o "${dest}" "${url}"
}

download_to_stdout() {
    local url=$1

    log_debug "Downloading ${url} to stdout..."
    curl --proto '=https' --tlsv1.2 -sSfL "${url}"
}

compute_default_dest_dir() {
    if [ -n "${XDG_BIN_HOME:-}" ]; then
        echo "$XDG_BIN_HOME"
    else
        echo "$HOME/.local/bin"
    fi
}

compute_base_download_uri() {
    local channel=$1

    case "$channel" in
        stable)
            compute_stable_base_download_uri
            ;;
        ci)
            compute_ci_base_download_uri
            ;;
        *)
            log_error "unsupported channel: $channel"
            exit 1
            ;;
    esac
}

compute_stable_base_download_uri() {
    release_tag=$(download_to_stdout "https://api.github.com/repos/$repo_name/releases/latest" 2>/dev/null | while read -r line; do
        case "$line" in
            '"tag_name":'*)
                line=${line##*\"tag_name\": \"}
                line=${line%%\",*}
                echo $line
                ;;
        esac
    done)

    if [ -z "$release_tag" ]; then
        log_error "failed to get latest release tag from GitHub"
        exit 1
    fi

    log_debug "Latest release tag: ${release_tag}"

    echo "https://github.com/$repo_name/releases/download/$release_tag"
}

compute_ci_base_download_uri() {
    artifact_base_name=$(select_artifact_base_name)

    local run_id=$(download_to_stdout "https://api.github.com/repos/$repo_name/actions/runs" 2>/dev/null | while read -r line; do
        line=${line## }
        case "$line" in
            '"id":'*)
                line=${line##*\"id\": }
                line=${line%%,*}
                run_id=$line
                ;;
            '"name":'*)
                [ "$line" = *CD* ] || run_id=
                ;;
            '"status":'*)
                [ "$line" = *completed* ] || run_id=
                ;;
            '"conclusion":'*)
                [ "$line" = *success* ] || run_id=
                ;;
            '"head_branch":'*)
                [ "$line" = *main* ] || run_id=
                ;;
            '"event":'*)
                [ "$line" = *push* ] || run_id=
                ;;
            *'}'*|*'},'*)
                [ -z "${run_id:-}" ] || (echo $run_id; break)
                run_id=
                ;;
        esac
    done)

    if [ -z "$run_id" ]; then
        log_error "failed to get latest successful CI run ID from GitHub"
        exit 1
    fi

    artifact_id=$(download_to_stdout "https://api.github.com/repos/$repo_name/actions/runs/${run_id}/artifacts" 2>/dev/null | while read -r line; do
        line=${line## }
        case "$line" in
            '"id":'*)
                line=${line##*\"id\": }
                line=${line%%,*}
                artifact_id=$line
                ;;
            '"name":'*)
                [ "$line" = "$artifact_base_name" ] || artifact_id=
                ;;
            *'}'*|*'},'*)
                [ -z "${artifact_id:-}" ] || (echo $artifact_id; break)
                artifact_id=
                ;;
        esac
    done)

    if [ -z "$artifact_id" ]; then
        log_error "failed to find artifact from latest successful CI run"
        exit 1
    fi

    # TODO
    echo "XYZZY: artifact-id=${artifact_id} not implemented"
}

# Constants
required_cmds="chmod curl pushd popd sha256sum tar uname"
repo_name="reubeno/brush"
artifact_ext=tar.gz
hash_type=sha256
bin_name=brush

# Init defaults
channel=stable
verbose=0
dest_dir=$(compute_default_dest_dir)

# Translate long options to short ones
for arg in "$@"; do
    case $arg in
        --help)
            set -- "$@" -h
            shift
            ;;
        --verbose)
            set -- "$@" -v
            shift
            ;;
        *)
            set -- "$@" "$arg"
            shift
            ;;
    esac
done

# Parse options
while getopts "b:c:hv" arg; do
    case ${arg} in
        b)
            dest_dir=$OPTARG
            ;;
        c)
            channel=$OPTARG
            ;;
        h)
            print_usage
            exit 0
            ;;
        v)
            [ $verbose -ne 0 ] && set -x
            verbose=1
            ;;
        \?)
            log_error "unknown option: -${arg}"
            print_usage
            exit 1
            ;;
    esac
done

# Validate args.
log_debug "Validating arguments..."
validate_channel "$channel"

# Validate prerequisites
log_debug "Validating prerequisites..."
for cmd in $required_cmds; do
    log_debug "Checking for command: ${cmd}"
    if ! command -v "${cmd}" >/dev/null 2>&1; then
        error "'${cmd}' is required but not installed. Please install it and try again."
        exit 1
    fi
done

# Find download URLs.
log_debug "Finding download URLs..."
base_uri=$(compute_base_download_uri $channel)
artifact_name="$(select_artifact_base_name).$artifact_ext"
artifact_uri="$base_uri/$artifact_name
artifact_hash_uri="$(get_artifact_hash_name "$artifact_uri")"
log_debug "Artifact URI: ${artifact_uri}"
log_debug "Artifact hash URI: ${artifact_hash_uri}"

# Download.
log_debug "Downloading artifacts..."
artifact_path=$(mktemp)
hash_path=$(mktemp)
download_file "$artifact_uri" "$artifact_path"
download_file "$artifact_hash_uri" "$hash_path"

# Validate and cleanup.
log_debug "Validating artifact..."
(
    cd "$(dirname "$artifact_path")"
    sha256sum --strict --check "$hash_path" >/dev/null 2>&1 || {
        log_error "artifact hash validation failed"
        exit 1
    }
)
rm -f "$hash_path"

# Update permissions on the artifact and move to destination.
chmod +x "$artifact_path"
dest=$dest_dir/$bin_name
log_debug "Installing brush to ${dest}..."
mkdir -p "$(dirname "$dest")"
mv "$artifact_path" "$dest"

log_debug "Done."
