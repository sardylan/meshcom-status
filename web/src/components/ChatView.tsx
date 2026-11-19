import { useEffect, useEffectEvent, useLayoutEffect, useRef, useState, type FormEvent } from 'react';

import { api } from '../lib/config';
import { ago, clock } from '../lib/format';
import { useLive } from '../lib/live';
import { saveSettings, settings } from '../lib/settings';
import type { ChatMessageView, ChatPage, ChatReceptionView, ChatTab, ChatTargetView } from '../lib/types';

const PAGE = 20;

interface Tab extends ChatTab {
  added: boolean;
}

export function parseTyped(text: string, suggestions: ChatTargetView[]): ChatTab | null {
  const t = text.trim().toUpperCase();
  const match = suggestions.find((s) => s.label.toUpperCase() === t);
  if (match) return { target: match.target, label: match.label };
  if (/^[0-9]{1,6}$/.test(t)) return { target: `channel:${t}`, label: t };
  if (t.length >= 3 && /^[A-Z0-9]{1,8}(-[A-Z0-9]{1,3})?$/.test(t)) return { target: `callsign:${t}`, label: t };
  return null;
}

async function fetchPage(target: string, before: string | null): Promise<ChatPage> {
  const q = new URLSearchParams({ target, limit: String(PAGE) });
  if (before) q.set('before', before);
  const r = await fetch(api(`/chat/messages?${q}`));
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return (await r.json()) as ChatPage;
}

function Via({ r }: { r: ChatReceptionView }) {
  const signal = r.snr === null ? 'signal not measured' : `${r.rssi} dBm / SNR ${r.snr} dB`;
  return (
    <li>
      <span className="route">{r.relays.length ? `via ${r.relays.join(' → ')}` : 'direct'}</span> · {signal} · {clock(r.received_at)}
    </li>
  );
}

function Message({ m, showDest, now }: { m: ChatMessageView; showDest: boolean; now: number }) {
  const dest = m.dest === '' || m.dest === '*' ? 'all' : m.dest;
  return (
    <li className={`msg${m.own ? ' own' : ''}`}>
      <div className="meta">
        <span className="from">{m.origin}</span>
        {showDest && <span>→ {dest}</span>}
        <span>
          {clock(m.received_at)}
          {m.receptions.length > 1 ? ` · received ${m.receptions.length}×` : ''}
        </span>
        <span className="age" title={new Date(m.received_at).toLocaleString()}>
          {ago(m.received_at, now)}
        </span>
      </div>
      <div className="text">{m.text}</div>
      <ul className="vias" title="Paths through which this node received the message">
        {m.receptions.map((r) => (
          <Via key={r.id} r={r} />
        ))}
      </ul>
    </li>
  );
}

export function suggestionLabel(s: ChatTargetView, now: number = Date.now()): string {
  const history = `${s.messages} message${s.messages === 1 ? '' : 's'}${s.last_seen ? ` · ${ago(s.last_seen, now)}` : ''}`;
  return `${s.label} · ${history}`;
}

function AddTab({ open, onAdd, onCancel }: { open: string[]; onAdd: (t: ChatTab) => void; onCancel: () => void }) {
  const [text, setText] = useState('');
  const [error, setError] = useState('');
  const [suggestions, setSuggestions] = useState<ChatTargetView[]>([]);

  useEffect(() => {
    const q = text.trim().toUpperCase();
    if (!/^[A-Z0-9-]{0,12}$/.test(q)) return;
    const t = setTimeout(async () => {
      try {
        const r = await fetch(api(`/chat/targets?${new URLSearchParams({ q, limit: '20', exclude: open.join(',') })}`));
        if (r.ok) setSuggestions(((await r.json()) as { targets: ChatTargetView[] }).targets);
      } catch (e) {
        console.warn('autocomplete failed', e);
      }
    }, 200);
    return () => clearTimeout(t);
  }, [text, open]);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const tab = parseTyped(text, suggestions);
    if (tab) onAdd(tab);
    else setError('Not a channel number or a callsign.');
  };

  return (
    <form className="chat-add-form" autoComplete="off" onSubmit={submit}>
      <input
        autoFocus
        list="chat-suggestions"
        placeholder="Channel (e.g. 12399) or callsign"
        aria-label="Channel or callsign"
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          setError('');
        }}
      />
      <datalist id="chat-suggestions">
        {suggestions.map((s) => (
          <option key={s.target} value={s.label} label={suggestionLabel(s)} />
        ))}
      </datalist>
      <button type="submit">Add</button>
      <button type="button" onClick={onCancel}>
        Cancel
      </button>
      <span className="error" role="alert">
        {error}
      </span>
    </form>
  );
}

interface ConversationState {
  messages: ChatMessageView[];
  more: boolean;
  loading: boolean;
  error: string;
}

type ScrollPlan = { kind: 'bottom' } | { kind: 'keep'; height: number } | null;

