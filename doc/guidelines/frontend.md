# Front-end Guidelines

These rules are binding for every web page (HTML/JavaScript front end) served by a program in
this repository. Code review enforces them. The HTTP APIs and WebSockets the pages use follow
[`http.md`](http.md); a Rust program that embeds its front end follows [`rust.md`](rust.md) §9.5.

The key words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119.

## 1. The backend owns the data

- All data lives on the backend. The page does not keep its own copy of the data to filter,
  aggregate or hide parts of it: it asks the backend for exactly what it shows, through the API
  ([`http.md`](http.md)) or the WebSocket, and renders the answer.
- Filtering, sorting, paging and windowing ("last N items") are done by the backend. When the
  user changes them, the page sends the new choice and replaces what it shows with the answer.
- The page MAY hold what it is currently displaying (to draw it and to apply live updates), and
  purely visual state (selection, open panels, map position). Nothing else.

## 2. Live pages use a WebSocket

- A page that updates by itself gets its data through a WebSocket with dedicated messages
  ([`http.md`](http.md) §1: JSON, one object per text frame, with a `type`): a full snapshot
  when the client connects or changes what it asks for, then incremental updates. Polling the REST API on a timer is not
  used for live data.
- The client tells the server what it wants to see with a message on the same WebSocket; the
  server keeps that choice for the connection and sends only what matches it.
- The page reconnects by itself, with backoff, when the connection drops, and asks again for its
  current choice after reconnecting.
- The page shows a **connection indicator** for the WebSocket, always visible: at least
  connecting, connected and disconnected, distinguishable at a glance (colour and text).

## 3. How many items

- Every list or view of items that can grow without bound has a **selector for how many items are
  shown** (for example 10, 30, 100, 500, 1000, all), with a **default already selected**.
- The number is sent to the backend (§1), which returns only those items.

## 4. Version

- The application version is always visible on the page, in a small footer or a similar
  unobtrusive place. It is the version of the running backend, sent by the backend, not a value
  written into the page.

## 5. Display settings

- The user's display settings (how many items, filters, open tab, map position, ...) are saved
  in the browser's local storage and restored when the page opens, before the first request, so
  the first answer already matches them.
- Local storage is a per-browser convenience only: it never holds data from the backend, and the
  page works with the defaults when storage is unavailable (private browsing, blocked site data).
  Every access is guarded.
- The stored object has a versioned key (for example `<app>.settings.v1`); a value that is
  missing or not valid falls back to its default.

## 6. Front-end stack

- A complex front end (a portal with several views, client-side state, maps or charts) is a
  **React** application written in **TypeScript** and built with **Vite**. A simple page (one
  view, a few lines of script) MAY stay plain HTML and JavaScript.
- TypeScript runs in `strict` mode. The build fails on type errors (`tsc --noEmit` is part of
  `npm run build`).
- **npm only.** Every command is an npm script, run with the npm that ships with Node.js:
  `npm ci` to install, `npm run dev`, `npm run build`, `npm run lint`, `npm test`. Yarn, pnpm,
  bun and other package managers MUST NOT be used. `package-lock.json` is committed; installs in
  builds and CI use `npm ci`, never `npm install`.
- **Node.js version through NVM.** The project root of the front end has an `.nvmrc` with the
  exact Node.js version (an LTS release); developers run `nvm use`, and builds use the same
  version. Raise it deliberately, like the Rust toolchain.
- The production build goes to `dist/`, which is never committed.

## 7. Maps

- Maps use **MapLibre GL JS** (in React through `react-map-gl`). No map library that needs an
  account or an API key unless a feature requires it.
- **Selectable layers.** The map offers:
  - base maps the user can switch between (for example streets, terrain, satellite), whose list
    (name, tile URL, attribution) comes from the backend configuration, with a sensible default
    list;
  - toggles for each overlay the application draws (for example lines, labels, highlights).

  The chosen base map and overlays are display settings (§5).
- **Controls.** The map always shows zoom in / zoom out buttons, plus a control to reset the
  bearing and pitch when they can change. Attributions required by the tile providers are shown.
