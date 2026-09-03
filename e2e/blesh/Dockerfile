# Environment for running ble.sh's own test suite (`ble.sh --test`, which is what upstream's
# `make check` runs; see https://github.com/akinomyoga/ble.sh/blob/master/.github/workflows/test.yml,
# BSD-3-Clause). Build needs git, GNU make and gawk (per upstream README). ble.sh is cloned and
# built at image build time, pinned below. less: the pager test expects it; procps: ps for hang diagnostics.
FROM ubuntu:24.04
ARG BLESH_REF=63c23e99f1f7133ae57b79f87b16d1f68cd39884
RUN apt-get update && apt-get install -y --no-install-recommends git make gawk less procps ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN git clone --filter=blob:none https://github.com/akinomyoga/ble.sh /blesh \
    && git -C /blesh checkout -q "$BLESH_REF" && make -C /blesh && chmod -R a+rwX /blesh
COPY blesh/entrypoint.sh blesh/skip-list.txt /e2e/
ENV LANG=C.UTF-8
ENTRYPOINT ["/e2e/entrypoint.sh"]
