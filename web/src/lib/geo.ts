export type LonLat = [number, number];

const RAD = Math.PI / 180;
const EARTH_KM = 6371;

function vector([lon, lat]: LonLat): [number, number, number] {
  return [Math.cos(lat * RAD) * Math.cos(lon * RAD), Math.cos(lat * RAD) * Math.sin(lon * RAD), Math.sin(lat * RAD)];
}

export function geodesic(a: LonLat, b: LonLat): LonLat[] {
  const va = vector(a);
  const vb = vector(b);
  const dot = Math.min(1, Math.max(-1, va[0] * vb[0] + va[1] * vb[1] + va[2] * vb[2]));
  const d = Math.acos(dot);
  if (d < 1e-9) return [a, b];
  const n = Math.min(64, Math.max(8, Math.ceil((d * EARTH_KM) / 20)));
  const points: LonLat[] = [];
  for (let i = 0; i <= n; i++) {
    const f = i / n;
    const A = Math.sin((1 - f) * d) / Math.sin(d);
    const B = Math.sin(f * d) / Math.sin(d);
    const x = A * va[0] + B * vb[0];
    const y = A * va[1] + B * vb[1];
    const z = A * va[2] + B * vb[2];
    let lon = Math.atan2(y, x) / RAD;
    const prev = points[points.length - 1];
    if (prev) {
      while (lon - prev[0] > 180) lon -= 360;
      while (lon - prev[0] < -180) lon += 360;
    }
    points.push([lon, Math.atan2(z, Math.hypot(x, y)) / RAD]);
  }
  return points;
}

export function bearing([lon1, lat1]: LonLat, [lon2, lat2]: LonLat): number {
  const p1 = lat1 * RAD;
  const p2 = lat2 * RAD;
  const dl = (lon2 - lon1) * RAD;
  const y = Math.sin(dl) * Math.cos(p2);
  const x = Math.cos(p1) * Math.sin(p2) - Math.sin(p1) * Math.cos(p2) * Math.cos(dl);
  return ((Math.atan2(y, x) / RAD) + 360) % 360;
}

export function along(points: LonLat[], f: number): { point: LonLat; bearing: number } {
  const last = points.length - 1;
  const pos = Math.min(Math.max(f, 0), 1) * last;
  const i = Math.min(last - 1, Math.floor(pos));
  const a = points[i] as LonLat;
  const b = points[i + 1] as LonLat;
  const t = pos - i;
  return { point: [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t], bearing: bearing(a, b) };
}

export function wheelPixels(deltaY: number, deltaMode: number): number {
  if (deltaMode === 1) return deltaY * 40;
  if (deltaMode === 2) return deltaY * 800;
  return deltaY;
}

export function wheelStep(px: number): number {
  return Math.abs(px) < 4 ? 0 : -Math.sign(px);
}
