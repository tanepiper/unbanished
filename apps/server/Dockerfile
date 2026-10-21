# The Un-Banished multiplayer server in a container. It holds no game data: players bring their
# own. Build from the repository's root (the .dockerignore there keeps the game's files out):
#
#   docker build -f apps/server/Dockerfile -t unbanished-server .
#   docker run -p 7780:7780 unbanished-server
#
# A settings file: docker run -v $PWD/server.conf:/etc/unbanished-server.conf -p 7780:7780 \
#   unbanished-server --config /etc/unbanished-server.conf 0.0.0.0:7780
# Players reach it at ws://HOST:7780; a page served over https needs wss:// through a proxy
# (apps/server/README.md).

FROM rust:1.99-slim-trixie AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p unbanished-server

FROM debian:trixie-slim
COPY --from=build /src/target/release/unbanished-server /usr/local/bin/unbanished-server
USER nobody
EXPOSE 7780
# `GET /health` answers `ok` on the game port.
HEALTHCHECK --interval=30s --timeout=3s CMD bash -c 'exec 3<>/dev/tcp/127.0.0.1/7780 && printf "GET /health HTTP/1.1\r\nHost: x\r\n\r\n" >&3 && grep -q "^ok" <&3'
ENTRYPOINT ["unbanished-server"]
CMD ["0.0.0.0:7780"]
