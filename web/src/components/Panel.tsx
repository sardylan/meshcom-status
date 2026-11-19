import { useEffect, useState } from 'react';

import { CONFIG } from '../lib/config';
import { ago, clock, KIND_LABEL, signalText, SNR_COLORS } from '../lib/format';
import { useLive } from '../lib/live';
import { saveSettings, settings, type PanelTab } from '../lib/settings';
import { KINDS, type FrameView, type Kind } from '../lib/types';
import { frameSummary, frameTitle } from './PathBanner';

function Filters() {
  const { view, setView } = useLive();
  const toggle = (k: Kind, on: boolean) => setView({ ...view, kinds: on ? KINDS.filter((x) => x === k || view.kinds.includes(x)) : view.kinds.filter((x) => x !== k) });
  return (
    <div className="filters">
      <div className="filter-row limit">
        <label>
          Show the last
          <select
            aria-label="Number of frames shown"
            value={view.limit === null ? 'all' : String(view.limit)}
            onChange={(e) => setView({ ...view, limit: e.target.value === 'all' ? null : Number(e.target.value) })}
          >
            {[...CONFIG.limits.map(String), 'all'].map((v) => (
              <option key={v} value={v}>
                {v}
              </option>
            ))}
          </select>
          frames
        </label>
        <label className="chip" title="Count every reception of a frame, not only the first">
          <input type="checkbox" checked={view.copies} onChange={(e) => setView({ ...view, copies: e.target.checked })} />
          copies
        </label>
      </div>
      <div className="filter-row" role="group" aria-label="Frame types">
        <span className="filter-label">Types</span>
        {KINDS.map((k) => (
          <label key={k} className={`chip k-${k}`}>
            <input type="checkbox" checked={view.kinds.includes(k)} onChange={(e) => toggle(k, e.target.checked)} />
            {k === 'other' ? 'other' : KIND_LABEL[k]}
          </label>
        ))}
      </div>
    </div>
  );
}

