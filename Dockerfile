FROM ekidd/rust-musl-builder AS build

COPY Cargo.toml Cargo.lock ./

# Build with a dummy main to pre-build dependencies
RUN mkdir src && \
 sudo chown -R rust:rust . && \
 sed -i '/test_client/d' Cargo.toml && \
 echo "fn main(){}" > src/main.rs && \
 cargo build --release && \
 rm -r src

COPY build.rs ./
COPY appinfo/info.xml ./appinfo/
COPY src/ ./src/
RUN sudo chown -R rust:rust . && touch src/main.rs

RUN cargo build --release

FROM scratch

COPY --from=build /home/rust/src/target/x86_64-unknown-linux-musl/release/notify_push /
EXPOSE 7867

CMD ["/notify_push"]