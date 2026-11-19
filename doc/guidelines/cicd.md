# CI/CD and Release Guidelines

These rules are binding for the continuous integration, delivery and release of every program in
this repository. They are tool-agnostic; the examples use GitHub Actions. Toolchain pins follow
[`rust.md`](rust.md) §8.2, container images [`containers.md`](containers.md).

The key words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119.

## 1. Branches

| Branch            | Content                                                                 |
|-------------------|-------------------------------------------------------------------------|
| `main`            | the released code: every release tag is on a commit of `main`           |
| `develop`         | the integration branch: the next release, always passing the tests      |
| feature branches  | one change each, branched from `develop`, merged back with a merge (pull) request |

- `main` and `develop` are protected: no direct pushes, changes arrive through merge requests,
  and a merge request MUST NOT be merged while the pipeline fails.
- `develop` is merged into `main` with a merge request when a release is prepared.

## 2. What runs when

| Event                                         | Tests | Build checks | Release build | Publication |
|-----------------------------------------------|:-----:|:------------:|:-------------:|:-----------:|
| Merge request to `develop` or `main`          |  yes  |     yes      |      no       |     no      |
| Commit (push) on `develop` or `main`          |  yes  |     yes      |      no       |     no      |
| Version tag (`vMAJOR.MINOR.PATCH…`)           |  yes  |      —       |      yes      |     yes     |

- **Tests** are everything that can fail a change: formatting, linters and static analysis with
  warnings as errors, unit tests of every component (back end and front end), lock-file
  consistency.
- **Build checks** prove that what would be released still builds (for example the container
  image), without publishing anything and without pushing to any registry.
- **The release build and the publication happen only on version tags**: never from a branch, a
  merge request or a manual run. There are no images or artifacts published per branch or per
  commit (`edge`, `nightly`, `sha-…`).
- A tag runs the tests again before building: a release is never built from code whose tests did
  not pass in that same run.

## 3. Versions: Semantic Versioning

