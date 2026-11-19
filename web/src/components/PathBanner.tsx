import { Fragment } from 'react';

import { ago, clock, KIND_LABEL, pathKey, signalText, snrColor } from '../lib/format';
import { useLive } from '../lib/live';
import { pathsThrough } from '../lib/mapdata';
import type { FrameView, KindsView, NodeView, PathView } from '../lib/types';
import type { Selection } from './MapView';

function role(i: number, calls: string[], nodes: Record<string, NodeView>): string {
  if (i === 0) return 'origin';
  if (nodes[calls[i] as string]?.own) return 'this node, receiver';
  if (i === calls.length - 1) return 'last relay';
  return `relay ${i}`;
}

function nodeInfo(n: NodeView | undefined): string {
  if (!n) return 'unknown node';
  const p = n.position;
  const pos = p ? `${p.lat.toFixed(4)}, ${p.lon.toFixed(4)}${p.alt_m != null ? ` · ${p.alt_m} m` : ''}` : 'no position';
  return [pos, n.hw != null ? `hw ${n.hw}` : null, n.firmware ? `fw ${n.firmware}` : null, `seen ${ago(n.last_seen)}`].filter(Boolean).join(' · ');
}

function Kinds({ kinds }: { kinds: KindsView }) {
  return (
    <div>
      {(Object.keys(kinds) as (keyof KindsView)[])
        .filter((k) => kinds[k] > 0)
        .map((k) => (
          <span key={k} className={`badge k-${k}`}>
            {KIND_LABEL[k]} {kinds[k]}
          </span>
        ))}
    </div>
  );
}

export function frameTitle(f: FrameView): string {
  if (f.kind !== 'message') return f.origin;
  const dest = f.dest === '' || f.dest === '*' ? 'all' : f.dest;
  return `${f.origin} → ${dest}`;
}

export function frameSummary(f: FrameView): string {
  switch (f.kind) {
    case 'message':
      return f.text ?? '';
    case 'position': {
      const p = f.position;
      return p ? `${p.lat.toFixed(4)}, ${p.lon.toFixed(4)}${p.alt_m != null ? ` · ${p.alt_m} m` : ''}` : '';
    }
    case 'hey':
      return `HEY beacon${f.neighbours != null ? ` · reports ${f.neighbours} neighbours` : ''}`;
    default:
      return `type ${f.type_char ?? '?'}: ${f.text ?? ''}`;
  }
}

interface Props {
  path: PathView;
  selection: Selection;
  onClose: () => void;
  onSelectPath: (key: string) => void;
  onFocusNode: (call: string) => void;
}

export function PathBanner({ path, selection, onClose, onSelectPath, onFocusNode }: Props) {
  const live = useLive();
  const calls = path.calls;
  const seg = selection.segment;
  const alternatives = seg ? pathsThrough(live.paths, seg.from, seg.to) : [];
  const f = selection.frame;
  const hops = calls.length - 1;
  return (
    <section className="banner" aria-live="polite" onWheel={(e) => e.stopPropagation()}>
      <header>
        <b>
          Path · {hops} hop{hops === 1 ? '' : 's'}
        </b>
        <button className="close" title="Close" aria-label="Close" onClick={onClose}>
          ×
        </button>
      </header>
      <div className="route">{calls.join(' → ')}</div>
      <div className="meta">
        seen {path.count}× · last {ago(path.last_seen)}
        {path.first_seen ? ` · first ${ago(path.first_seen)}` : ''}
      </div>
      <Kinds kinds={path.kinds} />
      {f && (
        <div className="frame-ref">
          <span className={`badge k-${f.kind}`}>{KIND_LABEL[f.kind]}</span>
          {frameTitle(f)} · {clock(f.received_at)}
          <br />
          {frameSummary(f)}
        </div>
      )}
      <h3>Nodes and hops</h3>
      <ol className="steps">
        {calls.map((call, i) => {
          const node = live.nodes[call];
          const hop = path.hops[i];
          const measured = hop?.snr !== null && hop?.snr !== undefined;
          return (
            <Fragment key={`${call}-${i}`}>
              <li className={`node${node?.own ? ' own' : ''}`} onClick={() => onFocusNode(call)}>
                <span className="num">{i + 1}</span>
                <span className="call">
                  {call}
                  <span className="role">{role(i, calls, live.nodes)}</span>
                </span>
                <span className="info">{nodeInfo(node)}</span>
              </li>
              {i < calls.length - 1 && (
                <li className={`hop${measured ? '' : ' unmeasured'}`} style={{ borderLeftColor: snrColor(hop?.snr) }}>
                  ↓ {calls[i + 1]} heard {call}: {signalText(hop?.rssi, hop?.snr)}
                  {measured ? ` (${ago(hop?.signal_seen)})` : ''}
                </li>
              )}
            </Fragment>
          );
        })}
      </ol>
      {seg && alternatives.length > 1 && (
        <>
          <h3>
            All {alternatives.length} paths through {seg.from} → {seg.to}
          </h3>
          <ul className="alts">
            {alternatives.map((a) => {
              const key = pathKey(a.calls);
              return (
                <li key={key} className={key === selection.key ? 'current' : ''} onClick={() => onSelectPath(key)}>
                  <span className="route-text">{a.calls.join(' → ')}</span> <span className="meta">· {a.count}× · {ago(a.last_seen)}</span>
                </li>
              );
            })}
          </ul>
        </>
      )}
    </section>
  );
}
