import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { setWorkerUrl } from 'maplibre-gl';
import mapLibreWorker from 'maplibre-gl/dist/maplibre-gl-worker.mjs?url';

import 'maplibre-gl/dist/maplibre-gl.css';
import './styles.css';

import { App } from './App';

setWorkerUrl(mapLibreWorker);

const root = document.getElementById('root');
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
