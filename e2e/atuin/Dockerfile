# Environment for running atuin's bash shell-integration tests (see tests/). atuin itself is
# a pinned release binary; the test suite is our own, written to be upstreamable.
FROM ubuntu:24.04
ARG ATUIN_VERSION=18.21.0
RUN apt-get update && apt-get install -y --no-install-recommends bats tmux curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL "https://github.com/atuinsh/atuin/releases/download/v${ATUIN_VERSION}/atuin-x86_64-unknown-linux-gnu.tar.gz" \
    | tar -xz -C /usr/local/bin --strip-components=1 atuin-x86_64-unknown-linux-gnu/atuin
COPY atuin/entrypoint.sh atuin/skip-list.txt /e2e/
COPY atuin/tests /e2e/tests
ENV LANG=C.UTF-8
ENTRYPOINT ["/e2e/entrypoint.sh"]
