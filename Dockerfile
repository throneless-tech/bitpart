####################################################################################################
## Builder
####################################################################################################
FROM rust:bookworm AS builder

RUN apt update && apt install -y protobuf-compiler
RUN update-ca-certificates

# Create appuser
ENV USER=bitpart
ENV UID=10001

RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    "${USER}"


WORKDIR /bitpart

COPY ./ .

# We no longer need to use the x86_64-unknown-linux-musl target
RUN cargo build --release

####################################################################################################
## Final image
####################################################################################################
FROM gcr.io/distroless/cc

# Import from builder.
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group

WORKDIR /bitpart

# Copy our build
COPY --from=builder /bitpart/target/release/bitpart ./

# Use an unprivileged user.
USER bitpart:bitpart

CMD ["/bitpart/bitpart"]
