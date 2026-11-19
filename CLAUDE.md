# CLAUDE.md

Guidance for Claude Code when working in this repository.

## Project

Status server for the MeshCom network, written in Rust. It reads the frames received by one
MeshCom node and shows on a live web map (Leaflet + WebSocket) which nodes hear which. Usage is
in [`README.md`](README.md).

## Commands

- Node.js: the version in `web/.nvmrc`, via NVM. In a non-interactive shell:
  `export NVM_DIR="$HOME/.nvm"; . "$NVM_DIR/nvm.sh"; (cd web && nvm use); hash -r`.
- Build and test: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`
  (`rustfmt.toml`: max width 120). `build.rs` builds the front end with npm;
  `MESHCOM_STATUS_SKIP_WEB=1` skips it.
- Release binary: `RUSTFLAGS="--remap-path-prefix=$HOME=~" cargo build --release --locked` (no
  build-machine paths; `[profile.release]` strips symbols), or the container image.
- Front end, in `web/`: `npm run build` (tsc + vite), `npm run lint`, `npm test` (vitest),
  `npm run dev` (proxy to the backend on :8080). npm only.
- Run against a node: `cargo run -- --node-host <ip>` (page on <http://localhost:8080/>). Needs
  PostgreSQL: `postgres://meshcom:meshcom@localhost:5432/meshcom` by default.
- Container (all in `docker/`, build context = repository root): `docker build -f docker/Dockerfile
  -t sardylan/meshcom-status .` (release), `--target dev` (toolchains, run with the repository on
  `/src`), `--target artifact` (binary alone). Compose, from `docker/`: `cp env.example .env`,
  `docker compose up -d`. CI/CD: `.github/workflows/ci.yml` (tests and image build check on merge requests and
  commits to `develop` / `main`; release build and publication only on semver `v*` tags).
  Raise the Rust pin in `docker/Dockerfile` and in the workflow together, and the Node.js pin
  together with `web/.nvmrc`.
- Debian package: `packaging/deb/build.sh <binary> <amd64|arm64> <version> <outdir>` (needs
  `dpkg-deb`, `objdump`); user `meshcom-status` created in `postinst`, deleted on `purge`;
  conffile `/etc/meshcom-status/meshcom-status.toml`. Do not install packages on the development
  machine to test them.
- Default config file: after changing a setting, regenerate `packaging/meshcom-status.toml` with
  `--config-dump` (the test `shipped_default_config_matches_the_defaults` fails otherwise).
- One version for back end and front end: `Cargo.toml` and `web/package.json` (and its lock file)
  must match, `build.rs` fails the build otherwise. Raise both together (`npm version <v>
  --no-git-tag-version` in `web/`).
- Licence: `GPL-3.0-or-later`, text in `LICENSE.md`. Declared as `license` (SPDX) in `Cargo.toml`
  (not `license-file`: Cargo wants only one, `license` for standard licences) and
  `web/package.json`; image label `org.opencontainers.image.licenses`; file shipped in the image
  and the release archive.
- No comments in source files (Rust, TypeScript, CSS, HTML, scripts, Docker, CI). Kept only where
  they work or a guideline requires them: the `///` of `struct Cli` in `src/config/cli.rs` (clap's
  `--help` and the config file text), comments justifying a rule exception (Rust guideline), lint
  and compiler directives, shebangs, `# syntax=`, the usage header of `packaging/deb/build.sh`,
  the header comments of `docker/compose.yml` and of the CI workflow, `docker/env.example`, the
  lintian overrides. SQL migrations are never edited.
- Unit tests never touch the database: the SQL layer (`src/store/mod.rs`) stays thin, row
  conversions (`src/store/rows.rs`) are pure and tested.

## Layout

