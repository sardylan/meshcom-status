# HTTP API Guidelines

These rules are binding for every HTTP API exposed by a program in this repository. Code review
enforces them. The web pages that use these APIs follow [`frontend.md`](frontend.md). They are language-agnostic; for Rust, the implementation rules (for example the use
of `axum`) are in [`rust.md`](rust.md).

The key words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119.

## 1. Payloads: JSON everywhere

- Every API request and response body is JSON (`Content-Type: application/json`). Error bodies
  too (§7).
- WebSocket messages are JSON as well: each message is one **text** frame holding one JSON object.
  Binary frames are not used.
- A WebSocket message SHOULD carry a `type` field naming its kind, so that both sides can dispatch
  on it without guessing from the shape of the payload.
- A request with a body of the wrong type is refused with `415 Unsupported Media Type`; a body that
  is not valid JSON, or does not match the expected shape, with `400 Bad Request` (or `422` when the
  JSON is well formed but its values are invalid).
- Field names use `snake_case`. Durations and sizes state their unit in the field name
  (`timeout_ms`, `size_bytes`).
- **Dates and times are ISO 8601 strings in UTC with the `Z` suffix** (the RFC 3339 profile),
  for example `2026-11-13T10:15:03Z` or `2026-11-13T10:15:03.123456Z`, in requests, responses,
  query parameters and WebSocket messages. Other offsets or local times are used only when a
  feature needs them, and are documented with that field. Never Unix numbers, never local time.
- Identifiers that are UUIDs are sent as lower-case strings in the canonical hyphenated form.
- Absent and `null` mean the same thing; a client MUST NOT need to tell them apart.

## 2. Stateless

The server is fully stateless towards its clients:

- Every request carries everything needed to serve it (including its credentials, §6). No
  server-side session, no session cookie, no state kept between two requests of the same client.
- Any instance of the service can serve any request, so instances can be added, removed or
  restarted freely and no sticky sessions are needed behind a load balancer.
- Persistent data lives in the program's storage (database, files, ...), never in the memory of a
  connection. A WebSocket connection MAY keep per-connection state (for example its
  subscriptions) only for its own lifetime; reconnecting re-sends whatever is needed.

## 3. RESTful, when applicable

Resource-oriented APIs follow REST:

- Paths name **resources**, as plural nouns: `/users`, `/users/{id}`, `/users/{id}/devices`. No
  verbs in paths for operations that map onto a method.
- Methods keep their HTTP meaning:

  | Method   | Use                                   | Safe | Idempotent |
  |----------|---------------------------------------|------|------------|
  | `GET`    | read a resource or a collection       | yes  | yes        |
  | `POST`   | create a resource in a collection     | no   | no         |
  | `PUT`    | replace a resource as a whole         | no   | yes        |
  | `PATCH`  | change part of a resource             | no   | no         |
  | `DELETE` | remove a resource                     | no   | yes        |

- `GET` never changes anything.
- A successful `POST` that creates a resource answers `201 Created` with a `Location` header and
  the new resource in the body.
- Filtering, sorting and paging use query parameters (`?limit=50&offset=100`), not path segments.

When an operation does not fit a resource (a command, a computation, a search with a large
query), a `POST` to an action path (`/devices/{id}/reboot`) is acceptable. Prefer a resource when
one exists.

## 4. Path layout

### 4.1 Check endpoints

Check endpoints report whether the service is working. They exist so that orchestrators, load
balancers and monitoring can probe the service without credentials.

- `/health` MUST always be present. Other check endpoints (for example `/ready`) MAY be added and
  follow the same rules.
- They live at the root of the server: not under the API prefix, not versioned, not
  authenticated.
- They answer `GET` (and `HEAD`) with **only** these status codes:
  - `204 No Content` when the check passes;
  - `500 Internal Server Error` when it fails.
- They have no body, so they are the one exception to §1. The reason for a failure is logged, not
  returned.
- They are cheap: they MUST NOT do slow or heavy work on every call, and MUST answer within a
  short, bounded time.

### 4.2 API prefix

All other endpoints live under a common prefix, `/api` by default. The prefix is a setting of the
program, so the service can be placed behind a reverse proxy under another path.

Under the prefix, endpoints are split by **audience**, not by authentication:

- `/api/public/...` — APIs meant for external clients (browsers, apps, third parties), whether or
  not they require authentication.
- `/api/private/...` — internal APIs meant for service-to-service calls, whether or not they
  require authentication. They are not meant to be reachable from outside; deployments SHOULD
  block them at the edge (reverse proxy, firewall), but the service MUST NOT rely on that alone
  for its security.

### 4.3 Versions

Every API is versioned in the path, right after the audience segment. Versions are `v` followed by
an integer, starting from `v1`:

```
/api/public/v1/users
/api/private/v1/metrics
```

- A version changes only for a **breaking** change: removing or renaming a field or endpoint,
  changing a type or a meaning, making an optional input required.
- Adding endpoints, optional request fields or response fields is not breaking. Clients MUST
  ignore response fields they do not know.
- When a new version is introduced, the previous one keeps working for a documented transition
  period before it is removed.
- Versions apply per audience: `public` and `private` evolve independently.

## 5. Status codes

Use the code that describes the outcome; do not answer `200` with an error in the body.

