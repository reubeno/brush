# Environment for bash-completion's own test suite (test/t, pytest). The base is upstream's own
# test image, which has the commands under test installed; it is deliberately unpinned, to track
# upstream, which this adapter's `report-only` policy tolerates. The suite is built as upstream's
# test/docker/entrypoint.sh does, since some completions (aliases such as scp, python3) exist only
# after `make`.
FROM ghcr.io/scop/bash-completion/test:fedoradev
ARG BASH_COMPLETION_REF=2.18.0
RUN git clone -q --depth 1 --branch "$BASH_COMPLETION_REF" \
        https://github.com/scop/bash-completion.git /bash-completion \
    && cd /bash-completion && autoreconf -i && ./configure -q && make -s -j"$(nproc)" \
    && chmod -R a+rwX /bash-completion \
    && chmod a+w /etc/passwd /etc/group
COPY bash-completion/entrypoint.sh /e2e/
COPY lib /e2e/lib
ENV PYTHONPATH=/e2e/lib
# Some completions (xrandr, say) need an X display.
ENTRYPOINT ["xvfb-run", "-a", "/e2e/entrypoint.sh"]
