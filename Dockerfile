# Stage 1: Build
FROM rust:1.85.1 AS builder

# Set the working directory
WORKDIR /usr/src/app

# Cache dependencies
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY . .

# Build the project in release mode
RUN cargo build --release

# Stage 2: Runtime
FROM ubuntu:24.04

# Install minimal runtime dependencies
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /app

# Copy the compiled binary from the build stage
COPY --from=builder /usr/src/app/target/release/rs-benchmark .

# Set the startup command
CMD ["./rs-benchmark"]