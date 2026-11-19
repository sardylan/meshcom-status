mod rows;

use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use crate::chat::{self, ChatMessage, Target, TargetInfo};
use crate::errors::{Error, Result};
use crate::frame::{Call, Frame, Reception};
use crate::graph::Node;
use crate::stats::{self, Range, Stats};
use crate::view::View;

macro_rules! reception_columns {
    () => {
        "id, (extract(epoch FROM received_at) * 1000000)::int8 AS at_us, source, msg_id, copy, origin, relays, \
         path, rssi, snr, hw, firmware, kind, dest, text, lat, lon, alt_m, neighbours, hops, type_char"
    };
}

const INSERT_RECEPTION: &str = "\
    INSERT INTO receptions (received_at, source, msg_id, copy, origin, relays, path, rssi, snr, hw, firmware, \
        kind, dest, text, lat, lon, alt_m, neighbours, hops, type_char) \
    VALUES (timestamp 'epoch' + $1::float8 * interval '1 microsecond', $2, $3, \
        CASE WHEN $3::int8 IS NULL THEN 1 ELSE 1 + (SELECT count(*) FROM receptions r WHERE r.msg_id = $3 \
            AND r.received_at > timestamp 'epoch' + $1::float8 * interval '1 microsecond' - interval '1 hour' \
            AND r.received_at <= timestamp 'epoch' + $1::float8 * interval '1 microsecond')::int4 END, \
        $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19) \
    RETURNING id, copy";

const SELECT_RECEPTIONS: &str = concat!(
    "SELECT * FROM ( SELECT ",
    reception_columns!(),
    " FROM receptions WHERE kind = ANY($1) AND ($2 OR copy = 1) \
        ORDER BY received_at DESC, id DESC LIMIT $3 \
    ) AS w ORDER BY at_us, id"
);

const UPSERT_NODE: &str = "\
    INSERT INTO nodes (last_seen, call, own, lat, lon, alt_m, hw, firmware, frames) \
    VALUES (timestamp 'epoch' + $1::float8 * interval '1 microsecond', $2, $3, $4, $5, $6, $7, $8, $9) \
    ON CONFLICT (call) DO UPDATE SET last_seen = EXCLUDED.last_seen, own = EXCLUDED.own, lat = EXCLUDED.lat, \
        lon = EXCLUDED.lon, alt_m = EXCLUDED.alt_m, hw = EXCLUDED.hw, firmware = EXCLUDED.firmware, \
        frames = EXCLUDED.frames";

const STATS_TOTALS: &str = "\
    SELECT count(*) FILTER (WHERE copy = 1)::int8 AS frames, count(*)::int8 AS receptions, \
        count(*) FILTER (WHERE copy = 1 AND kind = 'message' \
            AND btrim(coalesce(text, '')) !~ '^(ack|rej)[[:alnum:]]{1,5}$')::int8 AS messages, \
        count(*) FILTER (WHERE copy = 1 AND cardinality(relays) = 0)::int8 AS direct, \
        (extract(epoch FROM min(received_at)) * 1000000)::int8 AS first_us \
    FROM receptions \
    WHERE $1::float8 IS NULL OR received_at >= timestamp 'epoch' + $1::float8 * interval '1 microsecond'";

const STATS_KINDS: &str = "\
    SELECT kind, count(*)::int8 FROM receptions \
    WHERE copy = 1 AND ($1::float8 IS NULL OR received_at >= timestamp 'epoch' + $1::float8 * interval '1 microsecond') \
    GROUP BY kind";

