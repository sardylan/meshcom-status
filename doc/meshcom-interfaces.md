# MeshCom node interfaces

What a MeshCom node exposes on the network, and what the status server can use. Based on the
firmware source ([icssw-org/MeshCom-Firmware](https://github.com/icssw-org/MeshCom-Firmware),
HEAD of 2026-11-14) and on a live check of a test node running `v4.40a.11.14` (2026-11-18).

Firmware references:

- `docs/kiss_tcp_protocol.md` — KISS/TCP wire contract.
- `src/web_functions/Web-API_documentation.txt` — HTTP Web-API.
- `docs/ext_udp_telemetry.md` — ext-udp inbound `tele` message; the outbound feed is only in
  `src/extudp_functions.cpp`, `src/extern_msg_json.h`, `src/extern_tele_json.h`.

Four interfaces are in scope: **KISS/TCP on 8001**, **HTTP on 80**, **ext-udp on UDP 1799** and
the **net console on TCP 2323** (debug output).
HTTP is used only through dedicated JSON endpoints; HTML pages are never scraped.

## 1. Node prerequisites

| Setting      | Needed value | Why                                                             |
|--------------|--------------|-----------------------------------------------------------------|
| `kiss`       | `on`         | KISS server on port 8001                                        |
| `kissmeta`   | `on`         | RSSI/SNR frames after each received frame                       |
| `kisstx`     | any          | Not needed: the status server only listens                      |
| `kissauth`   | any          | Optional; HMAC-SHA256 handshake before KISS framing (§2.1)      |
| `--webpwd`   | any          | Optional; when set, the server must log in (§3.1)               |
| `extudp`     | `on`         | JSON feed over UDP (§4)                                         |
| `extudpip`   | server IP    | Where the node sends the UDP feed                               |
| `netconsole` | `on`         | Net console on TCP 2323 (§4b)                                   |
| `--loradebug`| `on`         | Frame lines on the net console (§4b)                            |
| WiFi         | STA mode     | KISS does not start on Ethernet-only boards or in WiFi-AP mode  |

## 2. KISS/TCP — port 8001

### 2.1 Connection

- Plain TCP, fixed port 8001. **A single client at a time**: while the status server is
  connected, no other KISS client (Dire Wolf, YAAC, WebDesk) can connect, and vice versa.
- Optional auth (`kissauth on`): node sends `NONCE: <32 hex>\r\n`, client answers
  `HMAC-SHA256(passwd, nonce)` as 64 hex + `\r\n`, node answers `OK` or `FAIL` (15 s deadline).
- On close / reset / read error: reconnect with backoff. The node drops half-open peers and
  rebuilds the server on WiFi loss.

### 2.2 Frames (node → client)

Standard KISS framing (`FEND 0xC0`, `FESC 0xDB`, `TFEND 0xDC`, `TFESC 0xDD`). Type byte =
`(port << 4) | 0`.

| Type   | Content                                                                              |
|--------|--------------------------------------------------------------------------------------|
| `0x00` | AX.25 UI frame without FCS: `dest`, `src`, 0..8 digipeaters (H-bit set), `0x03 0xF0`, info |
| `0x10` | RxMeta, **after** its `0x00`: `snr: i8` dB, `rssi: i16 LE` dBm                       |
| `0x20` | SrcInfo, **before** its `0x00`: full origin call in ASCII (e.g. `QO0NOV-99`)         |
| `0xF0` | TX result of a client-sent frame (not used: the server does not transmit)            |

- `dest` is the node's tocall (`APRSMC`), meaningless.
- `src` is the origin. SSIDs above 15 are clamped to `-15`; when that happens a `0x20` frame
  carries the real call, and it MUST replace `src`.
- Digipeaters are the mesh relay nodes, i.e. the path.
- RxMeta `snr=0, rssi=99` means "no RF measurement": the frame came through the MeshCom
  server/gateway, or was originated by the node itself.

### 2.3 Info field

Only two kinds of payload are forwarded:

- **Position** — starts with `!`: `!DDMM.mmN/DDDMM.mmE<sym><comment>`. The comment carries the
  MeshCom extensions: `/B=` battery %, `/A=` altitude (ft), `/N` neighbour count,
  `/P= /H= /T=` pressure/humidity/temperature, `/R=` relay/group list.
- **Message** — starts with `:`: `:ADDRESSEE:text`, addressee 9 chars space-padded.
  - Group: addressee is the group number (e.g. `123`).
  - Broadcast: `*` or **9 spaces** (seen live, not mentioned in the firmware doc).
  - A MeshCom ACK arrives as a message `:CALL     :ackNN`.

Text is **UTF-8** bytes; decode as UTF-8, not Latin-1.

### 2.4 Sample capture (live, 4 minutes, 8 frames)

```
QF0ECH-10>APRSMC,QN0MKE-11*,QB0ALF-12*:!1009.36N/03001.05W#/B=100/A=000039/R=123;12302;12399;9;
  meta snr 7 rssi -107
  srcinfo QJ0IND-99
QJ0IND-15>APRSMC,QB0ALF-12*::123      :... (via BOT)
  meta snr 6 rssi -108
QM0LMA-1>APRSMC,QB0ALF-12*::         :good morning ok
  meta snr 7 rssi -107
```

### 2.5 Not available over KISS

- **HEY beacons** and **telemetry-only frames**: a node that only sends HEY never shows up.
- Traffic the node only **digipeats** for others.
- Sender **hardware / firmware** and **structured telemetry** (only the raw comment text).
- **Origin signal**: RxMeta is the signal of the **last hop** that reached the radio, not of the
  origin.
- **Timestamps** and **message IDs**: the server must timestamp each frame on arrival.

## 3. HTTP — port 80

### 3.1 Authentication

Without `--webpwd` every endpoint is open. With it, every request except the login ones needs
the `mcs` session cookie:

1. `GET /?loginnonce` → 32 hex chars (`text/html`, valid 30 s).
2. `GET /?login=<nonce>.<hex HMAC-SHA256(key = password, msg = nonce)>` → `200` +
   `Set-Cookie: mcs=<token>`, or `403 {"ok":false,"err":"..."}`.
3. Session expires after 4 h without requests. 5 failed logins in a minute block logins for 60 s.

Sending `X-MC: 1` on every request is harmless and needed on builds with `MC_WEB_GUARD=1`.

### 3.2 JSON endpoints

| Endpoint                 | Content                                                               | Status |
|--------------------------|-----------------------------------------------------------------------|--------|
| `GET /getparam/?<name>`  | `{"returncode":0,"<name>":"<value>"}`, one setting per call. `422` + `returncode 2` if unknown | documented |
| `GET /config.json`       | Whole configuration in one object: `fw`, `hw`, callsign, position, groups, radio (`node_freq`, `node_bw`, `node_sf`, `node_cr`), ... **includes secrets** | undocumented |
| `GET /rmheard`           | `{"heard":[{"call","hw","age_s","rssi"}]}`                             | undocumented, internal |
| `GET /austatus`          | Firmware `run` / `avail`, uptime `up` (s), free heap `ifree`, update state (ESP32 only) | undocumented, internal |
| `GET /rmstatus`          | Remote-management state                                               | not useful |

`/rmheard` limits: at most **12** entries, last **3 hours**, age in **whole minutes**, `rssi`
may be `null`, no SNR, no position, only calls addressable by remote management.

`/getparam` names that return `returncode 2` on `v4.40a.11.14`: `chargeled`, `manualcommand`,
`netmode`, `sendpos`, `small`, `store*`, `tempoffsetoutdoor`.

### 3.3 Not available as an API

Only on HTML pages (not scraped):

- message history (`/?getmessages`, `/?page=messages`);
- full heard list (`/?page=mheard`), positions, paths, rx log, weather;
- the node's own telemetry (battery, voltage, sensors) and radio counters.

### 3.4 Protocol quirks

- The firmware itself calls the Web-API "not yet finished or stable". `/config.json`,
  `/rmheard` and `/austatus` are internal to the web UI and may change without notice.
- Status line ends with a trailing space and a bare `\n`; other headers use `\r\n`.
- `Content-type:application/json` (no space after the colon).
- No `Content-Length`: the body ends when the connection closes (`Connection: close`).
- GET only; one connection served at a time.

The HTTP client used by the server must be checked against these quirks.

## 4. ext-udp — UDP port 1799

### 4.1 Configuration

- Node: `extudp on`, `extudpip <status server IP>`. Readable with `/getparam/?extudp` and
  `/getparam/?extudpip` (`node_extern` in `/config.json`).
- The node sends from `<node-ip>:1799` to `<extudpip>:1799`, one JSON object per datagram
  (UTF-8, no framing, no handshake). The server binds UDP 1799.
- Not sent in WiFi-AP mode. Unlike KISS, it also works on Ethernet, and there is no single-client
  limit (the node only knows one target IP; a broadcast address is also accepted).
- Verified live on 2026-11-18: 5 datagrams in 5 minutes.

### 4.2 Datagrams (node → server)

Common fields: `src_type` is `lora` (heard over RF), `udp` (relayed by the MeshCom server) or
`node` (originated by the node itself). `src` is the **origin followed by the relay path**,
comma-separated (e.g. `"QC0BRV-10,QB0ALF-12"`), with the **real SSID** (no `-15` clamp).

**`type: "msg"`** — text message:

```json
{"src_type":"lora","type":"msg","src":"QE0DLT-22,QB0ALF-12","dst":"*","msg":"{UTC}2026-11-18 10:55:08",
 "msg_id":"6AFCB821","firmware":0,"fw_sub":"#","rssi":-104,"snr":5,"hw_id":0,"lora_mod":8,"max_hop":2}
```

**`type: "pos"`** — position beacon:

```json
{"src_type":"lora","type":"pos","src":"QC0BRV-10,QB0ALF-12","msg":"","lat":10.37,"lat_dir":"N",
 "long":30.1748,"long_dir":"W","aprs_symbol":"#","aprs_symbol_group":"/","hw_id":3,"msg_id":"45982118",
 "alt":715,"lora_mod":8,"max_hop":1,"batt":100,"firmware":40,"fw_sub":"a","rssi":-108,"snr":6}
```

**`type: "tele"`** — sent right after a `pos` that carries sensor values:

```json
{"src_type":"lora","type":"tele","src":"QC0BRV-10,QB0ALF-12","batt":100,"temp1":21.8,"temp2":0,
 "hum":63,"qfe":987.5,"qnh":1012.6,"pressure_alt":306,"gas":0,"co2":0}
```

**`type: "ack"`** — delivery status of a message: `{"type":"ack","msg_id":"...","status":N,
"from":"...","via":"..."}` (`from` optional).

Field notes:

- `msg_id`: 8 hex digits, the MeshCom message ID. Usable for de-duplication and to match
  `ack`s and KISS TX results.
- `alt`: the `/A=` value of the beacon, i.e. **feet**. `pressure_alt` is computed from pressure.
- `lat` / `long` are unsigned; the sign comes from `lat_dir` / `long_dir`.
- `hw_id`: hardware type number (names as in `/rmheard`'s `hw`); `firmware` + `fw_sub`: sender
  firmware (e.g. `40` + `a` = 4.40a). Both `0` for server-generated traffic (e.g. time beacons).
- `rssi` / `snr`: as with KISS, the **last hop**, not the origin.
- `tele` values of `0` mean "no sensor", not a reading of zero.

### 4.3 Limits

- **Only positions (`!`) and messages (`:`)**, plus `tele` and `ack`. HEY frames are **not**
  sent: `sendExtern()` returns early for any other payload type. This contradicts the firmware's
  `kiss_tcp_protocol.md` §6, which says ext-udp carries HEY.
- **No full position comment**: `pos.msg` is empty, so `/N` (neighbour count) and `/R=` (relay
  list) are only available over KISS.
- **No timestamp**: the server timestamps on arrival.
- **Lossy**: plain UDP with no sequence number, and the node's outgoing queue holds only **2**
  entries and overwrites the oldest, so bursts can be lost silently.

## 4b. Net console with LoRa debug — TCP port 2323

The only way to get **every** received frame (HEY beacons, duplicates, relayed traffic, CRC
errors) without changing the firmware. It is debug output, **not an API**: the format may change
in any release.

### 4b.1 Configuration

- `/setparam/?netconsole=on` — starts the console server on TCP 2323 (ESP32, WiFi or Ethernet).
- `/setparam/?manualcommand=--loradebug%20on` — turns on LoRa debug output (persisted).
- Optional `--txcapture on` adds raw hex of the node's own transmissions.
- Plain text over TCP, **single client**. No auth when `--passwd` is empty; otherwise an
  HMAC-SHA256 nonce handshake (as for KISS). The stream starts with `OK`, `MeshCom Console`.
- Verified live on 2026-11-18: 746 lines (≈40 KB) in 5 minutes, 16 received frames.

### 4b.2 Useful lines

The stream mixes many line kinds; the server keeps only these, by prefix:

| Line                                         | Meaning                                                    |
|----------------------------------------------|------------------------------------------------------------|
| `HH:MM:SS MH-LoRa <len> <type> x<msg_id> ...`| **Every** received frame, decoded, duplicates included     |
| `HH:MM:SS RX-LoRa2 ...`                      | Same, only the first copy (passed de-duplication)          |
| `HH:MM:SS TX-LoRa ...`                       | Frame transmitted by the node (own or relayed)             |
| `[MC-TEST] RX_FRAME len=<n> rssi=<dBm> snr=<dB> hex=<bytes>` | Raw bytes of every received frame, with its own signal |
| `[MC-DBG] CRC_ERROR rssi= snr= freq_err= size=` + `CRC_PAYLOAD[n]: ...` | Frame lost to a CRC error            |
| `[MC-DBG] RX_DEDUP_NEW` / `RX_DEDUP_DUP msg_id=`  | De-duplication decision                               |
| `[MC-DBG] CHANNEL_UTIL rx=<ms> tx=<ms> util=<%>`  | Channel utilisation                                   |
| `[MC-TEST] CAPTURE_DROPPED n=<count>`        | Raw frames lost because the capture ring was full          |
| `[WIFI];link;up;rssi;<dBm>;...`              | The node's own WiFi link                                   |

Decoded frame line, fields after the time:

```
MH-LoRa 070 @ x4598211D H02 S1 T0 M01 QC0BRV-10,QI0HTL-12,QB0ALF-12>HG@R7;3,104,-4;12,121,-6; HW:03 MOD:8/8 FCS:0F0F FW:40:a LH:83
        len type msg_id hops  flags   origin,relay,...>dest + payload                       src hw   mod      checksum fw     last-hop hw
```

The `HH:MM:SS` is node local time; the server should still timestamp on arrival.

### 4b.3 Raw frame layout (`RX_FRAME hex`)

From `decodeAPRS()` (`src/aprs_functions.cpp`):

```
[type 1][msg_id 4 LE][hop/flags 1][origin,relay,... ASCII]'>'[dest ASCII][type char][payload ASCII]0x00
[src hw 1][mod 1][fcs 2 BE = byte sum of everything before][fw ...trailer]
```

- `type`: `0x3A` `:` message, `0x21` `!` position, `0x40` `@` HEY, `0x41` binary ACK.
- hop/flags: low nibble = remaining hops, `0x80` via server, `0x40` track, `0x20` app offline,
  `0x10` mesh.

### 4b.4 HEY payload

Seen live, e.g. `HG@R7;3,109,-7;3,137,-19;1,69,11;`:

- Destination `H` or `HG` (meaning of `G` not checked yet), then `@`.
- `R<n>;` — the number of neighbours the origin reports.
- Each relay appends `<hw>,<rssi>,<snr>;`: the hardware ID of the receiving node and the
  **RSSI (sign dropped) and SNR at which it heard the previous hop**. So a HEY carries the link
  quality of **every hop** of its path. This is the data needed for a topology map.

### 4b.5 Limits

- Debug output: not documented as an API, and noisy (~85% of lines are not useful).
- The raw capture ring is 768 bytes (≈9 beacons); bursts can drop raw frames
  (`CAPTURE_DROPPED`). The decoded `MH-LoRa` lines are printed separately.
- About 48 ms of serial time per line on the node; LoRa debug stays on after a reboot.
- Single client on 2323.

## 4c. Comparison

|                                 | KISS (8001)            | ext-udp (1799)              | HTTP (80)          | Net console (2323)      |
|---------------------------------|------------------------|-----------------------------|--------------------|-------------------------|
| Messages, positions             | ✅                     | ✅                          | ✗ (HTML only)      | ✅                      |
| Real SSID of the origin         | via `0x20` frame       | ✅                          | —                  | ✅                      |
| Relay path                      | ✅ digipeaters         | ✅ in `src`                 | —                  | ✅                      |
| RSSI / SNR (last hop)           | with `kissmeta on`     | ✅ always                   | `/rmheard` RSSI    | ✅ every copy + per hop (HEY) |
| Sender hardware / firmware      | ✗                      | ✅                          | `/rmheard` hw      | ✅                      |
| Structured telemetry            | ✗ (comment text)       | ✅ `tele`                   | ✗                  | ✗ (comment text)       |
| Full comment, `/N`, `/R=`       | ✅                     | ✗                           | ✗                  | ✅                      |
| `msg_id`                        | ✗                      | ✅                          | ✗                  | ✅                      |
| HEY beacons                     | ✗                      | ✗                           | ✗                  | ✅                      |
| Clients at a time               | 1                      | 1 target IP (or broadcast)  | many, serialised   | 1                       |
| Delivery                        | TCP, reliable          | UDP, lossy                  | request/response   | TCP, debug text            |

KISS, ext-udp and the net console can run at the same time.

## 5. Security findings (test node, 2026-11-18)

With no `--webpwd` set and the default `MC_WEB_GUARD=0` build:

- `/getparam/?setpwd` and `/config.json` (`node_lpwd`) return the **WiFi password** in clear text;
  `/config.json` also returns the **Bluetooth PIN** (`bt_code`).
- `/setparam/...` and `/callfunction/...` (e.g. `?reboot`) are open to anyone on the network.
- KISS with `kisstx on` and `kissauth off` lets anyone on the network transmit with the node's
  callsign.

ext-udp accepts inbound `msg` and `tele` datagrams from anyone who can reach the node, with no
sender check.

**Decision:** the node sits on a dedicated network, so the open interfaces are accepted as they
are. The status server still supports the HTTP login (§3.1) and the KISS handshake (§2.1), for
nodes where they are enabled.
