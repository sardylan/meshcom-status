import { AUTO_ZOOM_MODES, type AutoZoom, type AutoZoomMode } from '../lib/settings';

const MODE_LABELS: Record<AutoZoomMode, string> = {
  last: 'last message',
  paths: 'all visible paths',
  stations: 'all visible stations',
};

interface Props {
  value: AutoZoom;
  onChange: (value: AutoZoom) => void;
  pausedFor: number;
  onResume: () => void;
}

export function AutoZoomControl({ value, onChange, pausedFor, onResume }: Props) {
  return (
    <div className="map-box auto-zoom" title="Move the map to new frames as they arrive">
      <label>
        <input type="checkbox" checked={value.enabled} onChange={(e) => onChange({ ...value, enabled: e.target.checked })} />
        Auto-zoom
      </label>
      <select
        aria-label="Auto-zoom to"
        value={value.mode}
        disabled={!value.enabled}
        onChange={(e) => onChange({ ...value, mode: e.target.value as AutoZoomMode })}
      >
        {AUTO_ZOOM_MODES.map((m) => (
          <option key={m} value={m}>
            {MODE_LABELS[m]}
          </option>
        ))}
      </select>
      {value.enabled && pausedFor > 0 && (
        <span className="paused">
          paused {pausedFor} s ·{' '}
          <button type="button" className="link" onClick={onResume}>
            resume
          </button>
        </span>
      )}
    </div>
  );
}
