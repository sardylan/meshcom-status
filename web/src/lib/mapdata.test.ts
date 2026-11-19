import { describe, expect, it } from 'vitest';

import { SNR_COLORS } from './format';
import { autoZoomTargets, chainDrawing, linksDrawing, pathsThrough, positions } from './mapdata';
import type { FrameView, LinkView, NodeView, PathView } from './types';

const node = (call: string, lat: number | null, lon = -30): NodeView => ({
  call,
  own: false,
  position: lat === null ? null : { lat, lon },
  hw: null,
  firmware: null,
  frames: 0,
  last_seen: '2026-11-18T12:00:00Z',
});

const nodes = Object.fromEntries([node('A1A', 10), node('B1B', 10.5), node('C1C', null)].map((n) => [n.call, n]));

const link = (from: string, to: string, snr: number | null): LinkView => ({
  from, to, count: 1, last_seen: '2026-11-18T12:00:00Z', rssi: snr === null ? null : -100, snr, signal_seen: null,
});

const path = (calls: string[], last: string): PathView => ({
  calls, count: 1, first_seen: last, last_seen: last, hops: [], kinds: { message: 0, position: 0, hey: 0, other: 0 },
});

describe('map data', () => {
  it('draws only links with both ends placed', () => {
    const d = linksDrawing([link('A1A', 'B1B', 6), link('B1B', 'C1C', null)], nodes);
    expect(d.lines.features).toHaveLength(1);
    expect(d.lines.features[0]?.properties).toEqual({ from: 'A1A', to: 'B1B', color: SNR_COLORS.great, measured: true });
    expect(d.arrows.features[0]?.properties.bearing).toBeCloseTo(0, 3);
  });

  it('moves the arrows aside when both directions exist', () => {
    const d = linksDrawing([link('A1A', 'B1B', null), link('B1B', 'A1A', null)], nodes);
    const lats = d.arrows.features.map((f) => f.geometry.coordinates[1] as number);
    expect(lats[0]).toBeGreaterThan(10.25);
    expect(lats[1]).toBeLessThan(10.25);
    expect(d.lines.features[0]?.properties.measured).toBe(false);
  });

  it('draws a chain and skips hops without positions', () => {
    const d = chainDrawing(['A1A', 'B1B', 'C1C'], nodes, '#00f');
    expect(d.lines.features).toHaveLength(1);
    expect(positions(['A1A', 'C1C', 'B1B'], nodes)).toEqual([[-30, 10], [-30, 10.5]]);
  });

  it('finds the paths through a hop, newest first', () => {
    const paths = [path(['A1A', 'B1B'], '2026-11-18T10:00:00Z'), path(['A1A', 'B1B', 'C1C'], '2026-11-18T11:00:00Z'), path(['B1B', 'A1A'], '2026-11-18T12:00:00Z')];
    expect(pathsThrough(paths, 'A1A', 'B1B').map((p) => p.calls.length)).toEqual([3, 2]);
    expect(pathsThrough(paths, 'C1C', 'A1A')).toEqual([]);
  });

  it('finds what auto-zoom fits to, per mode', () => {
    const more = { ...nodes, D1D: node('D1D', 11), E1E: node('E1E', 12) };
    const frame = (id: string, path: string[]): FrameView => ({
      id, received_at: '2026-11-18T12:00:00Z', source: 'console', copy: 1, origin: path[0] as string, relays: path.slice(1),
      path, rssi: null, snr: null, hw: null, firmware: null, kind: 'other',
    });
    const frames = [frame('2', ['A1A', 'B1B', 'C1C']), frame('1', ['E1E'])];
    const links = [link('A1A', 'B1B', 3), link('B1B', 'C1C', 3), link('D1D', 'B1B', null)];
    expect(autoZoomTargets('last', frames, links, more)).toEqual([[-30, 10], [-30, 10.5]]);
    expect(autoZoomTargets('paths', frames, links, more)).toEqual([[-30, 10], [-30, 10.5], [-30, 11]]);
    expect(autoZoomTargets('stations', frames, links, more)).toEqual([[-30, 10], [-30, 10.5], [-30, 12]]);
    expect(autoZoomTargets('last', [], links, more)).toEqual([]);
  });
});
