FROM rust:alpine3.22 AS builder

RUN apk add --no-cache build-base musl-dev openssl-dev pkgconfig

RUN apk add --no-cache openssl-libs-static

WORKDIR /app

COPY ./src ./src
COPY ./Cargo.toml ./Cargo.toml

RUN cargo build --release

# Final Image

FROM alpine

WORKDIR /app

COPY --from=builder /app/target/release/corel_rs /app

RUN chmod +x /app/corel_rs

ENTRYPOINT [ "/app/corel_rs" ]