| Path                   | Context                                                              |
|------------------------|----------------------------------------------------------------------|
| `src/main.rs`          | `main()` / `program()` (Rust guidelines §2)                          |
| `src/errors.rs`        | the crate `Error` and `Result`                                       |
| `src/config/`          | settings: `cli.rs` (clap + env), `file.rs` (TOML), `render.rs` (dump) |
| `src/logging.rs`       | `tracing` setup, UTC microsecond timestamps                          |
| `src/clock.rs`         | pure time formatting                                                 |
| `src/frame.rs`         | `Frame`/`Observation` shared by the sources, callsign/path/HEY parsing |
| `src/aprs.rs`          | APRS uncompressed position                                           |
| `src/console/`         | source: net console `RX_FRAME` lines and the raw MeshCom frame       |
| `src/kiss/`            | source: KISS deframer, AX.25, RxMeta/SrcInfo assembly                |
| `src/node_http/`       | source: own callsign/position from `/getparam/` (raw HTTP client)    |
| `src/retry.rs`         | reconnect delay of the sources: doubles up to 60 s, resets after a connection lasting 60 s — pure |
| `src/graph.rs`         | `Nodes` registry (global) and `window()`: links/routes of N receptions — pure |
| `src/chat.rs`          | chat targets (broadcast / channel / callsign), copy grouping — pure  |
| `src/stats.rs`         | network statistics: ranges, shapes, histograms — pure (counting is SQL in `store/`) |
| `src/view.rs`          | the window a client asks for: last N, kinds, copies                  |
| `src/store/`           | PostgreSQL (sqlx): receptions and nodes; `migrations/` has the schema |
| `src/import.rs`        | `--import`: parse an export of another instance's API                |
| `src/app.rs`           | wiring: sources → hub (store + nodes) → web events; the import       |
| `src/web/`             | axum: page, REST API, WebSocket (per-client window), Problem Details |
| `src/web/assets.rs`, `encoding.rs` | front end embedded as Brotli; identity/br/zstd/gzip forms computed on first request, kept in RAM; `Accept-Encoding` negotiation — pure |
| `docker/`              | `Dockerfile` (`base` / `dev` / `build` / `artifact` / `prod`; pins Rust and Node.js), `Dockerfile.dockerignore`, `compose.yml` (`postgres` 17 + `server`), `env.example`; `.env` and `volumes/` local only |
| `.github/workflows/ci.yml` | GitHub Actions: tests on merge requests and `develop`/`main`; on `v*` tags, native builds of 5 archives, 2 `.deb`, a 2-architecture image, then publication |
| `.github/dependabot.yml` | weekly update merge requests: Cargo, npm (`web/`), GitHub Actions; minor/patch grouped. Rust/Node.js pins stay manual |
| `packaging/`           | `meshcom-status.toml` (default config, = `--config-dump`, checked by a test); `deb/`: `build.sh` (dpkg-deb), systemd unit, maintainer scripts, lintian overrides |
| `build.rs`             | builds `web/` with npm into `$OUT_DIR/web` (embedded by `src/web/assets.rs`) |
| `web/src/lib/`         | front end: config, API types, WebSocket context (`live.tsx`), settings, geometry, GeoJSON — tested with vitest |
| `web/src/components/`  | front end: `MapView` (panel, MapLibre `MapPane`, `PathBanner`, `LayerSwitcher`), `ChatView`, `StatsView` (SVG `charts.tsx`) |
| `tests/fixtures/`      | real captures from a node (net console, KISS, HTTP)                  |

## Domain notes

- A link `A → B` means B received A. Links come from frame paths (origin, relays in order) plus
  the node's own reception when it measured the signal. HEY payloads (`R<n>;hw,rssi,snr;…`) carry
  the signal of every hop, measured by the hop's receiver.
- The firmware's `/getparam/` only answers when the request line says `HTTP/1.1`.

## Documentation

Read the relevant documents before writing or reviewing code. They are binding.

- [`doc/guidelines/guidelines.md`](doc/guidelines/guidelines.md) — rules for the
  `doc/guidelines/` folder.
- [`doc/guidelines/rust.md`](doc/guidelines/rust.md) — Rust implementation rules: errors, entry
  point, graceful shutdown on `SIGTERM`/`SIGINT`, configuration (CLI / env / TOML, `--config-save`,
  `--config-dump`), logging, modules, functional style, side effects, tests, toolchain, standard
  crates (`axum` for HTTP, `tokio`, `sqlx` with migrations and `--db-migrate`, `serde`, embedded
  front end built by `build.rs` with `rust-embed`).
- [`doc/guidelines/http.md`](doc/guidelines/http.md) — HTTP API rules: JSON payloads (WebSocket
  too), statelessness, REST, `/health`, `/api/{public,private}/vN` layout, JWT authentication,
  error format, compression (`br`/`zstd`/`gzip` negotiation, static files at maximum level, once).
- [`doc/guidelines/frontend.md`](doc/guidelines/frontend.md) — web pages: data kept on the
  backend, WebSocket for live data with a connection indicator, "how many items" selector with a
  default, visible app version, display settings in local storage; React + Vite + TypeScript for
  complex front ends, npm only, Node.js via NVM, MapLibre maps with selectable layers and zoom
  controls.
- [`doc/guidelines/persistence.md`](doc/guidelines/persistence.md) — storage choice: PostgreSQL
  (relational), MongoDB (documents), Redis (key-value caches).
- [`doc/guidelines/containers.md`](doc/guidelines/containers.md) — container images: one
  `Dockerfile` with `base` (runtime deps only), `dev` (`FROM base`, toolchains) and `prod` (last,
  `FROM base AS prod`) targets, Debian slim base, nothing from development in the release image,
  non-root user, pinned toolchains; Docker Compose: as much as possible in `compose.yml`, `env.example`, `.env` not committed, volumes
  bound on `./volumes/<service>/<volume>`.
- [`doc/guidelines/cicd.md`](doc/guidelines/cicd.md) — CI/CD and release: `main` / `develop` and
  merge requests, tests on merge requests and on commits to `develop` and `main`, release build and
  publication only on `v*` tags, Semantic Versioning (one version for every component, immutable
  tags), native builds per platform (archives for Linux/Windows/macOS, `.deb` for amd64/arm64,
  multi-architecture image), Debian packaging rules (FHS, conffiles, system user, hardened systemd
  unit, lintian), all-or-nothing publication, least-privilege pipelines.
- [`doc/meshcom-interfaces.md`](doc/meshcom-interfaces.md) — what a MeshCom node exposes
  (KISS/TCP on 8001, HTTP JSON endpoints on 80, ext-udp JSON on UDP 1799, net console debug stream on TCP 2323), their limits,
  quirks and security findings.

## Generic vs project documents

- Everything in `doc/guidelines/` is **project-agnostic** and copied across projects. Never add
  anything about this project there (no MeshCom references, no project-specific names, settings
  or paths).
- Project-specific documentation goes directly in `doc/` or in this file.