const STATS_TIMELINE: &str = "\
    WITH counted AS ( \
        SELECT date_trunc($3, received_at AT TIME ZONE 'UTC' AT TIME ZONE $2) AS local, kind, count(*) AS n \
        FROM receptions \
        WHERE copy = 1 AND ($1::float8 IS NULL OR received_at >= timestamp 'epoch' + $1::float8 * interval '1 microsecond') \
        GROUP BY 1, 2 \
    ), buckets AS ( \
        SELECT generate_series( \
            date_trunc($3, coalesce(timestamp 'epoch' + $1::float8 * interval '1 microsecond', \
                (SELECT min(received_at) FROM receptions)) AT TIME ZONE 'UTC' AT TIME ZONE $2), \
            date_trunc($3, (timestamp 'epoch' + $4::float8 * interval '1 microsecond') AT TIME ZONE 'UTC' AT TIME ZONE $2), \
            ('1 ' || $3)::interval) AS local \
    ) \
    SELECT (extract(epoch FROM b.local AT TIME ZONE $2) * 1000000)::int8 AS start_us, \
        coalesce(sum(c.n) FILTER (WHERE c.kind = 'message'), 0)::int8 AS message, \
        coalesce(sum(c.n) FILTER (WHERE c.kind = 'position'), 0)::int8 AS position, \
        coalesce(sum(c.n) FILTER (WHERE c.kind = 'hey'), 0)::int8 AS hey, \
        coalesce(sum(c.n) FILTER (WHERE c.kind NOT IN ('message', 'position', 'hey')), 0)::int8 AS other \
    FROM buckets b LEFT JOIN counted c ON c.local = b.local \
    GROUP BY b.local ORDER BY b.local";

const STATS_HOUR_OF_DAY: &str = "\
    SELECT extract(hour FROM received_at AT TIME ZONE 'UTC' AT TIME ZONE $2)::int4, count(*)::int8 \
    FROM receptions \
    WHERE copy = 1 AND ($1::float8 IS NULL OR received_at >= timestamp 'epoch' + $1::float8 * interval '1 microsecond') \
    GROUP BY 1";

const STATS_ORIGINS: &str = "\
    SELECT origin, count(*)::int8 AS n, (count(*) OVER ())::int8 AS distinct_count \
    FROM receptions \
    WHERE copy = 1 AND ($1::float8 IS NULL OR received_at >= timestamp 'epoch' + $1::float8 * interval '1 microsecond') \
    GROUP BY origin ORDER BY n DESC, origin LIMIT $2";

const STATS_RELAYS: &str = "\
    SELECT relay, count(*)::int8 AS n, (count(*) OVER ())::int8 AS distinct_count \
    FROM receptions, unnest(relays) AS relay \
    WHERE copy = 1 AND ($1::float8 IS NULL OR received_at >= timestamp 'epoch' + $1::float8 * interval '1 microsecond') \
    GROUP BY relay ORDER BY n DESC, relay LIMIT $2";

const STATS_HOPS: &str = "\
    SELECT cardinality(relays)::int4, count(*)::int8 FROM receptions \
    WHERE copy = 1 AND ($1::float8 IS NULL OR received_at >= timestamp 'epoch' + $1::float8 * interval '1 microsecond') \
    GROUP BY 1";

const STATS_SNR: &str = "\
    SELECT snr::int2, count(*)::int8 FROM receptions \
    WHERE snr IS NOT NULL \
      AND ($1::float8 IS NULL OR received_at >= timestamp 'epoch' + $1::float8 * interval '1 microsecond') \
    GROUP BY 1 ORDER BY 1";

const STATS_CHAT: &str = "\
    SELECT coalesce(dest, ''), count(*)::int8 AS n FROM receptions \
    WHERE kind = 'message' AND copy = 1 AND btrim(coalesce(text, '')) !~ '^(ack|rej)[[:alnum:]]{1,5}$' \
      AND ($1::float8 IS NULL OR received_at >= timestamp 'epoch' + $1::float8 * interval '1 microsecond') \
    GROUP BY 1 ORDER BY n DESC, 1 LIMIT $2";

const TIME_ZONE_EXISTS: &str = "SELECT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = $1)";

const SELECT_NODES: &str = "\
    SELECT call, own, lat, lon, alt_m, hw, firmware, frames, \
        (extract(epoch FROM last_seen) * 1000000)::int8 AS last_seen_us \
    FROM nodes ORDER BY call";

