FROM mcr.microsoft.com/onebranch/cbl-mariner/build:2.0

ARG DEFAULT_USER=devel

# Fail fast (on error)!
SHELL ["/bin/bash", "-euo", "pipefail", "-c"]

# Install dnf for a better package management experience, and then use it to install prerequisites:
#   - vim: for developer convenience (text editing)
#   - vim-extra: for developer convenience (syntax highlighting)
#   - zsh: for developer convenience
RUN tdnf install -qy dnf dnf-plugins-core && \
    dnf install -y \
        man \
        vim \
        vim-extra \
        zsh

# Add a non-root user that we'll do our best to use for development.
RUN useradd -m devel && \
    echo "devel ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Switch to user.
USER ${DEFAULT_USER}

# Set up path to include rust components.
ENV PATH="${PATH}:/home/${DEFAULT_USER}/.cargo/bin"
WORKDIR /tmp

# Copy scripts to temp dir.
WORKDIR /tmp

# Install rust and plugins
COPY install-rust-and-friends.sh .
RUN ./install-rust-and-friends.sh

# Install assorted development utilities
COPY install-dev-tools.sh .
RUN ./install-dev-tools.sh

# Bring modern conveniences to shell.
COPY make-shell-comfortable.sh .
RUN ./make-shell-comfortable.sh

# Switch back to home dir for normal usage.
WORKDIR /home/${DEFAULT_USER}
