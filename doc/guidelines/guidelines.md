# Generic Documents

The files in this folder are **project-agnostic**. They are shared across several projects and
are copied as they are from one project to another.

## Rules for this folder

- Files here MUST NOT contain anything specific to the project they sit in: no project name, no
  product, domain or protocol names, no paths, crates, modules, settings or hosts that exist only
  in one project.
- Examples use neutral placeholders (`myapp`, `MYAPP_...`, `/users`, `[web]`, `example.com`).
- Files may refer to each other by relative path, but never to files outside this folder.
- A change made here should be made in the shared copy as well, so every project stays in sync.
  Do not change a file here to fit one project only.
- Project-specific rules, which add to or narrow these documents, go in the parent `doc/` folder
  (or in the project's `CLAUDE.md`), never in this folder.

## Contents

| File                             | Topic                                           |
|----------------------------------|-------------------------------------------------|
| [`guidelines.md`](guidelines.md) | this file: rules for this folder                |
| [`rust.md`](rust.md)             | Rust implementation rules                       |
| [`http.md`](http.md)             | HTTP API rules (JSON, REST, paths, JWT, errors, compression) |
| [`frontend.md`](frontend.md)     | Web pages: data on the backend, WebSocket, item selectors, version, display settings, React/Vite/TypeScript, npm, NVM, MapLibre maps |
| [`persistence.md`](persistence.md) | Storage choice: PostgreSQL, MongoDB, Redis      |
| [`containers.md`](containers.md) | Container images: all in `docker/`; `base`/`dev`/`prod` targets, Debian slim base, nothing from development in the release image; Docker Compose: one `compose.yml` holding as much as possible, `env.example`, `.env` not committed, bind volumes under `./volumes/` |
| [`cicd.md`](cicd.md)             | CI/CD and release: branches (`main`, `develop`), tests on merge requests and commits, release build and publication only on tags, Semantic Versioning, artifacts, pipeline rules |
