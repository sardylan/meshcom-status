import { useState } from 'react';

import type { BaseMap } from '../lib/config';
import type { Overlays } from '../lib/settings';

const OVERLAY_LABELS: Record<keyof Overlays, string> = {
  links: 'Links (who heard whom)',
  nodes: 'Nodes',
  labels: 'Callsign labels',
  paths: 'Path highlights',
};

interface Props {
  baseMaps: BaseMap[];
  base: string;
  onBase: (name: string) => void;
  overlays: Overlays;
  onOverlays: (o: Overlays) => void;
  dimBase: boolean;
  onDimBase: (dim: boolean) => void;
}

export function LayerSwitcher({ baseMaps, base, onBase, overlays, onOverlays, dimBase, onDimBase }: Props) {
  const [open, setOpen] = useState(false);
  return (
    <div className="layer-switcher">
      <button className="layer-button" aria-expanded={open} onClick={() => setOpen((o) => !o)} title="Map layers">
        ◫ Layers
      </button>
      {open && (
        <div className="layer-panel">
          <fieldset>
            <legend>Base map</legend>
            {baseMaps.map((b) => (
              <label key={b.name}>
                <input type="radio" name="base-map" checked={b.name === base} onChange={() => onBase(b.name)} />
                {b.name}
              </label>
            ))}
            <label className="dim-base">
              <input type="checkbox" checked={dimBase} onChange={(e) => onDimBase(e.target.checked)} />
              Dim the map, so the lines stand out
            </label>
          </fieldset>
          <fieldset>
            <legend>Overlays</legend>
            {(Object.keys(OVERLAY_LABELS) as (keyof Overlays)[]).map((k) => (
              <label key={k}>
                <input type="checkbox" checked={overlays[k]} onChange={(e) => onOverlays({ ...overlays, [k]: e.target.checked })} />
                {OVERLAY_LABELS[k]}
              </label>
            ))}
          </fieldset>
        </div>
      )}
    </div>
  );
}