pub(crate) fn redact(url: &str) -> String {
    let Some(scheme_end) = url.find("://").map(|i| i + 3) else {
        return url.to_owned();
    };
    let rest = &url[scheme_end..];
    match rest.find('@').map(|at| (&rest[..at], &rest[at..])) {
        Some((user_info, host)) => match user_info.split_once(':') {
            Some((user, _)) => format!("{}{user}:***{host}", &url[..scheme_end]),
            None => url.to_owned(),
        },
        None => url.to_owned(),
    }
}

const CHAT_FIRSTS: &str = concat!(
    "SELECT ",
    reception_columns!(),
    " FROM receptions \
    WHERE kind = 'message' AND copy = 1 AND btrim(text) !~ '^(ack|rej)[[:alnum:]]{1,5}$' \
      AND CASE $1 \
        WHEN 'broadcast' THEN dest IN ('*', '') \
        WHEN 'channel' THEN dest = $2 \
        ELSE dest = $2 OR (origin = $2 AND dest !~ '^[0-9]{0,6}$' AND dest <> '*') \
      END \
      AND ($3::uuid IS NULL OR (received_at, id) < (SELECT r.received_at, r.id FROM receptions r WHERE r.id = $3)) \
    ORDER BY received_at DESC, id DESC LIMIT $4"
);

const CHAT_COPIES: &str = concat!(
    "SELECT ",
    reception_columns!(),
    " FROM receptions \
    WHERE kind = 'message' AND copy > 1 AND msg_id = ANY($1) \
      AND received_at >= timestamp 'epoch' + $2::float8 * interval '1 microsecond' \
      AND received_at <= timestamp 'epoch' + $3::float8 * interval '1 microsecond' + interval '1 hour' \
    ORDER BY received_at, id"
);

const CHAT_FIRST_OF: &str = concat!(
    "SELECT ",
    reception_columns!(),
    " FROM receptions \
    WHERE kind = 'message' AND copy = 1 AND msg_id = $1 \
      AND received_at <= timestamp 'epoch' + $2::float8 * interval '1 microsecond' \
      AND received_at >= timestamp 'epoch' + $2::float8 * interval '1 microsecond' - interval '1 hour' \
    ORDER BY received_at DESC, id DESC LIMIT 1"
);

const CHAT_CHANNELS: &str = "\
    SELECT dest AS value, count(*)::int8 AS messages, \
        (extract(epoch FROM max(received_at)) * 1000000)::int8 AS last_us \
    FROM receptions \
    WHERE kind = 'message' AND copy = 1 AND dest ~ '^[0-9]{1,6}$' AND starts_with(dest, $1) \
    GROUP BY dest ORDER BY max(received_at) DESC LIMIT $2";

const CHAT_CALLSIGNS: &str = "\
    SELECT c.call AS value, count(r.id)::int8 AS messages, \
        (extract(epoch FROM max(r.received_at)) * 1000000)::int8 AS last_us \
    FROM (SELECT call FROM nodes \
          UNION SELECT dest FROM receptions WHERE kind = 'message' AND dest !~ '^[0-9]{0,6}$' AND dest <> '*') c \
    LEFT JOIN receptions r ON r.kind = 'message' AND r.copy = 1 \
        AND (r.dest = c.call OR (r.origin = c.call AND r.dest !~ '^[0-9]{0,6}$' AND r.dest <> '*')) \
    WHERE starts_with(c.call, $1) \
    GROUP BY c.call ORDER BY max(r.received_at) DESC NULLS LAST, c.call LIMIT $2";

#[derive(Clone)]
pub(crate) struct Store {
    pool: PgPool,
}

