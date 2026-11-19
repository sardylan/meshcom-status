import { useCallback, useEffect, useMemo, useState } from 'react';

import { pathKey, linkKey } from '../lib/format';
import { useLive } from '../lib/live';
import { pathsThrough } from '../lib/mapdata';
import type { FrameView, PathView } from '../lib/types';
import { MapPane } from './MapPane';
import { Panel } from './Panel';
import { PathBanner } from './PathBanner';

export interface Selection {
  key: string;
  segment: { from: string; to: string } | null;
  frame: FrameView | null;
  fallback: PathView | null;
  fit: boolean;
}

const noKinds = () => ({ message: 0, position: 0, hey: 0, other: 0 });

function framePath(f: FrameView): PathView {
  return {
    calls: f.path,
    count: 1,
    first_seen: f.received_at,
    last_seen: f.received_at,
    kinds: noKinds(),
    hops: f.path.slice(1).map((to, i) => ({ from: f.path[i] as string, to, rssi: null, snr: null, signal_seen: null })),
  };
}

export interface Focus {
  call: string;
  nonce: number;
}

export function MapView({ visible }: { visible: boolean }) {
  const live = useLive();
  const [chosen, setSelection] = useState<Selection | null>(null);
  const [focus, setFocus] = useState<Focus | null>(null);
  const [popup, setPopup] = useState<string | null>(null);

  const selection = chosen?.frame && !live.frames.some((f) => f.id === chosen.frame?.id) ? null : chosen;

  const selectedPath = useMemo(
    () => (selection ? (live.paths.find((p) => pathKey(p.calls) === selection.key) ?? selection.fallback) : null),
    [selection, live.paths],
  );

  const close = useCallback(() => setSelection(null), []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [close]);

  const selectSegment = useCallback(
    (from: string, to: string) => {
      const through = pathsThrough(live.paths, from, to);
      const link = live.links.find((l) => linkKey(l.from, l.to) === linkKey(from, to));
      const first = through[0];
      setSelection({
        key: first ? pathKey(first.calls) : pathKey([from, to]),
        segment: { from, to },
        frame: null,
        fallback: link
          ? {
              calls: [from, to],
              count: link.count,
              first_seen: null,
              last_seen: link.last_seen,
              kinds: noKinds(),
              hops: [{ from, to, rssi: link.rssi, snr: link.snr, signal_seen: link.signal_seen }],
            }
          : null,
        fit: false,
      });
    },
    [live.paths, live.links],
  );

  const selectFrame = useCallback((f: FrameView) => {
    setSelection((s) => (s?.frame?.id === f.id ? null : { key: pathKey(f.path), segment: null, frame: f, fallback: framePath(f), fit: true }));
  }, []);

  const selectPath = useCallback((key: string) => {
    setSelection((s) => (s ? { ...s, key, fit: false } : s));
  }, []);

  const focusNode = useCallback((call: string) => {
    setFocus({ call, nonce: Date.now() });
    setPopup(call);
  }, []);

  return (
    <div id="view-map" className="view" hidden={!visible}>
      <Panel selectedFrameId={selection?.frame?.id ?? null} onSelectFrame={selectFrame} onFocusNode={focusNode} />
      <div id="map-wrap">
        <MapPane
          visible={visible}
          selection={selection}
          selectedPath={selectedPath}
          focus={focus}
          popup={popup}
          onPopup={setPopup}
          onSelectSegment={selectSegment}
        />
        {selection && selectedPath && (
          <PathBanner
            path={selectedPath}
            selection={selection}
            onClose={close}
            onSelectPath={selectPath}
            onFocusNode={focusNode}
          />
        )}
      </div>
    </div>
  );
}
