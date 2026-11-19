import { DEFAULT_STATS_RANGE, isStatsRange, type StatsRange } from './stats';
import { KINDS, type Kind } from './types';

export type ViewName = 'map' | 'chat' | 'stats';
export type PanelTab = 'traffic' | 'nodes' | 'legend';

export interface Overlays {
  links: boolean;
  nodes: boolean;
  labels: boolean;
  paths: boolean;
}

export type AutoZoomMode = 'last' | 'paths' | 'stations';
export const AUTO_ZOOM_MODES: AutoZoomMode[] = ['last', 'paths', 'stations'];

export interface AutoZoom {
  enabled: boolean;
  mode: AutoZoomMode;
}

export const DEFAULT_AUTO_ZOOM: AutoZoom = { enabled: true, mode: 'last' };

export interface Settings {
  view: ViewName;
  limit: number | 'all' | null;
  kinds: Kind[] | null;
  copies: boolean | null;
  tab: PanelTab;
  map: { lat: number; lon: number; zoom: number } | null;
  baseMap: string | null;
  overlays: Overlays;
  dimBase: boolean;
  autoZoom: AutoZoom;
  chatTabs: { target: string; label: string }[];
  chatActive: string | null;
  statsRange: StatsRange;
}

export const SETTINGS_KEY = 'meshcom-status.settings.v1';

export const DEFAULT_OVERLAYS: Overlays = { links: true, nodes: true, labels: true, paths: true };

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

export function parseSettings(raw: unknown): Settings {
  const s = isObject(raw) ? raw : {};
  const map = isObject(s.map) && [s.map.lat, s.map.lon, s.map.zoom].every((n) => Number.isFinite(n))
    ? { lat: s.map.lat as number, lon: s.map.lon as number, zoom: s.map.zoom as number }
    : null;
  const overlays = isObject(s.overlays)
    ? (Object.fromEntries(
        Object.entries(DEFAULT_OVERLAYS).map(([k, d]) => [k, typeof (s.overlays as Record<string, unknown>)[k] === 'boolean' ? (s.overlays as Record<string, boolean>)[k] : d]),
      ) as unknown as Overlays)
    : DEFAULT_OVERLAYS;
  return {
    view: s.view === 'chat' || s.view === 'stats' ? s.view : 'map',
    limit: s.limit === 'all' || (typeof s.limit === 'number' && Number.isInteger(s.limit) && s.limit > 0) ? (s.limit as number | 'all') : null,
    kinds: Array.isArray(s.kinds) ? (s.kinds.filter((k) => KINDS.includes(k as Kind)) as Kind[]) : null,
    copies: typeof s.copies === 'boolean' ? s.copies : null,
    tab: s.tab === 'nodes' || s.tab === 'legend' ? s.tab : 'traffic',
    map,
    baseMap: typeof s.baseMap === 'string' ? s.baseMap : null,
    overlays,
    dimBase: typeof s.dimBase === 'boolean' ? s.dimBase : true,
    autoZoom: isObject(s.autoZoom)
      ? {
          enabled: typeof s.autoZoom.enabled === 'boolean' ? s.autoZoom.enabled : DEFAULT_AUTO_ZOOM.enabled,
          mode: AUTO_ZOOM_MODES.includes(s.autoZoom.mode as AutoZoomMode) ? (s.autoZoom.mode as AutoZoomMode) : DEFAULT_AUTO_ZOOM.mode,
        }
      : DEFAULT_AUTO_ZOOM,
    chatTabs: Array.isArray(s.chatTabs)
      ? s.chatTabs.filter((t): t is { target: string; label: string } => isObject(t) && typeof t.target === 'string' && typeof t.label === 'string')
      : [],
    chatActive: typeof s.chatActive === 'string' ? s.chatActive : null,
    statsRange: isStatsRange(s.statsRange) ? s.statsRange : DEFAULT_STATS_RANGE,
  };
}

export function loadSettings(): Settings {
  try {
    return parseSettings(JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? '{}'));
  } catch {
    return parseSettings({});
  }
}

let current: Settings | null = null;

export function settings(): Settings {
  current ??= loadSettings();
  return current;
}

export function saveSettings(changes: Partial<Settings>): void {
  current = { ...settings(), ...changes };
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(current));
  } catch {
  }
}
