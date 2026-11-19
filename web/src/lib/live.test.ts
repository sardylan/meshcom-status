import { describe, expect, it } from 'vitest';

import { reduce } from './live';
import type { FrameView, LinkView, NodeView } from './types';

const node = (call: string): NodeView => ({ call, own: false, position: null, hw: null, firmware: null, frames: 0, last_seen: '2026-11-18T12:00:00Z' });

const frame = (id: string): FrameView => ({
  id,
  received_at: '2026-11-18T12:00:00Z',
  source: 'console',
  copy: 1,
  origin: 'A1A',
  relays: [],
  path: ['A1A'],
  rssi: null,
  snr: null,
  hw: null,
  firmware: null,
  kind: 'other',
});

const link: LinkView = { from: 'A1A', to: 'B1B', count: 1, last_seen: '2026-11-18T12:00:00Z', rssi: null, snr: null, signal_seen: null };

const empty = { status: 'open' as const, nodes: {}, links: [], paths: [], frames: [], fresh: null };

describe('live reducer', () => {
  it('replaces everything on a snapshot, newest frame first', () => {
    const s = reduce(empty, {
      type: 'message',
      message: { type: 'snapshot', view: { limit: 30, kinds: [], copies: true }, nodes: [node('A1A')], frames: [frame('1'), frame('2')], links: [link], paths: [] },
    });
    expect(Object.keys(s.nodes)).toEqual(['A1A']);
    expect(s.frames.map((f) => f.id)).toEqual(['2', '1']);
    expect(s.links).toEqual([link]);
  });

  it('adds a frame and drops the ones that left the window', () => {
    const start = { ...empty, frames: [frame('2'), frame('1')] };
    const s = reduce(start, { type: 'message', message: { type: 'frame', frame: frame('3'), dropped: ['1'], links: [], paths: [] } });
    expect(s.frames.map((f) => f.id)).toEqual(['3', '2']);
    expect(s.fresh?.id).toBe('3');
  });

  it('updates one node and the status', () => {
    const s = reduce(empty, { type: 'message', message: { type: 'node', node: node('B1B') } });
    expect(Object.keys(s.nodes)).toEqual(['B1B']);
    expect(reduce(s, { type: 'status', status: 'closed' }).status).toBe('closed');
  });
});
