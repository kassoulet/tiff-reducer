# Builder stage - Use Debian for standard dynamic library support
FROM rust:bookworm as builder

# Install build dependencies for dynamic linking
RUN apt-get update && apt-get install -y \
    cmake \
    git \
    pkg-config \
    libtiff-dev \
    libzstd-dev \
    liblzma-dev \
    libjpeg-dev \
    libwebp-dev \
    libdeflate-dev \
    libgeotiff-dev \
    zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/tiff-reducer
COPY . .

# Build without 'vendored' feature to link against system libraries
RUN cargo build --release

# Final stage - use Debian slim for small runtime image
FROM debian:bookworm-slim

# Install runtime libraries
RUN apt-get update && apt-get install -y \
    libtiff6 \
    libzstd1 \
    liblzma5 \
    libjpeg62-turbo \
    libwebp7 \
    libdeflate0 \
    libgeotiff5 \
    zlib1g \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/tiff-reducer/target/release/tiff-reducer /usr/local/bin/tiff-reducer

ENTRYPOINT ["tiff-reducer"]
