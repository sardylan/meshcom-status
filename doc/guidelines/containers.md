# Containers

Rules for projects that are deployed as container images. They apply on top of the language
guidelines (for example [`rust.md`](rust.md) §8.2 for pinned toolchains).

## 0. The `docker/` folder

Every container file lives in a `docker/` folder at the repository root, not at the root itself:

| File                               | Content                                                     |
|------------------------------------|-------------------------------------------------------------|
| `docker/Dockerfile`                | the image (§1)                                              |
| `docker/Dockerfile.dockerignore`   | what the build leaves out (§5)                              |
| `docker/compose.yml`               | the Compose deployment (§7)                                 |
| `docker/env.example`               | every setting with its default (§7)                         |
| `docker/.env`                      | local settings, never committed (§7)                        |
| `docker/volumes/`                  | data of the Compose services, never committed (§7)          |

The build context is still the repository root, which holds the sources:
`docker build -f docker/Dockerfile .`, and in Compose `build: {context: .., dockerfile:
docker/Dockerfile}`.

## 1. One `Dockerfile`, three targets

The repository has one `docker/Dockerfile` (§0), built with BuildKit. It defines at least these
targets, in this order:

| Target | Content                                                                         |
|--------|---------------------------------------------------------------------------------|
| `base` | the base OS plus the **runtime** dependencies only, if any                       |
| `dev`  | the development environment: `base` plus toolchains and development dependencies |
| `prod` | the release image                                                                |

- `base` is the first stage. It installs only what the program needs to run (shared libraries,
  CA certificates, time zone data, ...), nothing used to build it.
- `dev` is `FROM base AS dev`, so development runs on the same OS release and runtime libraries as
  production. It adds compilers, package managers, linters, formatters, test tools and the build
  dependencies. It does not copy the sources: it is meant to be run with the repository mounted
  (for example `docker run --rm -it -v "$PWD":/src myapp:dev`).
- `prod` MUST be the **last** stage and MUST be `FROM base AS prod`, so a plain `docker build`
  (no `--target`) builds the release image, and that image starts from `base` and nothing else.
- Intermediate stages (for example a `build` stage `FROM dev AS build` that copies the sources and
  compiles them) MAY sit between `dev` and `prod`. `prod` takes only the built artifacts from them,
  with `COPY --from=...`.

## 2. Nothing from development in the release image

The `prod` image MUST NOT contain anything used only for development or building:

- no compilers, toolchains, package managers of the language (`cargo`, `npm`, `pip`, ...), build
  tools, debuggers, linters or test tools;
- no source code, tests, fixtures, documentation or build directories (`target/`,
  `node_modules/`, ...);
- no package manager caches or lists (`/var/lib/apt/lists/*`, `~/.cargo/registry`, `~/.npm`, ...);
- no secrets: credentials, keys and tokens are given at run time (environment, mounted files),
  never copied into any stage or passed as build arguments.

`prod` copies the release artifacts (the binary, and any file it needs at run time) and sets how
the program runs. Nothing else.

## 3. Base image

- The base image MUST be the smallest one that runs the program, and SHOULD be **Debian**: the
  `slim` variant of a pinned release (`debian:13-slim`, `debian:trixie-slim`), never `latest` and
  never the full image. Another OS is used only when Debian cannot do the job; the reason is
  written next to the `FROM` line.
- Every stage uses the same Debian release, so what `dev` builds links against the libraries that
  `base` ships. Images of language toolchains are taken in their variant for that release
  (`<image>:<version>-slim-trixie`, `<image>:<version>-trixie-slim`).
- Packages are installed with `apt-get install --no-install-recommends`, and the apt lists are
  removed in the same `RUN` (`rm -rf /var/lib/apt/lists/*`), so they do not stay in a layer.
- Statically linked programs with no runtime dependencies still use the Debian slim base, so the
  image has the same layout, users and tools for inspection everywhere.

## 4. Versions

