export interface BaseMap {
  name: string;
  url: string;
  attribution: string;
  maxZoom: number;
}

export interface PageConfig {
  apiPrefix: string;
  version: string;
  repository: string;
  limits: number[];
  defaultLimit: number;
  baseMaps: BaseMap[];
}

declare global {
  interface Window {
    PAGE_CONFIG: Partial<PageConfig> | null;
  }
}

export const DEFAULT_CONFIG: PageConfig = {
  apiPrefix: '/api',
  version: '',
  repository: '',
  limits: [10, 30, 100, 500, 1000],
  defaultLimit: 30,
  baseMaps: [
    {
      name: 'OpenStreetMap',
      url: 'https://tile.openstreetmap.org/{z}/{x}/{y}.png',
      attribution: '© OpenStreetMap contributors',
      maxZoom: 19,
    },
  ],
};

export function pageConfig(raw: Partial<PageConfig> | null | undefined): PageConfig {
  const merged = { ...DEFAULT_CONFIG, ...(raw ?? {}) };
  return { ...merged, baseMaps: merged.baseMaps.length ? merged.baseMaps : DEFAULT_CONFIG.baseMaps };
}

export const CONFIG: PageConfig = pageConfig(typeof window === 'undefined' ? null : window.PAGE_CONFIG);

export const api = (path: string): string => `${CONFIG.apiPrefix}/public/v1${path}`;
