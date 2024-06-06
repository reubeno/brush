FROM ubuntu:latest

ARG USERNAME=devel
ARG USER_UID=1000
ARG USER_GID=$USER_UID
    
# Install basic utilities and prerequisites.
#   bash-completion - for completion testing
#   bsdmainutils - provides hexdump, used by integration tests
#   neovim - for convenience and modern editing
RUN apt-get update -y && \
    apt-get install -y \
        bash \
        bash-completion \
        bsdmainutils \
        build-essential \
        curl \
        git \
        language-pack-en \
        neovim \
        sed \
        sudo \
        wget

# Install gh cli
# Reference: https://github.com/cli/cli/blob/trunk/docs/install_linux.md
RUN mkdir -p -m 755 /etc/apt/keyrings && \
    wget -qO- https://cli.github.com/packages/githubcli-archive-keyring.gpg | tee /etc/apt/keyrings/githubcli-archive-keyring.gpg > /dev/null && \
    chmod go+r /etc/apt/keyrings/githubcli-archive-keyring.gpg && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | sudo tee /etc/apt/sources.list.d/github-cli.list > /dev/null && \
    apt-get update -y && \
    apt-get install -y gh

# Add a non-root user that we'll do our best to use for development.
RUN userdel ubuntu && \
    groupadd --gid $USER_GID $USERNAME && \
    useradd --uid $USER_UID --gid $USER_GID -m $USERNAME && \
    echo "devel ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Switch to user.
USER devel

# Set up path to include rust components.
ENV PATH="${PATH}:/home/devel/.cargo/bin"

# Copy scripts to temp dir.
WORKDIR /tmp

# Install rust toolchain and cargo tools.
COPY install-rust-tools.sh .
RUN ./install-rust-tools.sh

# Switch back to home dir for normal usage.
WORKDIR /home/devel
