FROM ubuntu:latest

ARG DEFAULT_USER=devel

RUN apt-get update -y && \
    apt-get install -y \
        curl \
        git \
	language-pack-en \
        neovim \
        sed \
        sudo \
	wget \
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
