#!/bin/bash
set -euo pipefail

gh_cli_version="2.38.0"

gh_arch="$(arch)"
if [[ "${gh_arch}" == "x86_64" ]]; then
    gh_arch="amd64"
fi

gh_cli_uri="https://github.com/cli/cli/releases/download/v${gh_cli_version}/gh_${gh_cli_version}_linux_${gh_arch}.tar.gz"

wget -qO- "${gh_cli_uri}" | sudo tar xz --strip-components=1 -C /usr/bin
