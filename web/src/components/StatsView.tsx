import { useEffect, useEffectEvent, useRef, useState } from 'react';

import { api } from '../lib/config';
import { ago, clock, snrColor } from '../lib/format';
import { useLive } from '../lib/live';
import { saveSettings, settings } from '../lib/settings';
import {
  KIND_NAMES,
  RANGE_LABELS,
  STATS_RANGES,
  bucketLabel,
  hopsLabel,
  kindTotal,
  num,
  percent,
  type StatsBody,
  type StatsRange,
} from '../lib/stats';
import { KINDS, type Kind } from '../lib/types';
import { BarList, Columns, type Column, type Series } from './charts';

const REFRESH_MS = 30_000;

const kindColor = (k: Kind): string => `var(--k-${k})`;
const KIND_SERIES: Series[] = KINDS.map((k) => ({ name: KIND_NAMES[k], color: kindColor(k) }));
const SINGLE: Series[] = [{ name: 'frames', color: 'var(--path)' }];

const timeZone = (): string => Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';

async function fetchStats(range: StatsRange): Promise<StatsBody> {
  const r = await fetch(api(`/stats?${new URLSearchParams({ range, tz: timeZone() })}`));
  if (!r.ok) throw new Error(`statistics: HTTP ${r.status}`);
  return (await r.json()) as StatsBody;
}

function Tile({ label, value, note }: { label: string; value: string; note?: string }) {
  return (
    <div className="tile">
      <div className="tile-label">{label}</div>
      <div className="tile-value">{value}</div>
      {note && <div className="tile-note">{note}</div>}
    </div>
  );
}

function Card({ title, subtitle, wide, children }: { title: string; subtitle?: string; wide?: boolean; children: React.ReactNode }) {
  return (
    <section className={wide ? 'card wide' : 'card'}>
      <h2>{title}</h2>
      {subtitle && <p className="muted">{subtitle}</p>}
      {children}
    </section>
  );
}

