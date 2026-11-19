export type Kind = 'message' | 'position' | 'hey' | 'other';
export const KINDS: Kind[] = ['message', 'position', 'hey', 'other'];

export interface Position {
  lat: number;
  lon: number;
  alt_m?: number;
}

export interface NodeView {
  call: string;
  own: boolean;
  position: Position | null;
  hw: number | null;
  firmware: string | null;
  frames: number;
  last_seen: string;
}

export interface LinkView {
  from: string;
  to: string;
  count: number;
  last_seen: string;
  rssi: number | null;
  snr: number | null;
  signal_seen: string | null;
}

export interface PathHopView {
  from: string;
  to: string;
  rssi: number | null;
  snr: number | null;
  signal_seen: string | null;
}

export interface KindsView {
  message: number;
  position: number;
  hey: number;
  other: number;
}

export interface PathView {
  calls: string[];
  count: number;
  first_seen: string | null;
  last_seen: string | null;
  hops: PathHopView[];
  kinds: KindsView;
}

export interface FrameView {
  id: string;
  received_at: string;
  source: string;
  msg_id?: string;
  copy: number;
  origin: string;
  relays: string[];
  path: string[];
  rssi: number | null;
  snr: number | null;
  hw: number | null;
  firmware: string | null;
  kind: Kind;
  dest?: string;
  text?: string;
  position?: Position;
  neighbours?: number;
  hops?: { hw: number; rssi: number; snr: number }[];
  type_char?: string;
}

export interface ViewChoice {
  limit: number | null;
  kinds: Kind[];
  copies: boolean;
}

export interface ChatReceptionView {
  id: string;
  received_at: string;
  source: string;
  relays: string[];
  path: string[];
  rssi: number | null;
  snr: number | null;
}

export interface ChatMessageView {
  key: string;
  msg_id?: string;
  received_at: string;
  origin: string;
  dest: string;
  text: string;
  own: boolean;
  hw: number | null;
  firmware: string | null;
  receptions: ChatReceptionView[];
}

export interface ChatTab {
  target: string;
  label: string;
}

export interface ChatTargetView {
  target: string;
  label: string;
  kind: 'channel' | 'callsign' | 'broadcast';
  messages: number;
  last_seen: string | null;
}

export interface ChatPage {
  target: string;
  messages: ChatMessageView[];
  more: boolean;
}

export type ServerMessage =
  | { type: 'snapshot'; view: ViewChoice; nodes: NodeView[]; frames: FrameView[]; links: LinkView[]; paths: PathView[] }
  | { type: 'frame'; frame: FrameView; dropped: string[]; links: LinkView[]; paths: PathView[] }
  | { type: 'node'; node: NodeView }
  | { type: 'chat'; target: string; message: ChatMessageView }
  | { type: 'error'; detail: string };
