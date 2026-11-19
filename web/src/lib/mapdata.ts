import type { Feature, FeatureCollection, LineString, Point } from 'geojson';

import { snrColor, linkKey } from './format';
import { along, geodesic, type LonLat } from './geo';
import type { AutoZoomMode } from './settings';
import type { FrameView, LinkView, NodeView, PathView } from './types';

export interface LineProps {
  from: string;
  to: string;
  color: string;
  measured: boolean;
}

export interface ArrowProps {
  bearing: number;
  color: string;
}

export interface Drawing {
  lines: FeatureCollection<LineString, LineProps>;
  arrows: FeatureCollection<Point, ArrowProps>;
}

const lonLat = (call: string, nodes: Record<string, NodeView>): LonLat | null => {
  const p = nodes[call]?.position;
  return p ? [p.lon, p.lat] : null;
};

function segment(a: LonLat, b: LonLat, props: LineProps, arrowAt: number): [Feature<LineString, LineProps>, Feature<Point, ArrowProps>] {
  const coordinates = geodesic(a, b);
  const { point, bearing } = along(coordinates, arrowAt);
  return [
    { type: 'Feature', geometry: { type: 'LineString', coordinates }, properties: props },
    { type: 'Feature', geometry: { type: 'Point', coordinates: point }, properties: { bearing, color: props.color } },
  ];
}

function drawing(pairs: [Feature<LineString, LineProps>, Feature<Point, ArrowProps>][]): Drawing {
  return {
    lines: { type: 'FeatureCollection', features: pairs.map(([l]) => l) },
    arrows: { type: 'FeatureCollection', features: pairs.map(([, a]) => a) },
  };
}

export function linksDrawing(links: LinkView[], nodes: Record<string, NodeView>): Drawing {
  const present = new Set(links.map((l) => linkKey(l.from, l.to)));
  return drawing(
    links.flatMap((l) => {
      const a = lonLat(l.from, nodes);
      const b = lonLat(l.to, nodes);
      if (!a || !b) return [];
      const both = present.has(linkKey(l.to, l.from));
      const props = { from: l.from, to: l.to, color: snrColor(l.snr), measured: l.snr !== null };
      return [segment(a, b, props, both ? 0.65 : 0.5)];
    }),
  );
}

export function chainDrawing(calls: string[], nodes: Record<string, NodeView>, color: string): Drawing {
  return drawing(
    calls.slice(1).flatMap((to, i) => {
      const from = calls[i] as string;
      const a = lonLat(from, nodes);
      const b = lonLat(to, nodes);
      return a && b ? [segment(a, b, { from, to, color, measured: true }, 0.5)] : [];
    }),
  );
}

export function chainsDrawing(chains: string[][], nodes: Record<string, NodeView>, color: string): Drawing {
  const parts = chains.map((c) => chainDrawing(c, nodes, color));
  return {
    lines: { type: 'FeatureCollection', features: parts.flatMap((p) => p.lines.features) },
    arrows: { type: 'FeatureCollection', features: parts.flatMap((p) => p.arrows.features) },
  };
}

export function pathsThrough(paths: PathView[], from: string, to: string): PathView[] {
  return paths
    .filter((p) => p.calls.some((c, i) => i > 0 && p.calls[i - 1] === from && c === to))
    .sort((a, b) => Date.parse(b.last_seen ?? '') - Date.parse(a.last_seen ?? ''));
}

export function positions(calls: string[], nodes: Record<string, NodeView>): LonLat[] {
  return calls.map((c) => lonLat(c, nodes)).filter((p): p is LonLat => p !== null);
}

export function autoZoomTargets(
  mode: AutoZoomMode,
  frames: FrameView[],
  links: LinkView[],
  nodes: Record<string, NodeView>,
): LonLat[] {
  const newest = frames[0];
  switch (mode) {
    case 'last':
      return newest ? positions(newest.path, nodes) : [];
    case 'paths': {
      const ends = links.filter((l) => nodes[l.from]?.position && nodes[l.to]?.position).flatMap((l) => [l.from, l.to]);
      return positions([...new Set(ends)], nodes);
    }
    case 'stations':
      return positions([...new Set(frames.flatMap((f) => f.path))], nodes);
  }
}

export const EMPTY: Drawing = {
  lines: { type: 'FeatureCollection', features: [] },
  arrows: { type: 'FeatureCollection', features: [] },
};
