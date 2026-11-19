import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import MapGL, {
  AttributionControl,
  Layer,
  Marker,
  NavigationControl,
  Popup,
  ScaleControl,
  Source,
  type MapLayerMouseEvent,
  type MapRef,
} from 'react-map-gl/maplibre';
import type { RasterLayerSpecification, StyleSpecification } from 'maplibre-gl';

import { CONFIG, type BaseMap } from '../lib/config';
import { ago, SNR_COLORS } from '../lib/format';
import { useLive } from '../lib/live';
import { wheelPixels, wheelStep } from '../lib/geo';
import { EMPTY, autoZoomTargets, chainDrawing, chainsDrawing, linksDrawing, pathsThrough, positions, type Drawing } from '../lib/mapdata';
import { saveSettings, settings, type AutoZoom, type Overlays } from '../lib/settings';
import type { NodeView, PathView } from '../lib/types';
import { AutoZoomControl } from './AutoZoomControl';
import { LayerSwitcher } from './LayerSwitcher';
import type { Focus, Selection } from './MapView';

const PATH_COLOR = '#2563eb';
const HOVER_COLOR = '#06b6d4';
const FLASH_MS = 4000;
const PAUSE_MS = 30000;
const ARROW_COLORS = [...Object.values(SNR_COLORS), PATH_COLOR, HOVER_COLOR];
const arrowId = (color: string) => `arrow-${color}`;
const DASH_FRAMES: number[][] = [
  [0, 2, 1.5],
  [0.5, 2, 1],
  [1, 2, 0.5],
  [1.5, 2, 0],
  [0, 0.5, 2, 1],
  [0, 1, 2, 0.5],
  [0, 1.5, 2],
];

export function basePaint(dim: boolean): RasterLayerSpecification['paint'] {
  return dim ? { 'raster-brightness-max': 0.72, 'raster-saturation': -0.35 } : {};
}

function baseStyle(b: BaseMap, dim: boolean): StyleSpecification {
  return {
    version: 8,
    sources: { base: { type: 'raster', tiles: [b.url], tileSize: 256, maxzoom: b.maxZoom, attribution: b.attribution } },
    layers: [{ id: 'base', type: 'raster', source: 'base', paint: basePaint(dim) }],
  };
}

const mapMaxZoom = (b: BaseMap | undefined) => (b?.maxZoom ?? 19) - 1;

function arrowImage(color: string): ImageData | null {
  const ratio = 2;
  const size = 16 * ratio;
  const canvas = document.createElement('canvas');
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext('2d');
  if (!ctx) return null;
  ctx.scale(ratio, ratio);
  ctx.translate(8, 8);
  ctx.beginPath();
  ctx.moveTo(0, -7);
  ctx.lineTo(6, 5);
  ctx.lineTo(0, 2);
  ctx.lineTo(-6, 5);
  ctx.closePath();
  ctx.lineJoin = 'round';
  ctx.lineWidth = 1.2;
  ctx.strokeStyle = '#ffffff';
  ctx.stroke();
  ctx.fillStyle = color;
  ctx.fill();
  return ctx.getImageData(0, 0, size, size);
}

function fitWhole(map: MapRef | null, pts: [number, number][], padding: number, animate: boolean) {
  if (!map || !pts.length) return;
  const lons = pts.map((p) => p[0]);
  const lats = pts.map((p) => p[1]);
  const camera = map.cameraForBounds(
    [
      [Math.min(...lons), Math.min(...lats)],
      [Math.max(...lons), Math.max(...lats)],
    ],
    { padding, maxZoom: 12 },
  );
  if (!camera) return;
  const target = { ...camera, zoom: Math.floor(camera.zoom ?? map.getZoom()) };
  if (animate) map.easeTo(target);
  else map.jumpTo(target);
}

const visibility = (on: boolean) => ({ visibility: on ? ('visible' as const) : ('none' as const) });

const OPACITY = 0.85;
const BLUR = 0.5;

