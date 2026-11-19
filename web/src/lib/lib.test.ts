import { describe, expect, it } from 'vitest';

import { DEFAULT_CONFIG, pageConfig } from './config';
import { ago, snrColor, SNR_COLORS } from './format';
import { along, bearing, geodesic, wheelPixels, wheelStep, type LonLat } from './geo';
import { basePaint } from '../components/MapPane';
import { DEFAULT_AUTO_ZOOM, DEFAULT_OVERLAYS, parseSettings } from './settings';

describe('geo', () => {
  it('draws a great circle with continuous longitudes', () => {
    const pts = geodesic([-30.19, 10.23], [-30.02, 10.29]);
    expect(pts[0]?.[0]).toBeCloseTo(-30.19, 6);
    expect(pts[0]?.[1]).toBeCloseTo(10.23, 6);
    const last = pts[pts.length - 1] as LonLat;
    expect(last[0]).toBeCloseTo(-30.02, 6);
    expect(last[1]).toBeCloseTo(10.29, 6);
    expect(pts.length).toBeGreaterThanOrEqual(9);
    const across = geodesic([179, 0], [-179, 0]);
    across.slice(1).forEach((p, i) => expect(Math.abs(p[0] - (across[i] as LonLat)[0])).toBeLessThan(5));
  });

  it('returns the two ends for a zero-length line', () => {
    expect(geodesic([1, 2], [1, 2])).toEqual([[1, 2], [1, 2]]);
  });

  it('computes bearings', () => {
    expect(bearing([0, 0], [0, 1])).toBeCloseTo(0, 6);
    expect(bearing([0, 0], [1, 0])).toBeCloseTo(90, 6);
    expect(bearing([0, 0], [0, -1])).toBeCloseTo(180, 6);
    expect(bearing([0, 0], [-1, 0])).toBeCloseTo(270, 6);
  });

  it('finds the point and direction along a line', () => {
    const { point, bearing: b } = along([[0, 0], [0, 2]], 0.5);
    expect(point).toEqual([0, 1]);
    expect(b).toBeCloseTo(0, 6);
  });
});

describe('format', () => {
  it('describes ages', () => {
    const now = Date.parse('2026-11-18T12:00:00Z');
    expect(ago('2026-11-18T11:59:30Z', now)).toBe('30 s ago');
    expect(ago('2026-11-18T11:30:00Z', now)).toBe('30 min ago');
    expect(ago('2026-11-18T09:00:00Z', now)).toBe('3 h ago');
    expect(ago('2026-11-15T12:00:00Z', now)).toBe('3 d ago');
    expect(ago(null, now)).toBe('—');
  });

  it('colours by SNR', () => {
    expect(snrColor(null)).toBe(SNR_COLORS.none);
    expect(snrColor(10)).toBe(SNR_COLORS.great);
    expect(snrColor(-20)).toBe(SNR_COLORS.bad);
  });
});

describe('settings', () => {
  it('falls back to defaults on garbage', () => {
    const s = parseSettings('nonsense');
    expect(s.view).toBe('map');
    expect(s.limit).toBeNull();
    expect(s.overlays).toEqual(DEFAULT_OVERLAYS);
    expect(s.chatTabs).toEqual([]);
    expect(s.autoZoom).toEqual(DEFAULT_AUTO_ZOOM);
    expect(DEFAULT_AUTO_ZOOM).toEqual({ enabled: true, mode: 'last' });
  });

  it('reads the auto-zoom choice', () => {
    expect(parseSettings({ autoZoom: { enabled: false, mode: 'stations' } }).autoZoom).toEqual({ enabled: false, mode: 'stations' });
    expect(parseSettings({ autoZoom: { enabled: 'x', mode: 'nope' } }).autoZoom).toEqual(DEFAULT_AUTO_ZOOM);
  });

  it('keeps valid values and drops invalid ones', () => {
    const s = parseSettings({
      view: 'chat',
      limit: 'all',
      kinds: ['hey', 'nope'],
      copies: false,
      tab: 'legend',
      map: { lat: 39, lon: 9, zoom: 12 },
      overlays: { links: false, labels: 'x' },
      chatTabs: [{ target: 'channel:12351', label: '12351' }, { target: 3 }],
      chatActive: 'broadcast',
    });
    expect(s.view).toBe('chat');
    expect(s.limit).toBe('all');
    expect(s.kinds).toEqual(['hey']);
    expect(s.copies).toBe(false);
    expect(s.tab).toBe('legend');
    expect(s.map).toEqual({ lat: 39, lon: 9, zoom: 12 });
    expect(s.overlays).toEqual({ ...DEFAULT_OVERLAYS, links: false });
    expect(s.chatTabs).toEqual([{ target: 'channel:12351', label: '12351' }]);
    expect(parseSettings({ limit: -3 }).limit).toBeNull();
    expect(parseSettings({ map: { lat: 'x' } }).map).toBeNull();
  });
});

describe('config', () => {
  it('fills missing values with defaults', () => {
    expect(pageConfig(null)).toEqual(DEFAULT_CONFIG);
    const c = pageConfig({ apiPrefix: '/x', baseMaps: [] });
    expect(c.apiPrefix).toBe('/x');
    expect(c.baseMaps).toEqual(DEFAULT_CONFIG.baseMaps);
  });
});

describe('chat suggestions', () => {
  it('label a suggestion with the channel or callsign first', async () => {
    const { suggestionLabel } = await import('../components/ChatView');
    const now = Date.parse('2026-11-18T12:00:00Z');
    const channel = { target: 'channel:12351', label: '12351', kind: 'channel' as const, messages: 3, last_seen: '2026-11-18T11:58:00Z' };
    expect(suggestionLabel(channel, now)).toBe('12351 · 3 messages · 2 min ago');
    const dm = { target: 'callsign:QA0OWN-11', label: 'QA0OWN-11', kind: 'callsign' as const, messages: 1, last_seen: null };
    expect(suggestionLabel(dm, now)).toBe('QA0OWN-11 · 1 message');
  });
});

describe('dimmed base map', () => {
  it('is on by default and kept when stored', () => {
    expect(parseSettings({}).dimBase).toBe(true);
    expect(parseSettings({ dimBase: false }).dimBase).toBe(false);
    expect(parseSettings({ dimBase: 'no' }).dimBase).toBe(true);
  });

  it('darkens the raster only when on', () => {
    expect(basePaint(false)).toEqual({});
    const p = basePaint(true) ?? {};
    expect(p['raster-brightness-max']).toBeLessThan(1);
  });
});

describe('wheel zoom', () => {
  it('turns any wheel unit into pixels', () => {
    expect(wheelPixels(-100, 0)).toBe(-100);
    expect(wheelPixels(3, 1)).toBe(120);
    expect(wheelPixels(1, 2)).toBe(800);
  });

  it('zooms exactly one level per gesture, whatever its size', () => {
    expect(wheelStep(-100)).toBe(1);
    expect(wheelStep(-20)).toBe(1);
    expect(wheelStep(-240)).toBe(1);
    expect(wheelStep(5000)).toBe(-1);
    expect(wheelStep(120)).toBe(-1);
    expect(wheelStep(2)).toBe(0);
  });

});