function Conversation({ target, showDest }: { target: string; showDest: boolean }) {
  const { onChat } = useLive();
  const [state, setState] = useState<ConversationState>({ messages: [], more: false, loading: true, error: '' });
  const logRef = useRef<HTMLDivElement | null>(null);
  const plan = useRef<ScrollPlan>(null);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 30000);
    return () => clearInterval(t);
  }, []);

  useEffect(() => {
    let cancelled = false;
    fetchPage(target, null)
      .then((page) => {
        if (cancelled) return;
        plan.current = { kind: 'bottom' };
        setState({ messages: page.messages, more: page.more, loading: false, error: '' });
      })
      .catch((e: Error) => !cancelled && setState((s) => ({ ...s, loading: false, error: `cannot load the messages (${e.message})` })));
    return () => {
      cancelled = true;
    };
  }, [target]);

  useEffect(
    () =>
      onChat((t, m) => {
        if (t !== target) return;
        const log = logRef.current;
        if (!log || log.scrollHeight - log.scrollTop - log.clientHeight < 80) plan.current = { kind: 'bottom' };
        setState((s) => ({
          ...s,
          messages: s.messages.some((x) => x.key === m.key) ? s.messages.map((x) => (x.key === m.key ? m : x)) : [...s.messages, m],
        }));
      }),
    [onChat, target],
  );

  const loadOlder = async () => {
    const oldest = state.messages[0]?.key;
    if (state.loading || !state.more || !oldest) return;
    setState((s) => ({ ...s, loading: true }));
    try {
      const page = await fetchPage(target, oldest);
      plan.current = { kind: 'keep', height: logRef.current?.scrollHeight ?? 0 };
      setState((s) => ({
        messages: [...page.messages.filter((m) => !s.messages.some((x) => x.key === m.key)), ...s.messages],
        more: page.more,
        loading: false,
        error: '',
      }));
    } catch (e) {
      setState((s) => ({ ...s, loading: false, error: `cannot load older messages (${(e as Error).message})` }));
    }
  };

  const fillScreen = useEffectEvent(() => {
    const log = logRef.current;
    if (log && log.scrollHeight <= log.clientHeight) void loadOlder();
  });

  useLayoutEffect(() => {
    const log = logRef.current;
    const p = plan.current;
    plan.current = null;
    if (log && p?.kind === 'keep') log.scrollTop += log.scrollHeight - p.height;
    if (log && p?.kind === 'bottom') log.scrollTop = log.scrollHeight;
    fillScreen();
  }, [state]);

  const days: { day: string; items: ChatMessageView[] }[] = [];
  state.messages.forEach((m) => {
    const day = new Date(m.received_at).toDateString();
    const last = days[days.length - 1];
    if (last && last.day === day) last.items.push(m);
    else days.push({ day, items: [m] });
  });

  const note = state.error || (state.loading ? 'loading…' : state.more ? 'scroll up for older messages' : state.messages.length ? 'start of the history' : '');

  return (
    <div
      className="chat-log"
      ref={logRef}
      aria-live="polite"
      onScroll={(e) => {
        if (e.currentTarget.scrollTop < 60) void loadOlder();
      }}
    >
      <div className="chat-older">{note}</div>
      <ul className="chat-messages">
        {!state.loading && !state.messages.length && !state.error && <li className="chat-empty">No messages yet.</li>}
        {days.map((d) => (
          <li key={d.day} className="day-group">
            <span className="day">{d.day}</span>
            <ul>
              {d.items.map((m) => (
                <Message key={m.key} m={m} showDest={showDest} now={now} />
              ))}
            </ul>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function ChatView({ visible }: { visible: boolean }) {
  const live = useLive();
  const { followChat, connection } = live;
  const [serverTabs, setServerTabs] = useState<ChatTab[]>([]);
  const [added, setAdded] = useState<ChatTab[]>(settings().chatTabs);
  const [active, setActive] = useState<string | null>(settings().chatActive);
  const [adding, setAdding] = useState(false);

  const seen = new Set(serverTabs.map((t) => t.target));
  const tabs: Tab[] = [
    ...serverTabs.map((t) => ({ ...t, added: false })),
    ...added.filter((t) => !seen.has(t.target)).map((t) => ({ ...t, added: true })),
  ];
  const activeTab = tabs.find((t) => t.target === active) ?? tabs[0];
  const target = visible ? (activeTab?.target ?? null) : null;

  useEffect(() => {
    if (!visible) return;
    let cancelled = false;
    fetch(api('/chat/tabs'))
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
      .then((b: { tabs: ChatTab[] }) => !cancelled && setServerTabs(b.tabs))
      .catch((e) => console.warn('cannot load the chat tabs', e));
    return () => {
      cancelled = true;
    };
  }, [visible, connection]);

  useEffect(() => {
    followChat(target);
  }, [followChat, target]);

  const open = (t: string) => {
    setActive(t);
    saveSettings({ chatActive: t });
  };
  const close = (t: string) => {
    const next = added.filter((x) => x.target !== t);
    setAdded(next);
    saveSettings({ chatTabs: next });
    if (activeTab?.target === t) open(tabs[0]?.target ?? '');
  };
  const add = (tab: ChatTab) => {
    if (!tabs.some((t) => t.target === tab.target)) {
      const next = [...added, tab];
      setAdded(next);
      saveSettings({ chatTabs: next });
    }
    setAdding(false);
    open(tab.target);
  };

  return (
    <div id="view-chat" className="view" hidden={!visible}>
      <div className="chat-bar">
        <nav className="chat-tabs" role="tablist" aria-label="Conversations">
          {tabs.map((t) => (
            <button key={t.target} className="chat-tab" role="tab" aria-selected={t.target === activeTab?.target} onClick={() => open(t.target)}>
              {t.label}
              {t.target.startsWith('callsign:') && <span className="kind">DM</span>}
              {t.added && (
                <span
                  className="close"
                  title="Close tab"
                  aria-label="Close tab"
                  onClick={(e) => {
                    e.stopPropagation();
                    close(t.target);
                  }}
                >
                  ×
                </span>
              )}
            </button>
          ))}
        </nav>
        <button className="chat-add" title="Add a tab for a channel or a callsign" aria-label="Add a tab" onClick={() => setAdding(true)}>
          +
        </button>
      </div>
      {adding && <AddTab open={tabs.map((t) => t.target)} onAdd={add} onCancel={() => setAdding(false)} />}
      {target && <Conversation key={`${target}#${connection}`} target={target} showDest={!target.startsWith('channel:')} />}
    </div>
  );
}