const arrowLayout = (on: boolean, image: string | unknown[], size: number) => ({
  ...visibility(on),
  'icon-image': image as string,
  'icon-size': size,
  'icon-rotate': ['get', 'bearing'] as unknown as number,
  'icon-rotation-alignment': 'map' as const,
  'icon-allow-overlap': true,
  'icon-ignore-placement': true,
});

function Lines({ id, drawing, color, width, dashed, on, arrows }: { id: string; drawing: Drawing; color: string; width: number; dashed?: boolean; on: boolean; arrows: boolean }) {
  return (
    <>
      <Source id={`${id}-lines`} type="geojson" data={drawing.lines}>
        <Layer
          id={`${id}-line`}
          type="line"
          layout={{ ...visibility(on), 'line-cap': dashed ? 'butt' : 'round', 'line-join': 'round' }}
          paint={{ 'line-color': color, 'line-width': width, 'line-opacity': 0.9, 'line-blur': BLUR, ...(dashed ? { 'line-dasharray': DASH_FRAMES[0] } : {}) }}
        />
      </Source>
      {arrows && (
        <Source id={`${id}-arrows`} type="geojson" data={drawing.arrows}>
          <Layer id={`${id}-arrow`} type="symbol" layout={arrowLayout(on, arrowId(color), 1.25)} paint={{ 'icon-opacity': 0.9 }} />
        </Source>
      )}
    </>
  );
}

