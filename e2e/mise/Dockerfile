# Environment for running mise's own bash activation tests. Only tests that need no tool
# downloads are run. MISE_REF must stay the commit that tag v$MISE_VERSION points at, so the
# tests and the binary under them come from one release; check with
# `git ls-remote --tags https://github.com/jdx/mise 'v<version>^{}'` when bumping.
FROM ubuntu:24.04
ARG MISE_VERSION=2026.9.1
ARG MISE_REF=8fe6385de7f73908ab5a6c9789f477a322eda3a5
# Bump these together with MISE_VERSION; they are the release's own SHASUMS256.txt entries.
ARG MISE_SHA256_X64=c98423c8470d6dc416d9f7036d0646d8ef5ae92ad9186907f8fcc84cbe7db4ea
ARG MISE_SHA256_ARM64=0ef0a778eaa8599f3e90a8a0979c9fc3f79922cafb5fa6d39f366d974da33bba
RUN apt-get update && apt-get install -y --no-install-recommends \
        curl git ca-certificates python3-pytest \
    && rm -rf /var/lib/apt/lists/*
RUN git clone --filter=blob:none https://github.com/jdx/mise /mise \
    && git -C /mise checkout -q "$MISE_REF" && chmod -R a+rwX /mise
RUN case "$(uname -m)" in \
        x86_64) arch=x64; sha=$MISE_SHA256_X64 ;; \
        aarch64) arch=arm64; sha=$MISE_SHA256_ARM64 ;; \
        *) exit 1 ;; \
    esac \
    && curl -fsSL -o /tmp/mise \
       "https://github.com/jdx/mise/releases/download/v${MISE_VERSION}/mise-v${MISE_VERSION}-linux-${arch}" \
    && echo "$sha  /tmp/mise" | sha256sum -c - \
    && install -m 755 /tmp/mise /usr/local/bin/mise \
    && rm /tmp/mise
COPY shim /e2e/bin
COPY mise/entrypoint.sh mise/skip-list.txt /e2e/
COPY mise/tests /e2e/tests
COPY lib /e2e/lib
ENV LANG=C.UTF-8 PYTHONPATH=/e2e/lib
ENTRYPOINT ["/e2e/entrypoint.sh"]