Versions follow [Semantic Versioning 2.0.0](https://semver.org): `MAJOR.MINOR.PATCH`, with an
optional pre-release (`-rc.1`, `-beta.2`) and optional build metadata (`+…`, ignored for
ordering).

- **MAJOR** for incompatible changes (API, configuration, storage format, command line), **MINOR**
  for compatible new features, **PATCH** for compatible fixes. Lower parts reset to 0.
- While MAJOR is `0`, anything MAY change between minor versions: `0.y.z` is for development, and
  `1.0.0` marks the first stable interface.
- A **pre-release** (`1.2.0-rc.1`) has lower precedence than its version and is never treated as
  the latest stable one.
- **One version for the whole project**: every component and manifest (for example `Cargo.toml`
  and the front end's `package.json` and lock file) carries the same version. The build fails
  when they differ.
- The version is raised in the code, in a commit of its own (or of the release merge request),
  before tagging.

## 4. Tags and releases

- A release is a tag `v` + version (`v1.4.2`, `v2.0.0-rc.1`) on a commit of `main`. Pre-releases
  MAY be tagged on `develop`.
- The pipeline rejects a tag that is not a valid semantic version with the `v` prefix, or whose
  version differs from the one in the code.
- **Tags are immutable**: a published tag is never moved, deleted or reused. A broken release is
  fixed by a new PATCH version.
- The release build runs in the pinned environment of the container builds (same toolchain
  versions), so that the released artifact is the one the tests validated and can be rebuilt.

## 5. Artifacts and publication

### 5.1 Platforms

Every release provides, unless a program states otherwise:

| Artifact                     | Platforms                                                        |
|------------------------------|------------------------------------------------------------------|
| archive                      | `linux-amd64`, `linux-arm64`, `windows-x64`, `windows-arm64`, `macos-arm64` |
| Debian package (`.deb`)      | `amd64`, `arm64`                                                 |
| container image              | `linux/amd64`, `linux/arm64`, as one multi-architecture image    |

- Each platform is built **natively** on a runner of that operating system and architecture (no
  emulation, no cross-compilation), and its tests run there before the build.
- Linux binaries come from the container build (the `artifact` stage, see
  [`containers.md`](containers.md)), so that they use the pinned toolchains and the base image's C
  library. macOS and Windows binaries are built with the same pinned toolchain versions.

### 5.2 Archives

- One archive per platform, `<program>-v<version>-<platform>.tar.gz` (`.zip` for Windows),
  holding one folder of the same name with exactly: the binary (stripped, free of build-machine
  paths, see [`rust.md`](rust.md) §8.3), the default configuration file, the README and the
  licence.
- The default configuration file is the one the program writes with its defaults
  (`--config-dump`), kept in the repository and checked by a test against the defaults, so that
  it never drifts.

### 5.3 Debian packages

A program that runs as a service is packaged for Debian (and derivatives) following the Debian
Policy and the FHS:

- **Files**: the binary in `/usr/bin/<program>`, the configuration in `/etc/<program>/`, the
  systemd unit in `/usr/lib/systemd/system/<program>.service`, documentation in
  `/usr/share/doc/<program>/` (README compressed, machine-readable `copyright` pointing to
  `/usr/share/common-licenses/`, `changelog.Debian.gz`). Data, if any, in `/var/lib/<program>/`;
  logs go to the journal.
- **Configuration** files under `/etc` are **conffiles**: an upgrade never overwrites local
  changes, `remove` keeps them, `purge` deletes them. A file that may hold credentials is
  `root:<program> 0640`, set by `postinst` unless the administrator overrode it with
  `dpkg-statoverride`.
- **A dedicated system user and group** named after the program, with no home and no login shell,
  created by `postinst` (`adduser --system`) when missing, and deleted by `postrm purge` (not by
  `remove`, which keeps everything needed to reinstall).
- **The systemd service** runs as that user, never as root, and is enabled and started on install
  and restarted on upgrade, with the maintainer-script logic of `dh_installsystemd`
  (`deb-systemd-helper`, `deb-systemd-invoke`): stopped in `prerm remove`, masked on `remove`,
  its state purged on `purge`. The unit is hardened (no capabilities, `NoNewPrivileges`,
  `ProtectSystem=strict`, private `/tmp` and devices, restricted address families and system
  calls) and restarts on failure.
- **Versions**: the Debian version is the semantic version with `~` before a pre-release and a
  packaging revision (`1.2.0-rc.1` becomes `1.2.0~rc.1-1`), so that pre-releases sort first.
- **Dependencies** are derived from the binary (for example the newest C library symbol version
  it needs), plus `adduser` and `init-system-helpers`; services it talks to (a database) are
  `Recommends`.
- Packages pass **lintian** with no error and no warning; a deliberate exception is a lintian
  override with its reason.

### 5.4 Container image and publication

- The image is built natively per architecture, pushed by digest, then published as one
  multi-architecture image under all its tags.
- **Image tags**: always the full version `X.Y.Z` (or `X.Y.Z-rc.1`); for stable versions also
  `X.Y`, `X` (not while MAJOR is 0) and `latest`. Pre-releases never move `X.Y`, `X` or `latest`.
- Images carry the OCI labels for title, source, version, revision and licence
  (`org.opencontainers.image.licenses` as an SPDX expression, for example `GPL-3.0-or-later`).
- **All or nothing**: tags, images and the release are published only after every artifact of
  every platform built; a failure on one platform publishes nothing.
- The release on the hosting platform (for example a GitHub release) is named after the tag, has
  generated release notes, the archives and packages, and one `SHA256SUMS` file covering them,
  and is marked as a pre-release for pre-release versions.

## 6. Pipeline rules

- **One pipeline definition** in the repository (for example `.github/workflows/ci.yml`), with a
  comment at the top saying what runs on which event.
- **Least privilege**: the default token permission is read-only; only the job that publishes a
  release gets write rights, and only those it needs.
- **Secrets** (registry tokens, signing keys) live only in the CI secret store. They are never
  written in files, command lines or logs, and are used only by publication jobs, so merge
  requests from forks never see them.
- **Pinned toolchains**: the same exact versions as the container builds (Rust, Node.js, ...),
  raised together. Dependencies are installed from the lock files (`cargo … --locked`, `npm ci`).
- **Third-party actions** are pinned at least to a major version (`actions/checkout@v5`); actions
  from publishers other than the platform and the tool vendors SHOULD be pinned to a commit SHA.
- **Concurrency**: a newer commit cancels the pipeline of an older one on the same branch or merge
  request; a release run is never cancelled.
- **Caches** (dependencies, build layers) speed up runs but never change results; a run with an
  empty cache produces the same artifacts.
- Pipelines fail fast and loudly: every check is an error, not a warning, and the failing step
  says what to do (for example the command that aligns the versions).