| Code  | When                                                                      |
|-------|---------------------------------------------------------------------------|
| `200` | success, with a body                                                      |
| `201` | resource created (§3)                                                     |
| `204` | success, no body                                                          |
| `400` | malformed request (syntax, missing parameter)                             |
| `401` | missing or invalid credentials (§6)                                       |
| `403` | valid credentials, but not allowed                                        |
| `404` | resource not found                                                        |
| `409` | conflict with the current state of the resource                           |
| `415` | request body is not JSON                                                  |
| `422` | well-formed request with invalid values                                   |
| `429` | rate limit exceeded                                                       |
| `500` | unexpected server failure                                                 |
| `503` | a dependency is temporarily unavailable                                   |

## 6. Authentication: JWT

Authenticated APIs use **JSON Web Tokens** (RFC 7519).

- The client sends the token on every request as `Authorization: Bearer <token>`. Tokens are not
  sent in query strings (they would end up in logs) and not in cookies (§2).
- WebSocket connections are authenticated once, at the upgrade request, with the same header. When
  a client cannot set headers, the first message after the upgrade carries the token instead, and
  the server closes the connection if it is missing or invalid.
- The server validates every token fully before trusting any claim:
  - the signature, with a key and **algorithm fixed by the server's configuration**. Tokens with
    `alg: none`, or with an algorithm other than the configured one, are refused;
  - `exp` (required) and `nbf` (when present), with a small, configured clock skew;
  - `iss` and `aud`, against configured values.
- A missing, malformed, expired or otherwise invalid token gives `401 Unauthorized` with a
  `WWW-Authenticate: Bearer` header. A valid token without the needed permissions gives
  `403 Forbidden`.
- Permissions are read from the token's claims (for example `sub` and a `scope` or `roles`
  claim); the server keeps no session for them (§2).
- Tokens are short-lived. Keys and secrets come from the configuration, never from the source code.
- Tokens, keys and secrets are never logged, not even partially, and never echoed in error bodies.

## 7. Errors

Error responses use the Problem Details format (RFC 9457), with
`Content-Type: application/problem+json`:

```json
{
  "type": "about:blank",
  "title": "Not Found",
  "status": 404,
  "detail": "device 42 does not exist"
}
```

- `status` repeats the HTTP status code; `title` is its short, fixed description; `detail`
  explains this occurrence in words a client developer can act on.
- Extra members MAY be added for machine-readable details (for example `errors` listing the
  invalid fields of a `422`).
- A `5xx` body MUST NOT expose internals (stack traces, SQL, file paths, upstream addresses). Those
  go to the logs.

## 8. Logging

- Every request is logged once, when it completes, with method, path, status code and duration, in
  an access log separate from the rest of the program's logs.
- Check endpoints (§4.1) are logged at a lower level than other requests, so that frequent probes
  do not flood the logs.
- Query strings, headers and bodies are logged only when they cannot contain credentials or
  personal data.

## 9. Compression

Responses are compressed with the content codings of RFC 9110 §8.4, chosen from the request's
`Accept-Encoding` (RFC 9110 §12.5.3).

### 9.1 Codings and negotiation

- The codings offered are `br` (Brotli), `zstd` (RFC 9659), `gzip`, and `identity` (no
  compression). `deflate` and `compress` are not offered; `x-gzip` is understood as `gzip`.
- The coding sent is the one with the highest weight (`q`) in `Accept-Encoding`. `q=0` excludes a
  coding; `*` gives its weight to every coding not listed. On equal weights the smallest output
  wins: `br`, then `zstd`, then `gzip`, then `identity`.
- `identity` is acceptable unless the client excludes it (`identity;q=0`, or `*;q=0` without
  `identity`), but when the client does not list it, it is only the fallback: any coding the
  client lists wins over it. Without `Accept-Encoding`, the response is not compressed.
- When nothing the server offers is acceptable, the response is sent uncompressed rather than
  refused with `406`: every client can read it.

### 9.2 Headers

- A compressed response carries `Content-Encoding` with the coding used. `Content-Type` stays the
  type of the content itself.
- Every response whose body can differ by coding carries `Vary: Accept-Encoding`, the uncompressed
  ones too, so that shared caches keep the forms apart.
- An `ETag`, if used, MUST differ between codings of the same content (for example a suffix per
  coding), since the bytes differ.

### 9.3 What is compressed

- Only types that compress: text (`text/*`), JavaScript, JSON, XML, SVG, WebAssembly. Files already
  compressed (images other than SVG, fonts such as WOFF2, audio, video, archives) are sent as they
  are, with no `Content-Encoding`.
- **Static files** (the web front end) are compressed at the **maximum level** of each coding
  (Brotli 11, zstd 19, gzip 9), **once**: stored compressed, or computed on the first request and
  kept for the life of the process. They are never compressed again on each request.
- `zstd` uses a window of at most 8 MiB, the largest that HTTP clients are required to decode
  (RFC 9659 §3). Brotli uses at most the standard 24-bit window.
- **Dynamic responses** (API JSON) are not compressed by default. A program MAY compress large
  ones on the fly at a fast level, and MUST NOT compress a response that mixes a secret (a token,
  a CSRF value) with content an attacker can influence (BREACH).
- WebSocket messages are not compressed by default; `permessage-deflate` (RFC 7692) MAY be enabled
  where messages are large.
- Request bodies are not compressed. A request with a `Content-Encoding` the endpoint does not
  document is refused with `415 Unsupported Media Type`.