impl Store {
    pub(crate) async fn connect(url: &str) -> Result<Store> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(10))
            .connect(url)
            .await
            .map_err(|source| Error::DatabaseConnect {
                url: redact(url),
                source,
            })?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        tracing::info!("database connected at {}", redact(url));
        Ok(Store { pool })
    }

    pub(crate) async fn insert_reception(&self, at: u64, path: &[Call], frame: &Frame) -> Result<Reception> {
        let c = rows::columns(frame);
        let (id, copy): (Uuid, i32) = sqlx::query_as(INSERT_RECEPTION)
            .bind(at as f64)
            .bind(c.source)
            .bind(c.msg_id)
            .bind(c.origin)
            .bind(c.relays)
            .bind(path)
            .bind(c.rssi)
            .bind(c.snr)
            .bind(c.hw)
            .bind(c.firmware)
            .bind(c.kind)
            .bind(c.dest)
            .bind(c.text)
            .bind(c.lat)
            .bind(c.lon)
            .bind(c.alt_m)
            .bind(c.neighbours)
            .bind(c.hops)
            .bind(c.type_char)
            .fetch_one(&self.pool)
            .await?;
        Ok(Reception {
            id,
            at,
            copy: u32::try_from(copy).unwrap_or(1),
            path: path.to_vec(),
            frame: frame.clone(),
        })
    }

    pub(crate) async fn receptions(&self, view: &View) -> Result<Vec<Reception>> {
        let kinds: Vec<&str> = view.kinds.iter().map(|k| k.as_str()).collect();
        let rows: Vec<rows::ReceptionRow> = sqlx::query_as(SELECT_RECEPTIONS)
            .bind(kinds)
            .bind(view.copies)
            .bind(view.limit.map(i64::from))
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(rows::reception).collect()
    }

    pub(crate) async fn save_nodes(&self, nodes: &[Node]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for n in nodes {
            sqlx::query(UPSERT_NODE)
                .bind(n.last_seen as f64)
                .bind(&n.call)
                .bind(n.own)
                .bind(n.position.map(|p| p.lat))
                .bind(n.position.map(|p| p.lon))
                .bind(n.position.and_then(|p| p.alt_m))
                .bind(n.hw.map(i16::from))
                .bind(&n.firmware)
                .bind(i64::try_from(n.frames).unwrap_or(i64::MAX))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn copies(&self, firsts: &[Reception]) -> Result<Vec<Reception>> {
        let ids: Vec<i64> = firsts.iter().filter_map(|r| r.frame.msg_id.map(i64::from)).collect();
        let (Some(from), Some(to)) = (firsts.iter().map(|r| r.at).min(), firsts.iter().map(|r| r.at).max()) else {
            return Ok(vec![]);
        };
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let rows: Vec<rows::ReceptionRow> = sqlx::query_as(CHAT_COPIES)
            .bind(ids)
            .bind(from as f64)
            .bind(to as f64)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(rows::reception).collect()
    }

    pub(crate) async fn chat_page(
        &self,
        target: &Target,
        before: Option<Uuid>,
        limit: u32,
    ) -> Result<(Vec<ChatMessage>, bool)> {
        let (kind, value) = target.query_parts();
        let rows: Vec<rows::ReceptionRow> = sqlx::query_as(CHAT_FIRSTS)
            .bind(kind)
            .bind(value)
            .bind(before)
            .bind(i64::from(limit) + 1)
            .fetch_all(&self.pool)
            .await?;
        let mut firsts = rows.into_iter().map(rows::reception).collect::<Result<Vec<_>>>()?;
        let more = firsts.len() > limit as usize;
        firsts.truncate(limit as usize);
        firsts.reverse();
        let copies = self.copies(&firsts).await?;
        Ok((chat::group(firsts, copies), more))
    }

    pub(crate) async fn chat_message_of(&self, r: &Reception) -> Result<Option<ChatMessage>> {
        let first = match r.frame.msg_id {
            Some(id) if r.copy > 1 => {
                let row: Option<rows::ReceptionRow> = sqlx::query_as(CHAT_FIRST_OF)
                    .bind(i64::from(id))
                    .bind(r.at as f64)
                    .fetch_optional(&self.pool)
                    .await?;
                match row {
                    Some(row) => rows::reception(row)?,
                    None => return Ok(None),
                }
            }
            _ => r.clone(),
        };
        let copies = self.copies(std::slice::from_ref(&first)).await?;
        Ok(chat::group(vec![first], copies).into_iter().next())
    }

    pub(crate) async fn chat_targets(&self, prefix: &str, limit: u32, exclude: &[Target]) -> Result<Vec<TargetInfo>> {
        let fetch = i64::from(limit) + exclude.len() as i64;
        let channels: Vec<rows::TargetRow> = sqlx::query_as(CHAT_CHANNELS)
            .bind(prefix)
            .bind(fetch)
            .fetch_all(&self.pool)
            .await?;
        let callsigns: Vec<rows::TargetRow> = sqlx::query_as(CHAT_CALLSIGNS)
            .bind(prefix)
            .bind(fetch)
            .fetch_all(&self.pool)
            .await?;
        let infos = channels
            .into_iter()
            .filter_map(|r| rows::target_info(r, true))
            .chain(callsigns.into_iter().filter_map(|r| rows::target_info(r, false)))
            .collect();
        Ok(chat::rank(infos, exclude, limit as usize))
    }

    pub(crate) async fn stats(&self, range: Range, now: u64, tz: &str) -> Result<Stats> {
        let known: bool = sqlx::query_scalar(TIME_ZONE_EXISTS)
            .bind(tz)
            .fetch_one(&self.pool)
            .await?;
        if !known {
            return Err(Error::Invalid {
                what: "time zone",
                input: tz.to_owned(),
                reason: "not an IANA time zone name".into(),
            });
        }
        let since = range.since(now);
        let since_f = since.map(|s| s as f64);
        let totals: rows::TotalsRow = sqlx::query_as(STATS_TOTALS).bind(since_f).fetch_one(&self.pool).await?;
        let kinds: Vec<(String, i64)> = sqlx::query_as(STATS_KINDS).bind(since_f).fetch_all(&self.pool).await?;
        let timeline: Vec<rows::TimelineRow> = sqlx::query_as(STATS_TIMELINE)
            .bind(since_f)
            .bind(tz)
            .bind(range.bucket().token())
            .bind(now as f64)
            .fetch_all(&self.pool)
            .await?;
        let hours: Vec<(i32, i64)> = sqlx::query_as(STATS_HOUR_OF_DAY)
            .bind(since_f)
            .bind(tz)
            .fetch_all(&self.pool)
            .await?;
        let origins: Vec<(String, i64, i64)> = sqlx::query_as(STATS_ORIGINS)
            .bind(since_f)
            .bind(stats::TOP)
            .fetch_all(&self.pool)
            .await?;
        let relays: Vec<(String, i64, i64)> = sqlx::query_as(STATS_RELAYS)
            .bind(since_f)
            .bind(stats::TOP)
            .fetch_all(&self.pool)
            .await?;
        let hops: Vec<(i32, i64)> = sqlx::query_as(STATS_HOPS).bind(since_f).fetch_all(&self.pool).await?;
        let snr: Vec<(i16, i64)> = sqlx::query_as(STATS_SNR).bind(since_f).fetch_all(&self.pool).await?;
        let chat: Vec<(String, i64)> = sqlx::query_as(STATS_CHAT)
            .bind(since_f)
            .bind(stats::TOP)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows::stats(rows::StatsRows {
            range,
            since,
            until: now,
            totals,
            kinds,
            timeline,
            hours,
            origins,
            relays,
            hops,
            snr,
            chat,
        }))
    }

    pub(crate) async fn load_nodes(&self) -> Result<Vec<Node>> {
        let rows: Vec<rows::NodeRow> = sqlx::query_as(SELECT_NODES).fetch_all(&self.pool).await?;
        rows.into_iter().map(rows::node).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_is_redacted() {
        assert_eq!(
            redact("postgres://meshcom:secret@localhost:5432/meshcom"),
            "postgres://meshcom:***@localhost:5432/meshcom"
        );
        assert_eq!(
            redact("postgres://meshcom@localhost/meshcom"),
            "postgres://meshcom@localhost/meshcom"
        );
        assert_eq!(redact("postgres://localhost/meshcom"), "postgres://localhost/meshcom");
        assert_eq!(redact("nonsense"), "nonsense");
    }
}
