# Builder stage - Use Alpine for native musl support
FROM rust:alpine as builder

# Install build dependencies (Alpine uses musl natively)
RUN apk add --no-cache \
    build-base \
    cmake \
    git \
    pkgconfig \
    tiff-dev \
    zstd-dev \
    xz-dev \
    zlib-dev \
    libjpeg-turbo-dev \
    libwebp-dev \
    libdeflate-dev

WORKDIR /usr/src/tiffthin-rs
COPY . .

# Build with vendored libtiff for maximum static linking
# Note: Alpine's musl libc is used, but some dynamic linking may occur
RUN cargo build --release --features vendored

# Final stage - use Alpine base for runtime libraries
FROM alpine:latest

RUN apk add --no-cache \
    tiff \
    libwebp \
    zstd \
    xz \
    zlib \
    libjpeg-turbo \
    libdeflate

COPY --from=builder /usr/src/tiffthin-rs/target/release/tiffthin-rs /tiffthin-rs

ENTRYPOINT ["/tiffthin-rs"]
