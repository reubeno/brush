# Environment for running atuin's bash shell-integration tests (see tests/). atuin itself is
# a pinned release binary; the test suite is our own, written to be upstreamable.
FROM ubuntu:24.04
ARG ATUIN_VERSION=18.21.0
# Bump these together with ATUIN_VERSION; they are the release's own .tar.gz.sha256 entries.
ARG ATUIN_SHA256_X86_64=434d82b4c5f691b6f0423f30a4b931df4afded19b05a795a280192fdf8a61a2d
ARG ATUIN_SHA256_AARCH64=dbdee27f7fc67b92e770d1af89d3b421051499a058cd3e4698d123e0117fa950
RUN apt-get update && apt-get install -y --no-install-recommends \
        tmux curl ca-certificates python3-pytest python3-libtmux \
    && rm -rf /var/lib/apt/lists/*
RUN case "$(uname -m)" in \
        x86_64) sha=$ATUIN_SHA256_X86_64 ;; \
        aarch64) sha=$ATUIN_SHA256_AARCH64 ;; \
        *) exit 1 ;; \
    esac \
    && target="$(uname -m)-unknown-linux-gnu" \
    && curl -fsSL -o /tmp/atuin.tar.gz \
       "https://github.com/atuinsh/atuin/releases/download/v${ATUIN_VERSION}/atuin-${target}.tar.gz" \
    && echo "$sha  /tmp/atuin.tar.gz" | sha256sum -c - \
    && tar -xzf /tmp/atuin.tar.gz -C /usr/local/bin --strip-components=1 "atuin-${target}/atuin" \
    && rm /tmp/atuin.tar.gz
COPY atuin/entrypoint.sh atuin/skip-list.txt /e2e/
COPY atuin/tests /e2e/tests
COPY lib /e2e/lib
ENV LANG=C.UTF-8 PYTHONPATH=/e2e/lib
ENTRYPOINT ["/e2e/entrypoint.sh"]
