# starship has no upstream shell-integration suite, so this image installs a pinned
# release and runs the small PTY-driven shell-integration suite in tests/.
FROM ubuntu:24.04
ARG STARSHIP_VERSION=1.26.0
# Bump these together with STARSHIP_VERSION; they are the release's own .tar.gz.sha256 entries.
ARG STARSHIP_SHA256_X86_64=b7c232b0e8249d8e55a40beb79c5c43a7d370f3f9408bd215deb0170daeaadf3
ARG STARSHIP_SHA256_AARCH64=dc30189378d2f2e287384e8a692d3f95ad1df64cf0e8c36aa9201516028aed6b
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl python3-pexpect python3-pytest \
    && rm -rf /var/lib/apt/lists/*
RUN case "$(uname -m)" in \
        x86_64) target=x86_64-unknown-linux-musl; sha=$STARSHIP_SHA256_X86_64 ;; \
        aarch64) target=aarch64-unknown-linux-musl; sha=$STARSHIP_SHA256_AARCH64 ;; \
        *) exit 1 ;; \
    esac \
    && curl -fsSL -o /tmp/starship.tar.gz \
       "https://github.com/starship/starship/releases/download/v${STARSHIP_VERSION}/starship-${target}.tar.gz" \
    && echo "$sha  /tmp/starship.tar.gz" | sha256sum -c - \
    && tar -xzf /tmp/starship.tar.gz -C /usr/local/bin starship \
    && rm /tmp/starship.tar.gz
COPY starship/entrypoint.sh starship/skip-list.txt /e2e/
COPY starship/tests /e2e/tests
COPY lib /e2e/lib
ENV LANG=C.UTF-8 PYTHONPATH=/e2e/lib
ENTRYPOINT ["/e2e/entrypoint.sh"]
