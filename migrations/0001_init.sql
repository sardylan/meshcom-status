-- Every reception of a frame by the node.
-- Times are `timestamp` (without time zone) holding UTC. Primary keys are random UUIDs; rows are
-- ordered by time, with the key as a tie-breaker.
CREATE TABLE receptions (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    received_at TIMESTAMP   NOT NULL,
    source      TEXT        NOT NULL CHECK (source IN ('console', 'kiss')),
    -- MeshCom message ID (u32); the same for every copy of one frame. NULL over KISS.
    msg_id      BIGINT,
    -- 1 for the first reception of a msg_id within an hour, 2 for the next, ...
    copy        INTEGER     NOT NULL DEFAULT 1,
    origin      TEXT        NOT NULL,
    relays      TEXT[]      NOT NULL,
    -- Transmission chain: origin, relays, and the own node when it measured the signal.
    path        TEXT[]      NOT NULL,
    rssi        SMALLINT,
    snr         SMALLINT,
    hw          SMALLINT,
    firmware    TEXT,
    kind        TEXT        NOT NULL CHECK (kind IN ('message', 'position', 'hey', 'other')),
    dest        TEXT,
    text        TEXT,
    lat         DOUBLE PRECISION,
    lon         DOUBLE PRECISION,
    alt_m       DOUBLE PRECISION,
    neighbours  INTEGER,
    -- HEY hop reports: [{"hw": 3, "rssi": -109, "snr": -7}, ...]
    hops        JSONB,
    type_char   TEXT
);

CREATE INDEX receptions_received_at ON receptions (received_at DESC, id DESC);
CREATE INDEX receptions_kind_received_at ON receptions (kind, received_at DESC, id DESC);
CREATE INDEX receptions_msg_id ON receptions (msg_id, received_at) WHERE msg_id IS NOT NULL;

-- The latest known state of every node. The callsign is a natural key: unique, not primary.
CREATE TABLE nodes (
    id        UUID             PRIMARY KEY DEFAULT gen_random_uuid(),
    call      TEXT             NOT NULL UNIQUE,
    own       BOOLEAN          NOT NULL DEFAULT FALSE,
    lat       DOUBLE PRECISION,
    lon       DOUBLE PRECISION,
    alt_m     DOUBLE PRECISION,
    hw        SMALLINT,
    firmware  TEXT,
    frames    BIGINT           NOT NULL DEFAULT 0,
    last_seen TIMESTAMP        NOT NULL
);
