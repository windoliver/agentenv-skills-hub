FROM rust:1-bookworm AS build
WORKDIR /app
COPY . .
RUN cargo build --release -p hub-api

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/hub-api /usr/local/bin/hub-api
EXPOSE 7777
CMD ["hub-api"]