- Toolchain versions are pinned exactly, with `ARG`s at the top of the `Dockerfile`
  (`ARG RUST_VERSION=1.2.3`, `ARG NODE_VERSION=24.1.0`, `ARG DEBIAN_RELEASE=trixie`), and match
  the pins elsewhere in the project (CI, `.nvmrc`, ...). Where a version is also written in a
  project file, the build checks that the two agree and fails if they do not.
- Lock files are used as they are: `cargo build --locked`, `npm ci`.

## 5. Build

- `docker/Dockerfile.dockerignore` (read by BuildKit because it sits next to the Dockerfile, with
  its name; patterns are relative to the context, the repository root) keeps the build context
  small and the build reproducible: it excludes build outputs (`target/`, `dist/`,
  `node_modules/`), version control data, editor files, local configuration, anything holding
  secrets (`**/.env`), and the `docker/` folder itself. There is no `.dockerignore` at the root.
- Dependency and compiler caches use BuildKit cache mounts
  (`RUN --mount=type=cache,target=...`), not layers, so they never reach an image.
- The build stage runs as root; that is fine because it never ships.

## 6. Running

- `prod` runs as a dedicated **non-root** user, created in the `prod` stage with a fixed UID/GID.
- The program is started in exec form (`ENTRYPOINT ["/usr/local/bin/myapp"]`), so it is PID 1 and
  receives `SIGTERM`; it shuts down cleanly on it. Default arguments, if any, go in `CMD`.
- Configuration comes from environment variables and optional mounted files (see
  [`rust.md`](rust.md) §3); the image sets sensible defaults (`ENV`) such as a listen address on
  all interfaces and a configuration file path under `/etc/myapp/`.
- Logs go to standard output and standard error, never to files inside the container.
- Network ports the program listens on are declared with `EXPOSE`.
- Data that must survive the container lives in external services or mounted volumes, never in
  the container's own file system.
- Health is checked by the orchestrator through the program's health endpoint (see
  [`http.md`](http.md)). No tool is added to the image just to run a `HEALTHCHECK`.

## 7. Docker Compose

When the program is deployed with Docker Compose (alone or with the services it needs, such as a
database):

- The file is named **`compose.yml`**, in `docker/` (§0), and Compose is run from there. Not
  `docker-compose.yml`, `compose.yaml` or other names, and no override files
  (`compose.override.yml`).
- **As much as possible lives in that single file**: images, build, environment, ports, volumes,
  health checks, and a default for every setting, so that the file alone shows the whole
  deployment and runs as it is. Settings that differ between installations (addresses, ports,
  credentials) are read from a `.env` file through interpolation with a default,
  `${MYAPP_PORT:-8080}`; the program's service may also take `env_file: [{path: .env, required:
  false}]`, so that any of its settings can be changed there. Nothing else goes in `.env`.
- **`docker/env.example`** lists every variable the program reads (and those that `compose.yml`
  interpolates), each with its default value and a comment. Users copy it to `docker/.env`,
  which Compose reads next to `compose.yml`. Defaults are generic: example addresses from the
  documentation ranges (`192.0.2.0/24`, `example.com`), never a real host, name or credential of
  the developers.
- **`.env` is never committed**: it is listed in `.gitignore` and in the Dockerfile's ignore file.
- **Volumes are bind mounts on a relative folder**, one per service and volume,
  `./volumes/<service>/<volume_name>` relative to `compose.yml` (so in `docker/volumes/`), for
  example `./volumes/postgres/data:/var/lib/postgresql/data`. No named volumes: the data sits next
  to the file, visible, easy to back up or move with the project. `docker/volumes/` is listed in
  `.gitignore`.
- The program's service is built from `docker/Dockerfile` with the repository root as context
  (target `prod`, see §1), and tagged with an `image:` name.
- Services that the program needs (a database) are not published on host ports unless a user must
  reach them directly; the program reaches them by service name on the Compose network.
- A needed service has a `healthcheck`, and the program waits for it with
  `depends_on: { <service>: { condition: service_healthy } }`.
- The program's service runs with `read_only: true`, `cap_drop: [ALL]` and
  `security_opt: [no-new-privileges:true]` unless it needs more, and `restart: unless-stopped`.
- A comment at the top of `compose.yml` gives the commands to start, follow the logs and stop.
