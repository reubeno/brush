# zoxide's upstream tests only validate generated scripts, so this image installs a
# pinned release and runs the small PTY-driven shell-integration suite in tests/.
FROM ubuntu:24.04
ARG ZOXIDE_VERSION=0.10.0
# Bump these together with ZOXIDE_VERSION; upstream publishes no checksum file, so they
# are the sha256 of the release tarballs themselves.
ARG ZOXIDE_SHA256_X86_64=2d93385b99f3e82cf2701609a1bffcad863fbeb75aa3fe7eb6be4d29be68b1ae
ARG ZOXIDE_SHA256_AARCH64=f1f16c5d6298d63dee467eedea1cdcd8490e43e493bea43acd416dc9033ef641
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl python3-pexpect python3-pytest \
    && rm -rf /var/lib/apt/lists/*
RUN case "$(uname -m)" in \
        x86_64) target=x86_64-unknown-linux-musl; sha=$ZOXIDE_SHA256_X86_64 ;; \
        aarch64) target=aarch64-unknown-linux-musl; sha=$ZOXIDE_SHA256_AARCH64 ;; \
        *) exit 1 ;; \
    esac \
    && curl -fsSL -o /tmp/zoxide.tar.gz \
       "https://github.com/ajeetdsouza/zoxide/releases/download/v${ZOXIDE_VERSION}/zoxide-${ZOXIDE_VERSION}-${target}.tar.gz" \
    && echo "$sha  /tmp/zoxide.tar.gz" | sha256sum -c - \
    && tar -xzf /tmp/zoxide.tar.gz -C /usr/local/bin zoxide \
    && rm /tmp/zoxide.tar.gz
COPY zoxide/entrypoint.sh zoxide/skip-list.txt /e2e/
COPY zoxide/tests /e2e/tests
COPY lib /e2e/lib
ENV LANG=C.UTF-8 PYTHONPATH=/e2e/lib
ENTRYPOINT ["/e2e/entrypoint.sh"]
