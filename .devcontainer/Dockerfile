# Use Ubuntu 20.04 with .NET 6.0 installed
FROM mcr.microsoft.com/vscode/devcontainers/rust:bullseye

# Make sure pipe failures trigger build failures
SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# Install native components
WORKDIR /tmp/work
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        curl \
        dos2unix \
        git \
        gnupg2 \
        sudo \
        zsh \
    && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/* && \
    \
    rm -rf /tmp/work

# Switch to non-root user
USER vscode

# Download external components
# Validate components against known hashes
# Clone additional oh-my-zsh plugins
# Setup oh-my-zsh plugins
# Install 'trunk'
WORKDIR /tmp/work
COPY hashes.txt .
RUN curl https://github.com/sharkdp/bat/releases/download/v0.22.1/bat-musl_0.22.1_amd64.deb -L --output ./bat-musl_0.22.1_amd64.deb && \
    curl https://github.com/ogham/exa/releases/download/v0.10.1/exa-linux-x86_64-v0.10.1.zip -L --output ./exa-linux-x86_64-v0.10.1.zip && \
    sha256sum --check hashes.txt && \
    \
    git clone https://github.com/zsh-users/zsh-autosuggestions.git ~/.oh-my-zsh/custom/plugins/zsh-autosuggestions && \
    git clone https://github.com/zsh-users/zsh-syntax-highlighting.git ~/.oh-my-zsh/custom/plugins/zsh-syntax-highlighting && \
    sed -i "s/^plugins=.*/plugins=(git zsh-autosuggestions zsh-syntax-highlighting)/" ~/.zshrc && \
    \
    sudo dpkg -i ./bat-musl_0.22.1_amd64.deb && \
    sudo apt-get install -f && \
    \
    unzip ./exa-linux-x86_64-v0.10.1.zip && \
    sudo mv ./bin/* /usr/bin && \
    sudo mv ./completions/exa.zsh /usr/share/zsh/vendor-completions/_exa && \
    \
    curl https://get.trunk.io -fsSL | sudo bash -s - -y && \
    sudo chmod a+r /usr/local/bin/trunk && \
    \
    rm -rf /tmp/work

# Use zsh as the default shell
ENV SHELL /bin/zsh