function FrameRow({ f, selected, fresh, onClick }: { f: FrameView; selected: boolean; fresh: boolean; onClick: () => void }) {
  const via = f.relays.length ? `via ${f.relays.join(', ')}` : 'direct';
  const hops = (f.hops ?? []).map((h, i) => `${f.path[i] ?? '?'} → ${f.path[i + 1] ?? '?'}: ${h.rssi} dBm, SNR ${h.snr} dB`);
  return (
    <li className={`${selected ? 'selected' : ''}${fresh ? ' fresh' : ''}`} onClick={onClick}>
      <div className="head">
        <span className={`badge k-${f.kind}`}>{KIND_LABEL[f.kind]}</span>
        <span className="who">{frameTitle(f)}</span>
        {f.copy > 1 && <span className="copy">copy {f.copy}</span>}
        <span className="time">{clock(f.received_at)}</span>
      </div>
      <div className="body">{frameSummary(f)}</div>
      <div className="via">
        {via} · {signalText(f.rssi, f.snr)}
      </div>
      {selected && (
        <div className="detail">
          path: {f.path.join(' → ')}
          <br />
          {f.msg_id ? `msg id ${f.msg_id} · ` : ''}from {f.source}
          <br />
          origin hw {f.hw ?? '—'} · fw {f.firmware ?? '—'}
          {hops.length > 0 && (
            <>
              <br />
              hop signals:
              <ul className="hops">
                {hops.map((h) => (
                  <li key={h}>{h}</li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
    </li>
  );
}

function Legend() {
  const bars: [string, string][] = [
    [SNR_COLORS.great, '≥ 5 dB'],
    [SNR_COLORS.good, '0 … 5 dB'],
    [SNR_COLORS.fair, '−7 … 0 dB'],
    [SNR_COLORS.poor, '−14 … −7 dB'],
    [SNR_COLORS.bad, '< −14 dB'],
  ];
  return (
    <section className="tab legend">
      <h2>Links: B heard A (A → B), by last SNR</h2>
      <ul>
        {bars.map(([c, t]) => (
          <li key={t}>
            <i style={{ background: c }} />
            {t}
          </li>
        ))}
        <li>
          <i className="dashed" />
          signal not measured
        </li>
        <li>
          <i style={{ background: '#2563eb' }} />
          selected path / newest frame
        </li>
        <li>
          <i style={{ background: '#06b6d4' }} />
          paths through the segment under the mouse
        </li>
      </ul>
      <p className="hint">
        The map and the list show the last N frames that pass the filters (default {CONFIG.defaultLimit}); links and paths come from those frames only, nodes are
        always shown. Hover a segment to see every path through it; click it to open the most recent one. Click a frame to open its path. Base maps and overlays are
        chosen with the Layers button on the map.
      </p>
      <h2>Frames</h2>
      <ul>
        <li>
          <span className="badge k-message">MSG</span>text message (DM, group, broadcast)
        </li>
        <li>
          <span className="badge k-position">POS</span>position beacon
        </li>
        <li>
          <span className="badge k-hey">HEY</span>HEY beacon, with the signal of each hop
        </li>
        <li>
          <span className="badge k-other">…</span>anything else
        </li>
        <li>
          <span className="copy">copy 2</span>the same frame again, through another path
        </li>
      </ul>
    </section>
  );
}

interface Props {
  selectedFrameId: string | null;
  onSelectFrame: (f: FrameView) => void;
  onFocusNode: (call: string) => void;
}

export function Panel({ selectedFrameId, onSelectFrame, onFocusNode }: Props) {
  const live = useLive();
  const [tab, setTab] = useState<PanelTab>(settings().tab);
  const [, tick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => tick((n) => n + 1), 30000);
    return () => clearInterval(t);
  }, []);

  const nodes = Object.values(live.nodes).sort((a, b) => Date.parse(b.last_seen) - Date.parse(a.last_seen));
  const placed = (call: string) => live.nodes[call]?.position != null;
  const drawn = live.links.filter((l) => placed(l.from) && placed(l.to)).length;
  const hidden = nodes.filter((n) => !n.position).length + (live.links.length - drawn);

  const choose = (t: PanelTab) => {
    setTab(t);
    saveSettings({ tab: t });
  };

  return (
    <aside id="panel">
      <section className="stats">
        <div>
          <b>{nodes.length}</b>
          <span>nodes</span>
        </div>
        <div>
          <b>{live.links.length}</b>
          <span>links</span>
        </div>
        <div>
          <b>{live.frames.length}</b>
          <span>frames</span>
        </div>
        <div>
          <b>{hidden}</b>
          <span>not on map</span>
        </div>
      </section>
      <nav className="tabs" role="tablist">
        {(['traffic', 'nodes', 'legend'] as const).map((t) => (
          <button key={t} role="tab" aria-selected={tab === t} onClick={() => choose(t)}>
            {t[0]!.toUpperCase() + t.slice(1)}
          </button>
        ))}
      </nav>
      {tab === 'traffic' && (
        <section className="tab">
          <Filters />
          <ul className="frames">
            {live.frames.map((f) => (
              <FrameRow key={f.id} f={f} selected={f.id === selectedFrameId} fresh={f === live.fresh} onClick={() => onSelectFrame(f)} />
            ))}
          </ul>
        </section>
      )}
      {tab === 'nodes' && (
        <section className="tab">
          <ul className="nodes">
            {nodes.map((n) => {
              const hears = live.links.filter((l) => l.to === n.call).length;
              const heardBy = live.links.filter((l) => l.from === n.call).length;
              return (
                <li key={n.call} className={`${n.own ? 'own' : ''} ${n.position ? '' : 'nopos'}`} onClick={() => onFocusNode(n.call)}>
                  <span className="call">{n.call}</span>
                  <span className="ago">{ago(n.last_seen)}</span>
                  <span className="meta">
                    hears {hears} · heard by {heardBy}
                  </span>
                </li>
              );
            })}
          </ul>
        </section>
      )}
      {tab === 'legend' && <Legend />}
    </aside>
  );
}