function Charts({ s }: { s: StatsBody }) {
  const t = s.totals;
  const kinds = kindTotal(s.kinds);
  const longRange = s.bucket === 'hour' && s.timeline.length > 25;
  const timeline: Column[] = s.timeline.map((b) => ({
    label: bucketLabel(b.start, s.bucket, false, longRange),
    title: bucketLabel(b.start, s.bucket, true),
    segments: KINDS.map((k) => ({ name: KIND_NAMES[k], value: b[k], color: kindColor(k) })),
  }));
  const hours: Column[] = s.hour_of_day.map((n, h) => ({
    label: String(h).padStart(2, '0'),
    title: `${String(h).padStart(2, '0')}:00–${String(h).padStart(2, '0')}:59`,
    segments: [{ name: 'frames', value: n, color: 'var(--path)' }],
  }));
  const hopFrames = s.hops.reduce((a, h) => a + h.frames, 0);
  const hops: Column[] = s.hops.map((h) => ({
    label: hopsLabel(h),
    title: `${h.hops === 0 ? 'Heard directly' : `${hopsLabel(h)} relay${h.hops === 1 && !h.or_more ? '' : 's'}`} · ${percent(h.frames, hopFrames)}`,
    segments: [{ name: 'frames', value: h.frames, color: 'var(--path)' }],
  }));
  const snrTotal = s.snr.reduce((a, x) => a + x.receptions, 0);
  const snr: Column[] = s.snr.map((x) => ({
    label: String(x.snr),
    title: `SNR ${x.snr} dB · ${percent(x.receptions, snrTotal)}`,
    segments: [{ name: 'receptions', value: x.receptions, color: snrColor(x.snr) }],
  }));

  return (
    <>
      <div className="tiles">
        <Tile label="Frames" value={num(t.frames)} note={`${num(t.receptions)} receptions with copies`} />
        <Tile label="Chat messages" value={num(t.messages)} note="acknowledgements left out" />
        <Tile label="Stations heard" value={num(t.stations)} note="distinct origins" />
        <Tile label="Relays" value={num(t.relays)} note="calls that forwarded frames" />
        <Tile label="Heard directly" value={percent(t.direct, t.frames)} note={`${num(t.direct)} frames with no relay`} />
        <Tile label="Copies per frame" value={t.frames ? (t.receptions / t.frames).toFixed(1) : '—'} note="receptions ÷ frames" />
      </div>
      <div className="cards">
        <Card wide title={`Frames per ${s.bucket}`} subtitle="Distinct frames by type; copies of the same frame count once.">
          <Columns columns={timeline} series={KIND_SERIES} unit="frames" height={220} maxLabels={12} labelWidth={longRange ? 96 : 56} />
        </Card>
        <Card title="Frames by type">
          <BarList
            items={KINDS.map((k) => ({ key: k, name: KIND_NAMES[k], value: s.kinds[k], color: kindColor(k), note: percent(s.kinds[k], kinds) }))}
            empty="No frames in this range."
          />
        </Card>
        <Card title="Activity by hour of day" subtitle={`Frames, in your time zone (${timeZone()}).`}>
          <Columns columns={hours} series={SINGLE} unit="frames" maxLabels={8} />
        </Card>
        <Card title="Relays per frame" subtitle="How many relays a frame went through before the node heard it.">
          <Columns columns={hops} series={SINGLE} unit="frames" maxLabels={6} />
        </Card>
        <Card title="Signal" subtitle="Receptions by SNR (dB) measured by the own node, coloured as on the map.">
          {snr.length ? <Columns columns={snr} series={[{ name: 'receptions', color: 'var(--path)' }]} unit="receptions" maxLabels={10} /> : <p className="muted">No measured receptions.</p>}
        </Card>
        <Card title="Busiest stations" subtitle="Frames originated.">
          <BarList items={s.top_origins.map((r) => ({ key: r.name, name: r.name, value: r.count, note: percent(r.count, t.frames) }))} />
        </Card>
        <Card title="Busiest relays" subtitle="Frames forwarded.">
          <BarList items={s.top_relays.map((r) => ({ key: r.name, name: r.name, value: r.count, note: percent(r.count, t.frames) }))} />
        </Card>
        <Card title="Chat" subtitle="Messages by addressee: all, a group, or a callsign.">
          <BarList items={s.chat.map((r) => ({ key: r.name, name: r.name, value: r.count, color: 'var(--k-message)' }))} empty="No chat messages in this range." />
        </Card>
      </div>
    </>
  );
}

export function StatsView({ visible }: { visible: boolean }) {
  const { fresh } = useLive();
  const [range, setRange] = useState<StatsRange>(settings().statsRange);
  const [data, setData] = useState<{ range: StatsRange; body: StatsBody; at: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const lastFetch = useRef(0);

  const load = useEffectEvent((r: StatsRange) => {
    lastFetch.current = Date.now();
    fetchStats(r)
      .then((body) => {
        setData({ range: r, body, at: new Date().toISOString() });
        setError(null);
      })
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
  });

  useEffect(() => {
    if (visible) load(range);
  }, [visible, range]);

  useEffect(() => {
    if (!visible || !fresh) return;
    const wait = Math.max(0, lastFetch.current + REFRESH_MS - Date.now());
    const timer = setTimeout(() => load(range), wait);
    return () => clearTimeout(timer);
  }, [fresh, visible, range]);

  const choose = (r: StatsRange) => {
    setRange(r);
    saveSettings({ statsRange: r });
  };

  const s = data?.body;
  const loading = !data || data.range !== range;
  return (
    <div id="view-stats" className="view" hidden={!visible}>
      <div className="stats-bar">
        <div className="segmented" role="radiogroup" aria-label="Range">
          {STATS_RANGES.map((r) => (
            <button key={r} role="radio" aria-checked={range === r} onClick={() => choose(r)}>
              {RANGE_LABELS[r]}
            </button>
          ))}
        </div>
        <span className="muted">
          {error ? <span className="error">{error}</span> : loading ? 'loading…' : `updated ${clock(data.at)}`}
          {s && !loading && s.totals.first_seen && ` · data since ${ago(s.totals.first_seen)}`}
        </span>
      </div>
      <div className={loading ? 'stats-body stale' : 'stats-body'}>{s && <Charts s={s} />}</div>
    </div>
  );
}
