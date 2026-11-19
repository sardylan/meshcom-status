export function ago(iso: string | null | undefined, now: number = Date.now()): string {
  if (!iso) return '—';
  const s = Math.max(0, Math.round((now - Date.parse(iso)) / 1000));
  if (s < 60) return `${s} s ago`;
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  if (s < 86400) return `${Math.round(s / 3600)} h ago`;
  return `${Math.round(s / 86400)} d ago`;
}

export const clock = (iso: string): string =>
  new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });

export const signalText = (rssi: number | null | undefined, snr: number | null | undefined): string =>
  snr === null || snr === undefined ? 'not measured' : `${rssi} dBm, SNR ${snr} dB`;

export const SNR_COLORS = {
  great: '#1a9850',
  good: '#66bd63',
  fair: '#e6a817',
  poor: '#f46d43',
  bad: '#d73027',
  none: '#6b7280',
};

export function snrColor(snr: number | null | undefined): string {
  if (snr === null || snr === undefined) return SNR_COLORS.none;
  if (snr >= 5) return SNR_COLORS.great;
  if (snr >= 0) return SNR_COLORS.good;
  if (snr >= -7) return SNR_COLORS.fair;
  if (snr >= -14) return SNR_COLORS.poor;
  return SNR_COLORS.bad;
}

export const KIND_LABEL: Record<string, string> = { message: 'MSG', position: 'POS', hey: 'HEY', other: '…' };

export const pathKey = (calls: string[]): string => calls.join('>');
export const linkKey = (from: string, to: string): string => `${from}\u0000${to}`;
