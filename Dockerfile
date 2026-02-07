FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml ./
COPY Cargo.lock* ./
COPY src ./src
RUN rustup target add x86_64-unknown-linux-musl
RUN RUSTFLAGS="-C link-arg=-s" cargo build --release --target x86_64-unknown-linux-musl

FROM scratch
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/stubdns /stubdns
EXPOSE 8053
ENTRYPOINT ["/stubdns"]
