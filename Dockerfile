# SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
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

# Pick the executable file for the right architecture and system
RUN mv /volume/target/*-unknown-*-musl/release/notify_push /notify_push

FROM scratch

COPY --from=build /notify_push /
EXPOSE 7867

CMD ["/notify_push"]
