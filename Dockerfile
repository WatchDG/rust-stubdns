FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev ca-certificates
WORKDIR /app
COPY Cargo.toml ./
COPY Cargo.lock* ./
COPY src ./src
RUN rustup target add x86_64-unknown-linux-musl
RUN RUSTFLAGS="-C link-arg=-s" cargo build --release --target x86_64-unknown-linux-musl

FROM scratch
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/stubdns /stubdns
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY config/stubdns.json /config/stubdns.json
EXPOSE 8053
ENTRYPOINT ["/stubdns", "--config=/config/stubdns.json"]
