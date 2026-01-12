FROM fedora:43

# Install dependencies that VHS needs plus build tools for ttyd
RUN dnf install -y \
    ffmpeg \
    chromium \
    bash \
    golang \
    cmake gcc g++ libuv-devel json-c-devel openssl-devel zlib-devel libwebsockets-devel \
    && dnf clean all

# Build and install ttyd from source
RUN git clone https://github.com/tsl0922/ttyd.git /tmp/ttyd \
    && cd /tmp/ttyd \
    && mkdir build && cd build \
    && cmake .. \
    && make && make install \
    && rm -rf /tmp/ttyd

# Install VHS
RUN go install github.com/charmbracelet/vhs@latest
ENV PATH="/root/go/bin:${PATH}"

# Install nice fonts
RUN dnf install -y \
    jetbrains-mono-fonts \
    google-noto-sans-mono-fonts \
    && dnf clean all

# Enable no-sandbox mode for chromium (required when running as root)
ENV VHS_NO_SANDBOX="true"

WORKDIR /vhs
ENTRYPOINT ["vhs"]
