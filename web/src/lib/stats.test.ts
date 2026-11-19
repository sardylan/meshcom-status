import { describe, expect, it } from 'vitest';

import { parseSettings } from './settings';
import { bucketLabel, hopsLabel, isStatsRange, kindTotal, labelStride, niceTicks, percent } from './stats';

describe('stats helpers', () => {
  it('makes nice ticks that cover the maximum', () => {
    expect(niceTicks(0)).toEqual([0, 1]);
    expect(niceTicks(7)).toEqual([0, 2, 4, 6, 8]);
    expect(niceTicks(132)).toEqual([0, 50, 100, 150]);
    const t = niceTicks(913);
    expect(t[0]).toBe(0);
    expect(t[t.length - 1]).toBeGreaterThanOrEqual(913);
    expect(t.length).toBeLessThanOrEqual(6);
  });

  it('thins out labels', () => {
    expect(labelStride(24, 8)).toBe(3);
    expect(labelStride(5, 12)).toBe(1);
    expect(labelStride(169, 12)).toBe(15);
  });

  it('formats shares, totals and hop labels', () => {
    expect(percent(1, 3)).toBe('33%');
    expect(percent(1, 1000)).toBe('<1%');
    expect(percent(0, 0)).toBe('—');
    expect(kindTotal({ message: 1, position: 2, hey: 3, other: 4 })).toBe(10);
    expect([0, 1, 4].map((h) => hopsLabel({ hops: h, or_more: h === 4 }))).toEqual(['direct', '1', '4+']);
  });

  it('labels buckets', () => {
    expect(bucketLabel('2026-11-18T13:00:00Z', 'hour')).toMatch(/\d{2}.\d{2}/);
    expect(bucketLabel('2026-11-18T00:00:00Z', 'day', true).length).toBeGreaterThan(3);
    expect(bucketLabel('2026-11-18T13:00:00Z', 'hour', false, true).length).toBeGreaterThan(bucketLabel('2026-11-18T13:00:00Z', 'hour').length);
  });

  it('keeps a valid stats range in the settings', () => {
    expect(isStatsRange('7d')).toBe(true);
    expect(isStatsRange('1y')).toBe(false);
    expect(parseSettings({ statsRange: '30d', view: 'stats' })).toMatchObject({ statsRange: '30d', view: 'stats' });
    expect(parseSettings({ statsRange: 'nope' }).statsRange).toBe('24h');
  });
});
