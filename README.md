# meshcom-status

Status server for a [MeshCom](https://icssw.org/meshcom/) network. It connects to one MeshCom
node, reads every frame the node receives, stores it in PostgreSQL, and shows on a live map
**which nodes hear which**:
nodes with their callsign, and geodesic lines with an arrow from the transmitter to the receiver.
A live **Traffic** list shows every received frame: text messages, position beacons, HEY beacons
and anything else. A **Chat** view shows the text messages as conversations.

The page has two views, switched from the top bar: **Map** and **Chat**. The top bar also shows
the WebSocket state; the footer, shared by both views, shows the server version.

## Quick start

The web front end (`web/`, React + Vite + TypeScript) is built by Cargo with npm, so Node.js must
be on `PATH`, in the version of `web/.nvmrc` (managed with [NVM](https://github.com/nvm-sh/nvm)):

```bash
cd web && nvm install && nvm use && cd ..
cargo run --release -- --node-host 192.0.2.10
```

Open <http://localhost:8080/>. A PostgreSQL database is needed; by default
`postgres://meshcom:meshcom@localhost:5432/meshcom` (`--database-url`). The schema is created and
updated at startup.

The node needs its net console and LoRa debug output on. Both are switched on through its Web-API:

```bash
curl 'http://192.0.2.10/setparam/?netconsole=on'
curl 'http://192.0.2.10/setparam/?manualcommand=--loradebug%20on'
```

## How the map is built

Every received frame carries its path: the origin, then each relay that retransmitted it. The
server turns each consecutive pair into a link **A → B**, meaning *B heard A*. When the node
measured the signal itself, a last link goes from the last transmitter to the node.

The map uses MapLibre GL. The **Layers** button (top left) chooses the base map (OpenStreetMap,
OpenTopoMap, Esri satellite by default; configurable with `web.base_maps`) and turns the overlays
on and off: links, nodes, callsign labels, path highlights. The base map is dimmed (darker, less
saturated) so that the lines stand out; a checkbox there turns this off. These choices are saved
in the browser. Zoom buttons and a compass are top right.

**Auto-zoom** (next to Layers, on by default) moves the map to every new frame shown:

- **last message**: the stations on the newest frame's path (default);
- **all visible paths**: both ends of every link drawn;
- **all visible stations**: every station on the path of a frame in the window.

A manual pan or zoom pauses it for 30 seconds ("resume" ends the pause). It waits while a path
banner is open, and stops on whole zoom levels like every other move.

- **Arrow**: direction of the transmission (from A to B). When both directions exist, the two
  arrows sit on either side of the middle of the line.
- **Colour**: last SNR measured at the receiver. Signals come from the node's own receptions and
  from **HEY beacons**, which carry the RSSI/SNR of every hop of their path.
- **Dashed grey**: the link is known from a path, but its signal was never measured (for example
  a hop through the MeshCom internet server).
- Nodes appear on the map once their position is known (from their position beacon, usually every
  15–30 minutes). Until then they are listed in the side panel only.

## Paths

A path is one distinct transmission chain: origin, relays in order, and this node when it
measured the signal (e.g. `QC0BRV-10 → QF0ECH-10 → QB0ALF-12 → QA0OWN-11`). A link usually
belongs to several paths.

- **Hover** a segment: every path through it is highlighted, and a tooltip says how many.
- **Click** a segment: the most recent path through it is highlighted, and a banner opens at the
  bottom right of the map with every node of the path (role, position, hardware, firmware, last
  seen), the last signal of every hop, how often the path was used and by which kinds of frames,
  and the list of all other paths through that segment (click one to switch).
- Clicking a frame in the Traffic list opens the banner for that frame's path. `Esc` or `×`
  closes the banner.

## Chat

One tab per conversation:

- **all**: broadcasts (messages addressed to everyone);
- one tab per **group of the node** (its `setgrc` setting, read at startup), e.g. `123`, `12302`;
- tabs added with **+**: a channel or a callsign, with autocomplete from the server (channels found
  in the stored messages, known callsigns; tabs already open are not suggested). Each suggestion
  shows the channel or callsign and its history. A callsign tab shows the direct messages to and from
  that callsign. Added tabs are saved in the browser and can be closed with ×.

A tab opens on the last 20 messages, newest at the bottom; older ones load while scrolling up. New
messages arrive live. Each message shows who sent it, when, how long ago (updated live), and every path through which the node
received it ("via" relays, RSSI/SNR, time): copies of the same message are shown as one message.
Acknowledgements (`ack…`) are not shown. The chat is read-only.

## Stats

Statistics of the network over the last 24 hours (default), 7 days, 30 days or all time; the
choice is saved in the browser. They are computed by the database and refreshed at most every
30 seconds while new frames arrive.

- **Tiles:** distinct frames and receptions (copies included), chat messages (acknowledgements
  left out), stations heard (distinct origins), relays (calls that forwarded frames), share of
  frames heard directly (no relay), copies per frame.
- **Frames per hour** (per day for 30 days and all time), stacked by type.
- **Frames by type**, **activity by hour of day** (in the browser's time zone), **relays per
  frame** (direct, 1, 2, 3, 4 or more), **signal** (receptions by SNR measured by the own node,
  coloured as on the map).
- **Busiest stations** (frames originated), **busiest relays** (frames forwarded) and **chat**
  (messages by addressee), top 10 each.

Frames count once whatever the number of copies, except where receptions are named. Every chart
has a hover tooltip and a table view.

## Storage

PostgreSQL keeps everything, forever. Primary keys are UUIDs and times are stored in UTC
(`timestamp` without time zone); the API sends times as ISO 8601 with `Z`.

- `receptions`: every reception, with its time, source, message ID, copy number, origin, relays,
  path, signal, origin hardware/firmware and payload (message text, position, HEY hop reports...).
- `nodes`: the latest state of every node (position, hardware, firmware, frames sent, last seen,
  and which one is this node).

Links and paths are not stored: they are rebuilt from the receptions of the requested window.

The schema is applied at startup. To apply it as a separate deployment step, run
`meshcom-status --db-migrate`, which migrates the database and exits.

To move the data of another instance, export its API and import it:

```bash
curl -s http://old-host:8080/api/public/v1/nodes > nodes.json
curl -s http://old-host:8080/api/public/v1/frames > frames.json
jq -s '{nodes: .[0].nodes, frames: .[1].frames}' nodes.json frames.json > export.json
meshcom-status --import export.json
```

## What is shown: the last N frames

The page shows the **last N frames** that pass the filters, **30** by default; the selector offers
10, 30, 100, 500, 1000 and all. The server keeps the whole history in the database and sends the
page only that window:

- the **Traffic** list shows exactly those frames;
- **links and paths** on the map are built from those frames only;
- **nodes** are always all shown, with their latest known position (positions are sent rarely).

The status badge at the top shows the WebSocket state (connecting / live / offline); the
footer shows the server version.

Display settings (the "last N" value, the filters, the open tab, the map position and zoom) are
saved in the browser's local storage and restored when the page opens; they are per browser and
never hold data.

Changing the selector or a filter (MSG / POS / HEY / other, copies) asks the server for a new
window: the list and the map change together. N counts only the frames that pass the filters
(e.g. "last 30 HEY beacons"). New frames slide into the window live.

## Traffic

The side panel lists the frames of the window, newest first:

- **MSG** text messages (direct, group, broadcast), **POS** position beacons, **HEY** beacons
  with the signal of each hop, and **…** anything else, with its raw text.
- The same frame received again through another path is marked **copy 2**, **copy 3**, ... (by
  MeshCom message ID, within an hour).
- Each new frame briefly draws its path on the map. Clicking a frame keeps its path drawn,
  zooms to it and shows the details: full path, message ID, origin hardware and firmware, and
  the signal of each hop of a HEY beacon.

## Sources

| Source        | Setting                     | Default | Gives                                                  |
|---------------|-----------------------------|---------|--------------------------------------------------------|
| Net console   | `node.console_enabled`      | on      | every frame, duplicates and HEY beacons, with signal   |
| KISS          | `node.kiss_enabled`         | off     | positions and messages, first copy only, no HEY        |
| HTTP Web-API  | (always, with `node.host`)  | —       | the node's own callsign and position                   |

Enabling KISS together with the net console counts each frame twice. See
[`doc/meshcom-interfaces.md`](doc/meshcom-interfaces.md) for the details of every interface.

## API

| Endpoint                        | Content                                                     |
|---------------------------------|-------------------------------------------------------------|
| `GET /health`                   | `204` while the server runs                                 |
| `GET /api/public/v1/nodes`      | `{"nodes":[…]}`, every node                                 |
| `GET /api/public/v1/frames`     | `{"view":…,"frames":[…]}`, the window's frames, oldest first |
| `GET /api/public/v1/links`      | `{"view":…,"links":[…]}`, links built from the window      |
| `GET /api/public/v1/paths`      | `{"view":…,"paths":[…]}`, paths built from the window      |
| `GET /api/public/v1/chat/tabs`  | `{"tabs":[…],"node_groups":[…]}`, default chat tabs          |
| `GET /api/public/v1/chat/targets` | `?q=<prefix>&limit=20&exclude=<targets>`: channels and callsigns for a new tab, without the excluded ones |
| `GET /api/public/v1/chat/messages` | `?target=<target>&before=<key>&limit=20`: a page of messages, oldest first, and `more` |
| `GET /api/public/v1/stats`      | `?range=24h\|7d\|30d\|all&tz=<IANA zone>`: statistics of the range (defaults `24h`, `UTC`); `tz` sets the hours of day and the timeline buckets |
| `GET /api/public/v1/ws`         | WebSocket, see below                                        |

`frames`, `links` and `paths` take the window as query parameters: `limit` (a number or `all`,
default 30), `kinds` (comma-separated `message,position,hey,other`, default all) and `copies`
(`true`/`false`, default `true`). A bad value is answered with `422` and a Problem Details body.

WebSocket:

- server → client: `snapshot` (view, all nodes, window frames, links, paths) on connect and after
  every view change; `frame` (the new frame, the IDs of the frames that left the window, and the
  window's links and paths); `node` (a node changed); `error` (a client message was refused).
- client → server: `{"type":"view","limit":30,"kinds":["message","hey"],"copies":true}`
  (`limit: null` for all); `{"type":"chat","target":"channel:123"}` to receive the new messages
  of a conversation as `chat` messages (`target: null` to stop).

A chat target is `broadcast`, `channel:<group>` or `callsign:<CALL>`. A chat message's `key` is
the ID of its first reception; a `chat` message with a known `key` replaces it (a new copy).

The `/api` prefix is configurable (`web.api_prefix`).

## Docker Compose

Everything for containers is in [`docker/`](docker/). `docker/compose.yml` runs the server (image
`sardylan/meshcom-status` from Docker Hub) and its PostgreSQL database. It holds every setting
with a default; what differs for your installation goes in `docker/.env`, starting from
`docker/env.example`, which lists every variable with its default value:

```bash
cd docker
cp env.example .env     # then set MESHCOM_STATUS_NODE_HOST to your node
docker compose up -d    # add --build to build the image from this checkout
```

`.env` is never committed. The page is on <http://localhost:8080/> (`MESHCOM_STATUS_HTTP_PORT`).
The database is not published outside the Compose network; its files are in
`docker/volumes/postgres/data` (ignored by git) and survive restarts. They belong to the
container's `postgres` user (UID 999): deleting them needs `sudo rm -rf docker/volumes/postgres`.
The server waits for the database to be healthy, creates the schema at startup, runs read-only
without privileges, and stops gracefully on `docker compose down`.

## Container

`docker/Dockerfile` builds a Debian slim image holding only the binary (and, in
`/usr/share/doc/meshcom-status/`, the licence and the README), run as an unprivileged user,
published as `sardylan/meshcom-status`. The build context is the repository root:

```bash
docker build -f docker/Dockerfile -t sardylan/meshcom-status .
docker run -d --name meshcom-status -p 8080:8080 \
  -e MESHCOM_STATUS_NODE_HOST=192.0.2.10 \
  -e MESHCOM_STATUS_DATABASE_URL=postgres://meshcom:meshcom@db.example.com:5432/meshcom \
  sardylan/meshcom-status
```

Every setting is also an environment variable (`--help` and `docker/env.example` list them). A
configuration file can be mounted on `/etc/meshcom-status/meshcom-status.toml`. The image listens
on `0.0.0.0:8080`; the health check is `GET /health`.

The `dev` target is the development environment (Rust with clippy and rustfmt, Node.js and npm,
in the pinned versions), to run with the repository mounted; the `artifact` target is the release
binary alone, with the licence:

```bash
docker build -f docker/Dockerfile --target dev -t sardylan/meshcom-status:dev .
docker run --rm -it -v "$PWD":/src sardylan/meshcom-status:dev cargo test
docker buildx build -f docker/Dockerfile --target artifact --output type=local,dest=out .
```

## Debian package

Each release has a `.deb` for `amd64` and `arm64` (Debian 13 or newer, Ubuntu 22.04 or newer):

```bash
sudo apt install ./meshcom-status_1.0.1-1_amd64.deb
```

| Path                                             | Content                                         |
|--------------------------------------------------|-------------------------------------------------|
| `/usr/bin/meshcom-status`                        | the program                                     |
| `/etc/meshcom-status/meshcom-status.toml`        | the configuration (conffile, `root:meshcom-status 0640`) |
| `/usr/lib/systemd/system/meshcom-status.service` | the service                                     |
| `/usr/share/doc/meshcom-status/`                 | README, copyright, changelog                    |

- Installing creates the system user and group `meshcom-status` (no home, no shell), enables and
  starts the `meshcom-status` service, which runs as that user with a hardened unit (no
  capabilities, read-only system, private `/tmp`), and restarts it on upgrades.
- Set the node and the database in `/etc/meshcom-status/meshcom-status.toml`, then
  `sudo systemctl restart meshcom-status`; logs: `journalctl -u meshcom-status`. Upgrades keep
  your changes to the file. PostgreSQL is recommended on the same host (`Recommends: postgresql`);
  the service retries every 10 s until the database answers.
- `apt remove` stops and removes the program but keeps the configuration and the user;
  `apt purge` also deletes the configuration, the user and the group.

The package is built by `packaging/deb/build.sh` (`dpkg-deb`, no debhelper) from the release
binary; `packaging/deb/` holds the unit, the maintainer scripts and the lintian overrides.

## Continuous integration and releases

`.github/workflows/ci.yml` (GitHub Actions), with the toolchains pinned as in `docker/Dockerfile`,
following [`doc/guidelines/cicd.md`](doc/guidelines/cicd.md):

- **merge requests** to `develop` or `main`, and **commits** on `develop` or `main`: front-end lint
  and tests, `cargo fmt --check`, clippy, Rust tests, and a check that the container image builds.
  Nothing is published.
- **tag `vMAJOR.MINOR.PATCH`** (Semantic Versioning, optionally `-rc.1` and such; it must equal the
  version in `Cargo.toml` and `web/package.json`): the tests again, then a release build on a
  native runner per platform, and, only when every build succeeded, the publication:

| Artifact | Platforms |
|----------|-----------|
| archive: binary, `meshcom-status.toml`, `README.md`, `LICENSE.md` | `linux-amd64`, `linux-arm64`, `macos-arm64` (`.tar.gz`); `windows-x64`, `windows-arm64` (`.zip`) |
| Debian package (lintian-checked) | `amd64`, `arm64` |
| image `sardylan/meshcom-status` on Docker Hub | `linux/amd64`, `linux/arm64` (one multi-architecture image) |

The GitHub release holds the archives, the packages and `SHA256SUMS`, and is marked as a
pre-release for `-…` versions. Image tags: `X.Y.Z`; for stable versions also `X.Y`, `X` (from
1.0.0 on) and `latest`. Linux binaries come from the Dockerfile's `artifact` target; macOS and
Windows ones are built with the same pinned Rust. The default `meshcom-status.toml` is
`packaging/meshcom-status.toml`, which a test keeps equal to `--config-dump` with the defaults.

The repository needs the secrets `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN`.

To release, set the new version in `Cargo.toml` (`version = "0.2.0"`) and in the front end, merge
into `main`, then tag `v0.2.0` there:

```bash
cd web && npm version 0.2.0 --no-git-tag-version
```

## Configuration

Every setting can be given on the command line, in an environment variable or in a TOML file,
in this order of precedence. `meshcom-status --help` lists them all with their defaults.

```bash
meshcom-status -n 192.0.2.10 --config-dump   # write meshcom-status.toml with every setting
```

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

A release binary built outside the container remaps the home directory, so that it carries no
paths of the build machine (the release profile already strips the symbols):

```bash
RUSTFLAGS="--remap-path-prefix=$HOME=~" cargo build --release --locked
```

`build.rs` runs `npm ci` (when needed) and `npm run build` in `web/`, and the binary embeds
`web/dist` (`rust-embed`), every file compressed with Brotli (2.0 MB of front end, 0.46 MB
embedded). `MESHCOM_STATUS_SKIP_WEB=1` skips the front end and embeds a placeholder page, for
builds without Node.js.

The server sends each text file as Brotli, zstd, gzip or uncompressed, as the request's
`Accept-Encoding` allows (Brotli first on equal weights). Brotli is sent as embedded; the other
forms are computed on the first request for them and kept in memory.

Front end alone (in `web/`, after `nvm use`), with a backend running on port 8080:

```bash
npm run dev     # http://localhost:5173 with hot reload; /api is forwarded to :8080
npm run lint
npm test
npm run build
```

The rules for the code are in [`doc/guidelines/`](doc/guidelines/).

## License

meshcom-status is free software: you can redistribute it and/or modify it under the terms of the
GNU General Public License as published by the Free Software Foundation, either version 3 of the
License, or (at your option) any later version (SPDX: `GPL-3.0-or-later`). See
[`LICENSE.md`](LICENSE.md).

The licence is declared in `Cargo.toml` and `web/package.json`, set as the
`org.opencontainers.image.licenses` label of the container image, installed in it
(`/usr/share/doc/meshcom-status/LICENSE.md`) and shipped with every release archive.
