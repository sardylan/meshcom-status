import { useEffect, useState } from 'react';

import { ChatView } from './components/ChatView';
import { MapView } from './components/MapView';
import { StatsView } from './components/StatsView';
import { CONFIG } from './lib/config';
import { LiveProvider, useLive, type Status } from './lib/live';
import { saveSettings, settings, type ViewName } from './lib/settings';

const VIEW_LABELS: Record<ViewName, string> = { map: 'Map', chat: 'Chat', stats: 'Stats' };

const STATUS_TEXT: Record<Status, string> = { connecting: 'connecting', open: 'live', closed: 'offline' };

function StatusBadge() {
  const { status } = useLive();
  return (
    <span className={`status status-${status}`} title="WebSocket connection">
      {STATUS_TEXT[status]}
    </span>
  );
}

function useOwnCall(): string | null {
  const { nodes } = useLive();
  return Object.values(nodes).find((n) => n.own)?.call ?? null;
}

function Shell() {
  const [view, setView] = useState<ViewName>(settings().view);
  const own = useOwnCall();
  useEffect(() => {
    document.title = own ? `${own} · MeshCom Status` : 'MeshCom Status';
  }, [own]);
  const choose = (v: ViewName) => {
    setView(v);
    saveSettings({ view: v });
  };
  return (
    <div id="shell">
      <header className="topbar">
        <h1>
          <span className="app-name">MeshCom Status</span>
          {own && (
            <span className="own-call" title="Callsign of the station this server listens to">
              {own}
            </span>
          )}
        </h1>
        <nav className="views" role="tablist" aria-label="View">
          {(['map', 'chat', 'stats'] as const).map((v) => (
            <button key={v} role="tab" aria-selected={view === v} onClick={() => choose(v)}>
              {VIEW_LABELS[v]}
            </button>
          ))}
        </nav>
        <StatusBadge />
      </header>
      <main>
        <MapView visible={view === 'map'} />
        <ChatView visible={view === 'chat'} />
        <StatsView visible={view === 'stats'} />
      </main>
      <footer className="app-footer">
        <span>meshcom-status {CONFIG.version ? `v${CONFIG.version}` : 'unknown version'}</span>
        {CONFIG.repository && (
          <a href={CONFIG.repository} target="_blank" rel="noopener noreferrer">
            GitHub
          </a>
        )}
      </footer>
    </div>
  );
}

export function App() {
  return (
    <LiveProvider>
      <Shell />
    </LiveProvider>
  );
}
