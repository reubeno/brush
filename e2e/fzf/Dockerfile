# Environment for running fzf's bash shell-integration tests. Package list derived from
# fzf's Dockerfile (https://github.com/junegunn/fzf/blob/master/Dockerfile, MIT license),
# trimmed to what the bash tests need. fzf itself is cloned at build time, pinned below.
FROM rubylang/ruby:3.4.1-noble
ARG FZF_REF=52f4319a72c17e123396cc3a2e6abf2e96e9d753
RUN apt-get update && apt-get install -y --no-install-recommends git make golang tmux ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY fzf/Gemfile /e2e/Gemfile
RUN BUNDLE_GEMFILE=/e2e/Gemfile bundle install
RUN git clone --filter=blob:none https://github.com/junegunn/fzf /fzf \
    && git -C /fzf checkout -q "$FZF_REF" && cd /fzf && make install && chmod -R a+rwX /fzf
# Do not set default PS1 (same as upstream)
RUN rm -f /etc/bash.bashrc
COPY shim /e2e/bin
COPY fzf/entrypoint.sh fzf/skip-list.txt /e2e/
ENV LANG=C.UTF-8 PATH=/e2e/bin:$PATH BUNDLE_GEMFILE=/e2e/Gemfile
ENTRYPOINT ["/e2e/entrypoint.sh"]
