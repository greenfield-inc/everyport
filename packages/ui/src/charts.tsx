// Hand-drawn SVG charts. Time runs left to right over the last 10 minutes,
// ending at the snapshot's `taken_at`.
import type { Sample } from "@ppm/protocol";
import type { PointerEvent } from "react";
import { totalParts } from "./format.ts";

const WINDOW = 10 * 60 * 1000;

type SparkProps = { history: Sample[]; warn: boolean };

/** The memory trend in a list row. Flat and dashed when there is no history. */
export function Sparkline({ history, warn }: SparkProps) {
  const width = 44;
  const height = 18;
  if (history.length < 2) {
    return (
      <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} className="ppm:shrink-0 ppm:text-fg/50" aria-hidden>
        <path d="M0 12 L44 12" fill="none" stroke="currentColor" strokeWidth="1.25" strokeLinecap="round" strokeDasharray="2 3" />
      </svg>
    );
  }
  const values = history.map((sample) => sample.memory);
  const low = Math.min(...values);
  const high = Math.max(...values);
  const range = Math.max(high - low, high * 0.05, 1);
  const d = values
    .map((value, index) => {
      const x = (width * index) / (values.length - 1);
      const y = height * (1 - (value - low) / range) * 0.8 + height * 0.1;
      return `${index ? "L" : "M"}${x.toFixed(1)} ${y.toFixed(1)}`;
    })
    .join(" ");
  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className={`ppm:shrink-0 ${warn ? "ppm:text-warn" : "ppm:text-fg/70"}`}
      aria-hidden
    >
      <path d={d} fill="none" stroke="currentColor" strokeWidth={warn ? 1.5 : 1.25} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

const PLOT_WIDTH = 328;

type PlotProps = {
  history: Sample[];
  now: number;
  /** Hovered time in ms, shared by both charts so one scrubs the other. */
  hover: number | null;
  onHover: (time: number | null) => void;
};

const xOf = (at: number, now: number) => ((at - (now - WINDOW)) / WINDOW) * PLOT_WIDTH;

/** The sample closest to `time`. */
export function sampleNear(history: Sample[], time: number | null): Sample | null {
  if (time === null || !history.length) return null;
  return history.reduce((best, sample) => (Math.abs(sample.at - time) < Math.abs(best.at - time) ? sample : best));
}

function hoverHandlers(now: number, onHover: (time: number | null) => void) {
  return {
    onPointerMove: (event: PointerEvent<SVGSVGElement>) => {
      const bounds = event.currentTarget.getBoundingClientRect();
      const fraction = Math.min(1, Math.max(0, (event.clientX - bounds.left) / bounds.width));
      onHover(now - WINDOW + fraction * WINDOW);
    },
    onPointerLeave: () => onHover(null),
  };
}

/** Memory over 10 minutes, with the alert threshold as a dashed amber line. */
export function MemoryChart({ history, now, hover, onHover, threshold }: PlotProps & { threshold: number }) {
  const height = 76;
  const top = Math.max(threshold, ...history.map((sample) => sample.memory * 1.1));
  const yOf = (bytes: number) => height - (bytes / top) * (height - 1);
  const hovered = sampleNear(history, hover);
  const last = history[history.length - 1];
  const dot = hovered ?? last;
  const line = history.map((sample, index) => `${index ? "L" : "M"}${xOf(sample.at, now).toFixed(1)} ${yOf(sample.memory).toFixed(1)}`).join(" ");
  const thresholdY = yOf(threshold);
  return (
    <div className="ppm:flex ppm:gap-2">
      <div className="ppm:relative ppm:w-8 ppm:shrink-0 ppm:font-mono ppm:text-11 ppm:text-fg3" style={{ height }} aria-hidden>
        <span className="ppm:absolute ppm:right-0 ppm:text-warn/85" style={{ top: thresholdY - 7 }}>
          {totalParts(threshold).join(" ")}
        </span>
        <span className="ppm:absolute ppm:right-0" style={{ top: height - 8 }}>
          0
        </span>
      </div>
      <svg
        width={PLOT_WIDTH}
        height={height}
        viewBox={`0 0 ${PLOT_WIDTH} ${height}`}
        className="ppm:shrink-0 ppm:overflow-visible"
        role="img"
        aria-label="Memory over the last 10 minutes"
        {...hoverHandlers(now, onHover)}
      >
        <path d={`M0.5 0 V${height} M0 ${height - 0.5} H${PLOT_WIDTH}`} fill="none" stroke="currentColor" className="ppm:text-fg/14" />
        <path d={`M0 ${thresholdY} H${PLOT_WIDTH}`} fill="none" stroke="currentColor" strokeDasharray="3 3" className="ppm:text-warn/55" />
        <path d={line} fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" className="ppm:text-fg/90" />
        {hovered && <path d={`M${xOf(hovered.at, now)} 0 V${height}`} stroke="currentColor" className="ppm:text-fg/25" />}
        {dot && <circle cx={Math.min(xOf(dot.at, now), PLOT_WIDTH - 1)} cy={yOf(dot.memory)} r="3" className="ppm:fill-fg" />}
      </svg>
    </div>
  );
}

/** CPU over 10 minutes, one bar per sample. The current (or hovered) bar is bright. */
export function CpuChart({ history, now, hover, onHover }: PlotProps) {
  const height = 28;
  const hovered = sampleNear(history, hover);
  const highlight = hovered ?? history[history.length - 1];
  return (
    <div className="ppm:flex ppm:gap-2">
      <div className="ppm:flex ppm:w-8 ppm:shrink-0 ppm:flex-col ppm:items-end ppm:justify-between ppm:font-mono ppm:text-11 ppm:text-fg3" style={{ height }} aria-hidden>
        <span className="ppm:-mt-1.5">100%</span>
        <span className="ppm:-mb-1.5">0</span>
      </div>
      <svg
        width={PLOT_WIDTH}
        height={height}
        viewBox={`0 0 ${PLOT_WIDTH} ${height}`}
        className="ppm:shrink-0"
        role="img"
        aria-label="CPU over the last 10 minutes"
        {...hoverHandlers(now, onHover)}
      >
        <path d={`M0.5 0 V${height}`} fill="none" stroke="currentColor" className="ppm:text-fg/14" />
        {history.map((sample) => {
          const barHeight = Math.max(0.9, (Math.min(sample.cpu_percent, 100) / 100) * height);
          return (
            <rect
              key={sample.at}
              x={Math.min(Math.max(xOf(sample.at, now) - 2, 0.5), PLOT_WIDTH - 5)}
              y={height - barHeight}
              width="4"
              height={barHeight}
              rx="1"
              className={sample === highlight ? "ppm:fill-fg/80" : "ppm:fill-fg/32"}
            />
          );
        })}
        {hovered && <path d={`M${xOf(hovered.at, now)} 0 V${height}`} stroke="currentColor" className="ppm:text-fg/25" />}
      </svg>
    </div>
  );
}
