import type { Kind, KindsView } from './types';

export type StatsRange = '24h' | '7d' | '30d' | 'all';
export const STATS_RANGES: StatsRange[] = ['24h', '7d', '30d', 'all'];
export const DEFAULT_STATS_RANGE: StatsRange = '24h';
export const RANGE_LABELS: Record<StatsRange, string> = { '24h': '24 h', '7d': '7 days', '30d': '30 days', all: 'All' };

export interface Ranked {
  name: string;
  count: number;
}

export interface StatsBody {
  range: StatsRange;
  ranges: StatsRange[];
  bucket: 'hour' | 'day';
  since: string | null;
  until: string;
  totals: {
    frames: number;
    receptions: number;
    messages: number;
    stations: number;
    relays: number;
    direct: number;
    first_seen: string | null;
  };
  kinds: KindsView;
  timeline: ({ start: string } & KindsView)[];
  hour_of_day: number[];
  top_origins: Ranked[];
  top_relays: Ranked[];
  hops: { hops: number; or_more: boolean; frames: number }[];
  snr: { snr: number; receptions: number }[];
  chat: Ranked[];
}

export const KIND_NAMES: Record<Kind, string> = { message: 'Messages', position: 'Positions', hey: 'HEY beacons', other: 'Other' };

export const isStatsRange = (v: unknown): v is StatsRange => STATS_RANGES.includes(v as StatsRange);

export function niceTicks(max: number, count = 4): number[] {
  if (!(max > 0)) return [0, 1];
  const raw = max / count;
  const power = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 5, 10].map((m) => m * power).find((s) => s >= raw) ?? 10 * power;
  const top = Math.ceil(max / step) * step;
  return Array.from({ length: Math.round(top / step) + 1 }, (_, i) => i * step);
}

export const labelStride = (columns: number, max: number): number => Math.max(1, Math.ceil(columns / Math.max(1, max)));

export function bucketLabel(iso: string, bucket: 'hour' | 'day', long = false, withDay = false): string {
  const d = new Date(iso);
  if (bucket === 'day') return d.toLocaleDateString([], { day: 'numeric', month: 'short', ...(long ? { weekday: 'short' } : {}) });
  const time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (long) return `${d.toLocaleDateString([], { day: 'numeric', month: 'short', weekday: 'short' })} ${time}`;
  return withDay ? `${d.toLocaleDateString([], { weekday: 'short' })} ${time}` : time;
}

export const kindTotal = (k: KindsView): number => k.message + k.position + k.hey + k.other;

export function percent(n: number, total: number): string {
  if (total <= 0) return '—';
  const p = (100 * n) / total;
  return p > 0 && p < 1 ? '<1%' : `${Math.round(p)}%`;
}

export const num = (n: number): string => n.toLocaleString();

export const hopsLabel = (h: { hops: number; or_more: boolean }): string => (h.hops === 0 ? 'direct' : `${h.hops}${h.or_more ? '+' : ''}`);
