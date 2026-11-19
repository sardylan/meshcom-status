import { useCallback, useState, type MouseEvent, type ReactNode } from 'react';

import { labelStride, niceTicks, num } from '../lib/stats';

export interface Segment {
  name: string;
  value: number;
  color: string;
}

export interface Column {
  label: string;
  title: string;
  segments: Segment[];
}

export interface Series {
  name: string;
  color: string;
}

function useWidth(): [(el: HTMLElement | null) => void, number] {
  const [width, setWidth] = useState(0);
  const ref = useCallback((el: HTMLElement | null) => {
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => entry && setWidth(Math.floor(entry.contentRect.width)));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return [ref, width];
}

function roundedTop(x: number, y: number, w: number, h: number, r: number): string {
  const rr = Math.max(0, Math.min(r, w / 2, h));
  return `M${x},${y + h}V${y + rr}Q${x},${y} ${x + rr},${y}H${x + w - rr}Q${x + w},${y} ${x + w},${y + rr}V${y + h}Z`;
}

const sum = (c: Column) => c.segments.reduce((a, s) => a + s.value, 0);

interface Tip {
  x: number;
  y: number;
  column: Column;
}

function Legend({ series }: { series: Series[] }) {
  return (
    <ul className="chart-legend">
      {series.map((s) => (
        <li key={s.name}>
          <span className="swatch" style={{ background: s.color }} />
          {s.name}
        </li>
      ))}
    </ul>
  );
}

function DataTable({ columns, series, unit }: { columns: Column[]; series: Series[]; unit: string }) {
  const stacked = series.length > 1;
  return (
    <details className="chart-table">
      <summary>Table</summary>
      <div className="scroll">
        <table>
          <thead>
            <tr>
              <th />
              {stacked && series.map((s) => <th key={s.name}>{s.name}</th>)}
              <th>{stacked ? 'Total' : unit}</th>
            </tr>
          </thead>
          <tbody>
            {columns.map((c) => (
              <tr key={c.title}>
                <th scope="row">{c.title}</th>
                {stacked && series.map((s) => <td key={s.name}>{num(c.segments.find((g) => g.name === s.name)?.value ?? 0)}</td>)}
                <td>{num(sum(c))}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </details>
  );
}

interface ColumnsProps {
  columns: Column[];
  series: Series[];
  unit: string;
  height?: number;
  maxLabels?: number;
  labelWidth?: number;
}

const M = { top: 8, right: 4, bottom: 22, left: 40 };

export function Columns({ columns, series, unit, height = 180, maxLabels = 12, labelWidth = 48 }: ColumnsProps) {
  const [ref, width] = useWidth();
  const [tip, setTip] = useState<Tip | null>(null);
  const plotW = Math.max(0, width - M.left - M.right);
  const plotH = height - M.top - M.bottom;
  const ticks = niceTicks(Math.max(0, ...columns.map(sum)));
  const top = ticks[ticks.length - 1] ?? 1;
  const y = (v: number) => M.top + plotH - (v / top) * plotH;
  const band = columns.length ? plotW / columns.length : 0;
  const gap = Math.min(4, band * 0.25);
  const barW = Math.max(1, band - gap);
  const stride = labelStride(columns.length, Math.min(maxLabels, Math.floor(plotW / labelWidth)));

  const hover = (e: MouseEvent<SVGRectElement>, column: Column) => {
    const box = e.currentTarget.ownerSVGElement?.getBoundingClientRect();
    if (box) setTip({ x: e.clientX - box.left, y: e.clientY - box.top, column });
  };

  return (
    <div className="chart" ref={ref}>
      {series.length > 1 && <Legend series={series} />}
      <div className="chart-plot">
        {width > 0 && (
          <svg width={width} height={height} role="img" aria-label={`${columns.length} columns of ${unit}`}>
            {ticks.map((t) => (
              <g key={t} className="grid">
                <line x1={M.left} x2={width - M.right} y1={y(t)} y2={y(t)} />
                <text x={M.left - 6} y={y(t)} dy="0.32em" textAnchor="end">
                  {num(t)}
                </text>
              </g>
            ))}
            {columns.map((c, i) => {
              const x = M.left + i * band + gap / 2;
              let base = 0;
              const parts = c.segments.filter((s) => s.value > 0);
              return (
                <g key={c.title} className={tip && tip.column !== c ? 'col dim' : 'col'}>
                  {parts.map((s, j) => {
                    const y0 = y(base);
                    base += s.value;
                    const y1 = y(base);
                    const inset = (j > 0 ? 1 : 0) + (j < parts.length - 1 ? 1 : 0);
                    const h = Math.max(0, y0 - y1 - inset);
                    const yy = y1 + (j < parts.length - 1 ? 1 : 0);
                    return j === parts.length - 1 ? (
                      <path key={s.name} d={roundedTop(x, yy, barW, h, 4)} fill={s.color} />
                    ) : (
                      <rect key={s.name} x={x} y={yy} width={barW} height={h} fill={s.color} />
                    );
                  })}
                  {i % stride === 0 && (
                    <text className="axis" x={x + barW / 2} y={height - 6} textAnchor="middle">
                      {c.label}
                    </text>
                  )}
                  <rect
                    className="hit"
                    x={M.left + i * band}
                    y={M.top}
                    width={band}
                    height={plotH}
                    onMouseMove={(e) => hover(e, c)}
                    onMouseLeave={() => setTip(null)}
                  />
                </g>
              );
            })}
            <line className="baseline" x1={M.left} x2={width - M.right} y1={y(0)} y2={y(0)} />
          </svg>
        )}
        {tip && (
          <div className="chart-tip" style={{ left: Math.min(tip.x + 12, Math.max(0, width - 180)), top: Math.max(0, tip.y - 12) }}>
            <strong>{tip.column.title}</strong>
            {series.length > 1 &&
              tip.column.segments.map((s) => (
                <div key={s.name}>
                  <span className="swatch" style={{ background: s.color }} /> {s.name} <b>{num(s.value)}</b>
                </div>
              ))}
            <div>
              {series.length > 1 ? 'Total' : unit} <b>{num(sum(tip.column))}</b>
            </div>
          </div>
        )}
      </div>
      <DataTable columns={columns} series={series} unit={unit} />
    </div>
  );
}

export interface BarItem {
  name: ReactNode;
  key: string;
  value: number;
  color?: string;
  note?: string;
}

export function BarList({ items, empty = 'Nothing yet.' }: { items: BarItem[]; empty?: string }) {
  const max = Math.max(0, ...items.map((i) => i.value));
  if (!items.length || max === 0) return <p className="muted">{empty}</p>;
  return (
    <ol className="bar-list">
      {items.map((i) => (
        <li key={i.key}>
          <span className="name">{i.name}</span>
          <span className="track">
            {i.value > 0 && <span className="bar" style={{ width: `${(100 * i.value) / max}%`, background: i.color ?? 'var(--path)' }} />}
          </span>
          <span className="value">
            {num(i.value)}
            {i.note && <span className="note"> {i.note}</span>}
          </span>
        </li>
      ))}
    </ol>
  );
}
