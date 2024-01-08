FROM rust:1.75.0 as builder

RUN apt-get update \
  && apt install -y clang llvm pkg-config nettle-dev postgresql-client sqitch

###########################################################
FROM builder as ci

# We need the top-level Cargo.toml in order to build, so copy everything.
WORKDIR /app
COPY . .

#RUN cargo build --release
RUN sh .ci/ci.sh

###########################################################
FROM debian:bookworm-slim as prod
RUN apt-get update \
    && apt-get install -y ca-certificates tzdata libnettle8 imagemagick postgresql-client sqitch \
    && rm -rf /var/lib/apt/lists/*

# Use of integer ids is required for 'securityContext' in k8s manifests.
RUN addgroup --system --gid 1001 rustacean \
  && adduser --system --shell /bin/false --no-create-home --disabled-password --disabled-login --gid 1001 --uid 1001 rustacean

WORKDIR /app

COPY --chown=rustacean:rustacean --from=ci /app/target/release/scrolls scrolls

USER rustacean
CMD ["/app/scrolls"]