function NodePopup({ node }: { node: NodeView }) {
  const { links } = useLive();
  const hears = links.filter((l) => l.to === node.call).map((l) => l.from);
  const heardBy = links.filter((l) => l.from === node.call).map((l) => l.to);
  const p = node.position;
  const rows: [string, string][] = [
    ['Position', p ? `${p.lat.toFixed(5)}, ${p.lon.toFixed(5)}${p.alt_m != null ? `, ${p.alt_m} m` : ''}` : 'unknown'],
    ['Hardware', node.hw != null ? String(node.hw) : '—'],
    ['Firmware', node.firmware ?? '—'],
    ['Frames sent', String(node.frames)],
    ['Last seen', ago(node.last_seen)],
    ['Hears', hears.join(', ') || '—'],
    ['Heard by', heardBy.join(', ') || '—'],
  ];
  return (
    <div className="popup">
      <h3>
        {node.call}
        {node.own ? ' (this node)' : ''}
      </h3>
      <table>
        <tbody>
          {rows.map(([k, v]) => (
            <tr key={k}>
              <td>{k}</td>
              <td>{v}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

interface Props {
  visible: boolean;
  selection: Selection | null;
  selectedPath: PathView | null;
  focus: Focus | null;
  popup: string | null;
  onPopup: (call: string | null) => void;
  onSelectSegment: (from: string, to: string) => void;
}

export function MapPane({ visible, selection, selectedPath, focus, popup, onPopup, onSelectSegment }: Props) {
  const live = useLive();
  const mapRef = useRef<MapRef | null>(null);
  const [loaded, setLoaded] = useState(false);
  const saved = settings();
  const [baseName, setBaseName] = useState(saved.baseMap ?? CONFIG.baseMaps[0]?.name ?? '');
  const [overlays, setOverlays] = useState<Overlays>(saved.overlays);
  const [dimBase, setDimBase] = useState(saved.dimBase);
  const [hover, setHover] = useState<{ from: string; to: string; x: number; y: number } | null>(null);
  const [flashOver, setFlashOver] = useState<string | null>(null);
  const fitted = useRef(saved.map !== null);
  const [autoZoom, setAutoZoom] = useState<AutoZoom>(saved.autoZoom);
  const [pausedUntil, setPausedUntil] = useState(0);
  const [now, setNow] = useState(() => Date.now());
  const pausedRef = useRef(false);
  const paused = pausedUntil > now;
  useEffect(() => {
    pausedRef.current = paused;
  }, [paused]);

  const base = CONFIG.baseMaps.find((b) => b.name === baseName) ?? CONFIG.baseMaps[0];
  const mapStyle = useMemo(() => (base ? baseStyle(base, dimBase) : undefined), [base, dimBase]);

  const links = useMemo(() => linksDrawing(live.links, live.nodes), [live.links, live.nodes]);
  const hovered = useMemo(
    () => (hover ? chainsDrawing(pathsThrough(live.paths, hover.from, hover.to).map((p) => p.calls), live.nodes, HOVER_COLOR) : EMPTY),
    [hover, live.paths, live.nodes],
  );
  const selected = useMemo(() => (selectedPath ? chainDrawing(selectedPath.calls, live.nodes, PATH_COLOR) : EMPTY), [selectedPath, live.nodes]);
  const flash = live.fresh && live.fresh.id !== flashOver ? live.fresh.path : null;
  const flashed = useMemo(() => (flash && !selection ? chainDrawing(flash, live.nodes, PATH_COLOR) : EMPTY), [flash, selection, live.nodes]);
  useEffect(() => {
    const fresh = live.fresh;
    if (!fresh) return;
    const t = setTimeout(() => setFlashOver(fresh.id), FLASH_MS);
    return () => clearTimeout(t);
  }, [live.fresh]);

  useEffect(() => {
    if (visible) mapRef.current?.resize();
  }, [visible]);

  useEffect(() => {
    if (fitted.current || !loaded || autoZoom.enabled) return;
    const pts = Object.values(live.nodes).filter((n) => n.position).map((n) => [n.position!.lon, n.position!.lat] as [number, number]);
    if (!pts.length) return;
    fitted.current = true;
    fitWhole(mapRef.current, pts, 60, false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [live.nodes, loaded]);

  const newest = live.frames[0]?.id;
  useEffect(() => {
    if (!loaded || !visible || !autoZoom.enabled || pausedRef.current || selection) return;
    const pts = autoZoomTargets(autoZoom.mode, live.frames, live.links, live.nodes);
    if (!pts.length) return;
    fitted.current = true;
    fitWhole(mapRef.current, pts, 80, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [newest, autoZoom, loaded, visible, selection]);

  useEffect(() => {
    if (pausedUntil <= Date.now()) return;
    const timer = setInterval(() => {
      setNow(Date.now());
      if (Date.now() >= pausedUntil) clearInterval(timer);
    }, 1000);
    return () => clearInterval(timer);
  }, [pausedUntil]);

  const changeAutoZoom = (a: AutoZoom) => {
    setAutoZoom(a);
    saveSettings({ autoZoom: a });
    setPausedUntil(0);
  };

  useEffect(() => {
    if (!selection?.fit || !selectedPath || !loaded) return;
    const pts = positions(selectedPath.calls, live.nodes);
    if (pts.length === 1) mapRef.current?.panTo(pts[0]!);
    if (pts.length > 1) fitWhole(mapRef.current, pts, 80, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selection, loaded]);

  useEffect(() => {
    if (!focus || !loaded) return;
    const p = live.nodes[focus.call]?.position;
    if (p) mapRef.current?.flyTo({ center: [p.lon, p.lat], zoom: Math.max(mapRef.current.getZoom(), 11) });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focus, loaded]);

  const attachMap = useCallback((ref: MapRef | null) => {
    mapRef.current = ref;
    const map = ref?.getMap();
    if (!map) return;
    map.setMissingStyleImageResolver((id: string) => {
      const color = ARROW_COLORS.find((c) => arrowId(c) === id);
      const img = color ? arrowImage(color) : null;
      if (img && !map.hasImage(id)) map.addImage(id, img, { pixelRatio: 2 });
    });
  }, []);

  const onLoad = useCallback(() => {
    const map = mapRef.current?.getMap();
    if (!map) return;
    map.on('movestart', (e: { originalEvent?: unknown }) => {
      if (e.originalEvent) {
        const t = Date.now();
        setNow(t);
        setPausedUntil(t + PAUSE_MS);
      }
    });
    let wheelPx = 0;
    let wheelTimer: ReturnType<typeof setTimeout> | undefined;
    let wheelAt: [number, number] = [0, 0];
    let wheelTarget: number | null = null;
    const container = map.getCanvasContainer();
    container.addEventListener(
      'wheel',
      (e: WheelEvent) => {
        e.preventDefault();
        wheelPx += wheelPixels(e.deltaY, e.deltaMode);
        const box = container.getBoundingClientRect();
        wheelAt = [e.clientX - box.left, e.clientY - box.top];
        clearTimeout(wheelTimer);
        wheelTimer = setTimeout(() => {
          const steps = wheelStep(wheelPx);
          wheelPx = 0;
          if (!steps) return;
          const from = wheelTarget ?? Math.round(map.getZoom());
          const zoom = Math.max(map.getMinZoom(), Math.min(map.getMaxZoom(), from + steps));
          if (zoom === from) return;
          map.easeTo({ zoom, around: map.unproject(wheelAt), duration: 200 }, { originalEvent: e });
          wheelTarget = zoom;
        }, 40);
      },
      { passive: false },
    );
    map.on('moveend', () => {
      wheelTarget = null;
    });
    map.on('zoomend', () => {
      const z = map.getZoom();
      const whole = Math.round(z);
      if (wheelTarget === null && Math.abs(z - whole) > 0.01) map.easeTo({ zoom: whole, duration: 150 });
    });
    setLoaded(true);
  }, []);

  useEffect(() => {
    if (!loaded) return;
    const map = mapRef.current?.getMap();
    let step = 0;
    const timer = setInterval(() => {
      step = (step + 1) % DASH_FRAMES.length;
      ['selected-line', 'flash-line'].forEach((id) => {
        if (map?.getLayer(id)) map.setPaintProperty(id, 'line-dasharray', DASH_FRAMES[step]);
      });
    }, 90);
    return () => clearInterval(timer);
  }, [loaded]);

  const onMove = useCallback((e: MapLayerMouseEvent) => {
    const f = e.features?.[0];
    const props = f?.properties as { from?: string; to?: string } | undefined;
    if (props?.from && props.to) setHover({ from: props.from, to: props.to, x: e.point.x, y: e.point.y });
    else setHover(null);
  }, []);

  const onClick = useCallback(
    (e: MapLayerMouseEvent) => {
      const props = e.features?.[0]?.properties as { from?: string; to?: string } | undefined;
      if (props?.from && props.to) onSelectSegment(props.from, props.to);
    },
    [onSelectSegment],
  );

  const changeBase = (name: string) => {
    setBaseName(name);
    saveSettings({ baseMap: name });
  };
  const changeDimBase = (dim: boolean) => {
    setDimBase(dim);
    saveSettings({ dimBase: dim });
  };
  const changeOverlays = (o: Overlays) => {
    setOverlays(o);
    saveSettings({ overlays: o });
    if (!o.links) setHover(null);
  };

  const hoverCount = hover ? pathsThrough(live.paths, hover.from, hover.to).length : 0;
  const popupNode = popup ? live.nodes[popup] : undefined;
  const view = saved.map ? { latitude: saved.map.lat, longitude: saved.map.lon, zoom: saved.map.zoom } : { latitude: 20, longitude: 0, zoom: 2 };

  return (
    <>
      <MapGL
        ref={attachMap}
        initialViewState={view}
        mapStyle={mapStyle}
        maxZoom={mapMaxZoom(base)}
        attributionControl={false}
        canvasContextAttributes={{ antialias: true }}
        interactiveLayerIds={overlays.links ? ['links-hit'] : []}
        cursor={hover ? 'pointer' : 'grab'}
        onLoad={onLoad}
        scrollZoom={false}
        onMouseMove={onMove}
        onMouseLeave={() => setHover(null)}
        onClick={onClick}
        onMoveEnd={(e) => {
          const c = e.viewState;
          saveSettings({ map: { lat: c.latitude, lon: c.longitude, zoom: c.zoom } });
        }}
        style={{ position: 'absolute', inset: 0 }}
      >
        <NavigationControl position="top-right" showCompass visualizePitch />
        <ScaleControl position="bottom-left" />
        <AttributionControl position="bottom-right" compact />

        <Source id="links-lines" type="geojson" data={links.lines}>
          <Layer
            id="links-line"
            type="line"
            filter={['==', ['get', 'measured'], true]}
            layout={{ ...visibility(overlays.links), 'line-cap': 'round', 'line-join': 'round' }}
            paint={{ 'line-color': ['get', 'color'], 'line-width': 3, 'line-opacity': OPACITY, 'line-blur': BLUR }}
          />
          <Layer
            id="links-dashed"
            type="line"
            filter={['==', ['get', 'measured'], false]}
            layout={{ ...visibility(overlays.links), 'line-join': 'round' }}
            paint={{ 'line-color': ['get', 'color'], 'line-width': 2, 'line-opacity': OPACITY, 'line-blur': BLUR, 'line-dasharray': [3, 3] }}
          />
          <Layer id="links-hit" type="line" layout={visibility(overlays.links)} paint={{ 'line-color': '#000', 'line-width': 16, 'line-opacity': 0 }} />
        </Source>
        <Source id="links-arrows" type="geojson" data={links.arrows}>
          <Layer
            id="links-arrow"
            type="symbol"
            layout={arrowLayout(overlays.links, ['concat', 'arrow-', ['get', 'color']], 1)}
            paint={{ 'icon-opacity': OPACITY }}
          />
        </Source>

        <Lines id="hover" drawing={hovered} color={HOVER_COLOR} width={7} on={overlays.paths} arrows={false} />
        <Lines id="flash" drawing={flashed} color={PATH_COLOR} width={5} dashed on={overlays.paths} arrows />
        <Lines id="selected" drawing={selected} color={PATH_COLOR} width={5} dashed on arrows />

        {overlays.nodes &&
          Object.values(live.nodes)
            .filter((n) => n.position)
            .map((n) => (
              <Marker
                key={n.call}
                longitude={n.position!.lon}
                latitude={n.position!.lat}
                anchor="left"
                offset={[-8, 0]}
                onClick={(e) => {
                  e.originalEvent.stopPropagation();
                  onPopup(n.call);
                }}
              >
                <div className={`node-marker${n.own ? ' own' : ''}`} title={n.call}>
                  <span className="dot" />
                  {overlays.labels && <span className="label">{n.call}</span>}
                </div>
              </Marker>
            ))}

        {popupNode?.position && (
          <Popup longitude={popupNode.position.lon} latitude={popupNode.position.lat} offset={12} closeOnClick={false} onClose={() => onPopup(null)} maxWidth="320px">
            <NodePopup node={popupNode} />
          </Popup>
        )}
      </MapGL>

      <div className="map-tools">
        <LayerSwitcher baseMaps={CONFIG.baseMaps} base={base?.name ?? ''} onBase={changeBase} overlays={overlays} onOverlays={changeOverlays} dimBase={dimBase} onDimBase={changeDimBase} />
        <AutoZoomControl
          value={autoZoom}
          onChange={changeAutoZoom}
          pausedFor={paused ? Math.ceil((pausedUntil - now) / 1000) : 0}
          onResume={() => changeAutoZoom(autoZoom)}
        />
      </div>

      {hover && (
        <div className="map-tooltip" style={{ left: hover.x + 14, top: hover.y + 14 }}>
          <b>
            {hover.from} → {hover.to}
          </b>
          <br />
          {hoverCount} path{hoverCount === 1 ? '' : 's'} · click for details
        </div>
      )}
    </>
  );
}
