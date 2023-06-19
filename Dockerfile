FROM clux/muslrust:stable AS build

COPY Cargo.toml Cargo.lock ./

# Build with a dummy main to pre-build dependencies
RUN mkdir src && \
 sed -i '/test_client/d' Cargo.toml && \
 echo "fn main(){}" > src/main.rs && \
 cargo build --release && \
 rm -r src

COPY build.rs ./
COPY appinfo/info.xml ./appinfo/
COPY src/ ./src/
RUN touch src/main.rs

RUN cargo build --release

FROM scratch

COPY --from=build /volume/target/x86_64-unknown-linux-musl/release/notify_push /
EXPOSE 7867

ENTRYPOINT ["/notify_push"]
