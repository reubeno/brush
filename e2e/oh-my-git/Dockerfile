# oh-my-git is two sourced scripts with no tests of their own, so this image clones them at a
# pinned commit and runs the small PTY-driven prompt suite in tests/. Upstream has no tags or
# releases; OMG_REF is master's head, unchanged since 2022.
FROM ubuntu:26.04
ARG OMG_REF=8804d8bd763be329543bbe58f0377b28beb7e111
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates git python3-pexpect python3-pytest \
    && rm -rf /var/lib/apt/lists/*
RUN git clone --filter=blob:none https://github.com/arialdomartini/oh-my-git /opt/oh-my-git \
    && git -C /opt/oh-my-git checkout -q "$OMG_REF"
COPY oh-my-git/entrypoint.sh oh-my-git/skip-list.txt /e2e/
COPY oh-my-git/tests /e2e/tests
COPY lib /e2e/lib
ENV LANG=C.UTF-8 PYTHONPATH=/e2e/lib
ENTRYPOINT ["/e2e/entrypoint.sh"]
