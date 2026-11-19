import { createContext, useCallback, useContext, useEffect, useMemo, useReducer, useRef, type ReactNode } from 'react';

import { CONFIG } from './config';
import { saveSettings, settings } from './settings';
import { KINDS, type ChatMessageView, type FrameView, type LinkView, type NodeView, type PathView, type ServerMessage, type ViewChoice } from './types';

export type Status = 'connecting' | 'open' | 'closed';

interface State {
  status: Status;
  nodes: Record<string, NodeView>;
  links: LinkView[];
  paths: PathView[];
  frames: FrameView[];
  fresh: FrameView | null;
}

type Action =
  | { type: 'status'; status: Status }
  | { type: 'message'; message: ServerMessage };

const INITIAL: State = { status: 'connecting', nodes: {}, links: [], paths: [], frames: [], fresh: null };

export function reduce(state: State, action: Action): State {
  if (action.type === 'status') return { ...state, status: action.status };
  const m = action.message;
  switch (m.type) {
    case 'snapshot':
      return {
        ...state,
        nodes: Object.fromEntries(m.nodes.map((n) => [n.call, n])),
        links: m.links,
        paths: m.paths,
        frames: [...m.frames].reverse(),
        fresh: null,
      };
    case 'frame': {
      const gone = new Set(m.dropped);
      return {
        ...state,
        links: m.links,
        paths: m.paths,
        frames: [m.frame, ...state.frames.filter((f) => !gone.has(f.id))],
        fresh: m.frame,
      };
    }
    case 'node':
      return { ...state, nodes: { ...state.nodes, [m.node.call]: m.node } };
    default:
      return state;
  }
}

export function initialView(): ViewChoice {
  const s = settings();
  const limit = s.limit === 'all' ? null : s.limit !== null && CONFIG.limits.includes(s.limit) ? s.limit : CONFIG.defaultLimit;
  return { limit, kinds: s.kinds ?? KINDS, copies: s.copies ?? true };
}

type ChatListener = (target: string, message: ChatMessageView) => void;

interface Live extends State {
  view: ViewChoice;
  setView: (view: ViewChoice) => void;
  followChat: (target: string | null) => void;
  onChat: (listener: ChatListener) => () => void;
  connection: number;
}

const LiveContext = createContext<Live | null>(null);

export function useLive(): Live {
  const live = useContext(LiveContext);
  if (!live) throw new Error('useLive outside LiveProvider');
  return live;
}

export function LiveProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reduce, INITIAL);
  const [view, setViewState] = useReducer((_: ViewChoice, v: ViewChoice) => v, undefined, initialView);
  const [connection, bump] = useReducer((n: number) => n + 1, 0);
  const socket = useRef<WebSocket | null>(null);
  const viewRef = useRef(view);
  const chatRef = useRef<string | null>(null);
  const listeners = useRef(new Set<ChatListener>());

  const send = useCallback((message: unknown) => {
    const ws = socket.current;
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(message));
  }, []);

  useEffect(() => {
    let retry = 1000;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let stopped = false;
    const connect = () => {
      const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
      const ws = new WebSocket(`${proto}//${location.host}${CONFIG.apiPrefix}/public/v1/ws`);
      socket.current = ws;
      dispatch({ type: 'status', status: 'connecting' });
      ws.onopen = () => {
        retry = 1000;
        dispatch({ type: 'status', status: 'open' });
        ws.send(JSON.stringify({ type: 'view', ...viewRef.current }));
        ws.send(JSON.stringify({ type: 'chat', target: chatRef.current }));
        bump();
      };
      ws.onmessage = (e) => {
        try {
          const message = JSON.parse(e.data as string) as ServerMessage;
          if (message.type === 'chat') listeners.current.forEach((l) => l(message.target, message.message));
          else if (message.type === 'error') console.warn('server refused:', message.detail);
          else dispatch({ type: 'message', message });
        } catch (err) {
          console.error('bad message', err);
        }
      };
      ws.onclose = () => {
        if (stopped) return;
        dispatch({ type: 'status', status: 'closed' });
        timer = setTimeout(connect, retry);
        retry = Math.min(retry * 2, 30000);
      };
    };
    connect();
    return () => {
      stopped = true;
      clearTimeout(timer);
      socket.current?.close();
    };
  }, []);

  const setView = useCallback(
    (v: ViewChoice) => {
      viewRef.current = v;
      setViewState(v);
      saveSettings({ limit: v.limit === null ? 'all' : v.limit, kinds: v.kinds, copies: v.copies });
      send({ type: 'view', ...v });
    },
    [send],
  );

  const followChat = useCallback(
    (target: string | null) => {
      if (chatRef.current === target) return;
      chatRef.current = target;
      send({ type: 'chat', target });
    },
    [send],
  );

  const onChat = useCallback((listener: ChatListener) => {
    listeners.current.add(listener);
    return () => {
      listeners.current.delete(listener);
    };
  }, []);

  const value = useMemo(
    () => ({ ...state, view, setView, followChat, onChat, connection }),
    [state, view, setView, followChat, onChat, connection],
  );
  return <LiveContext.Provider value={value}>{children}</LiveContext.Provider>;
}
