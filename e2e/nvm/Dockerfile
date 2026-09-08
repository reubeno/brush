# Environment for nvm's upstream version-listing tests. The suite is pinned to
# nvm v0.40.7; its runner is pinned to the version used by nvm itself.
FROM ubuntu:24.04
ARG NVM_REF=f0b0c6bb0b281ceeb106c8cf9ab8fde141215092
# Bump this together with the urchin version in the URL below; it is the sha256 of the
# npm tarball, cross-checked against the registry's own sha512 integrity field.
ARG URCHIN_SHA256=caa836710230bf0b82caa54375cd5609c7083336910b83202156e2424b4139d0
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl git python3-pexpect python3-pytest \
    && rm -rf /var/lib/apt/lists/* \
    && curl -fsSL -o /tmp/urchin.tgz https://registry.npmjs.org/urchin/-/urchin-0.0.5.tgz \
    && echo "$URCHIN_SHA256  /tmp/urchin.tgz" | sha256sum -c - \
    && tar -xzf /tmp/urchin.tgz -C /usr/local/bin --strip-components=1 package/urchin \
    && rm /tmp/urchin.tgz \
    && chmod +x /usr/local/bin/urchin
COPY nvm/skip-list.txt /e2e/skip-list.txt
# urchin runs every executable file in the suite, so skipping means taking the execute bit off.
# It happens here, as root: the container runs as the host user and could not chmod these itself.
# `chmod -R a+rwX` then leaves them alone, since capital X only affects what is already executable.
RUN git clone --filter=blob:none https://github.com/nvm-sh/nvm /nvm \
    && git -C /nvm checkout -q "$NVM_REF" \
    && grep -v '^[[:space:]]*\(#\|$\)' /e2e/skip-list.txt | while IFS= read -r test; do \
           chmod a-x "/nvm/test/fast/Listing versions/$test"; \
       done \
    && chmod -R a+rwX /nvm
COPY nvm/entrypoint.sh /e2e/entrypoint.sh
COPY nvm/tests /e2e/tests
COPY lib /e2e/lib
ENV LANG=C.UTF-8 PYTHONPATH=/e2e/lib
ENTRYPOINT ["/e2e/entrypoint.sh"]
