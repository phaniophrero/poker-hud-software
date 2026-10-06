// MDA tab - population-wide preflop/postflop tendency charts, modeled on
// the Drivetracker MDA screenshots the project owner shared. Built one tab at a
// time, by explicit request: "Defense vs RFI by Position", then
// "Positional EV Leakage", then "Cold-Call Frequency Imbalance", then
// "Preflop Aggression Profitability", then "Positional EV Realization",
// "Preflop Archetype Distribution", and "Preflop EV Stability" - all seven
// Preflop sub-tabs. Flop, Turn, and River now use the same real-history
// pipeline, with missing samples rendered explicitly rather than invented.
//
// Every chart reads `get_mda_preflop_defense_vs_rfi` /
// `get_mda_positional_ev_leakage`, computed in `src-tauri/src/mda.rs`
// straight from imported hand history - there is no separate "population"
// dataset. `sample_size` on every bar is why a bar with no eligible hands
// renders as "--" rather than a confident-looking 0%, the same "0.0 vs --"
// honesty rule the Gauges panel already follows (see `TOOLTIP.gauges` in
// App.tsx).
//
// Colors: Drivetracker's own reference screenshots mix red/green/blue/orange/
// teal/purple somewhat arbitrarily per panel. Charts here instead follow
// this app's color-formula rule (sequential magnitude -> one hue; diverging
// sign -> two hues + a zero baseline; identity/category -> a fixed-order
// categorical set) - validated with the dataviz skill's palette checker
// against this app's actual dark panel surface (`#151a31`):
//   - `#4779e8` (existing --dh-blue-bright) vs `#d94b58` (existing --dh-red)
//     for a diverging pair - the app's own established green/red
//     (`.dh-money-positive`/`.dh-money-negative`) fails the deuteranopia
//     check outright (ΔE 1.3, far under the floor) for a bar fill with no
//     adjacent numeral carrying the same information.
//   - `#4779e8` vs `#d95926` for a 2-series identity pair (Actual vs.
//     All-In EV; VPIP vs. PFR) - also passes clean (ΔE 27.9).
//   - `#d94b58` / `#4779e8` / `#d95926` (fold/call/raise) as a 3-slot
//     categorical set for the stacked Fold-Call-Raise bar - also clean.
//   - the dataviz skill's own 8-slot validated categorical theme, first six
//     slots in fixed order, for the 6-position EV-tracking line chart:
//     blue/orange/aqua/yellow/magenta/green - lines are validated on the
//     *adjacent* pairlist (not the stricter all-pairs one scatter/small-
//     multiples need), which six simultaneous lines with direct hover and
//     a legend satisfies.
//   - the cold-call-frequency heatmap uses a single sequential hue (blue,
//     opacity-scaled by magnitude) for real cells, and a plain muted "--"
//     for insufficient-data ones - never a red "zero" the way Drivetracker's
//     own screenshot renders an empty heatmap, since that would read as a
//     real (bad) 0% instead of "no data yet" (see this section's own note
//     on `classify_archetype` below for why "no data yet" is the common
//     case at this app's actual sample size).
// None of this is a hue swap done for taste - every pairing here was run
// through `validate_palette.js`, not eyeballed.

import { createContext, useContext, useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChartGrid, PanelHeader, chartLabelIndexes, formatMoney, moneyAxis } from "./App";
import type {
  MdaAggressionFactor,
  MdaArchetypeFlopEdge,
  MdaBar,
  MdaColdCallFrequencyImbalance,
  MdaEvByStreet,
  MdaArchetypeTableRow,
  MdaEvSeries,
  MdaFlopAggressionEfficiency,
  MdaFlopCbetFrequency,
  MdaFlopOopResistance,
  MdaFlopOverCalling,
  MdaFlopToTurnContinuity,
  MdaHeatmapRow,
  MdaPositionalEvLeakage,
  MdaPositionalEvRealization,
  MdaPostFlopEvContinuity,
  MdaPreflopAggressionProfitability,
  MdaPreflopArchetypeDistribution,
  MdaPreflopDefenseVsRfi,
  MdaPreflopEvStability,
  MdaRiverBluffImbalance,
  MdaRiverEvByArchetype,
  MdaRiverOverbetResponse,
  MdaRiverSizingPolarization,
  MdaRiverSeriesPair,
  MdaRiverThinValueDeficit,
  MdaRiverWeakShowdownIndex,
  MdaStatRow,
  MdaTurnAggressionRoi,
  MdaTurnBarrelDefense,
  MdaTurnBluffValueBalance,
  MdaTurnDefenseByArchetype,
  MdaTurnLeverageDominance,
  MdaTurnOopSurrenderRate,
} from "./types";

// Current MDA chart palette from the reference graphs.
const CHART_RED = "#D74F57";
const CHART_BLUE = "#4072EE";
const CHART_ORANGE = "#B46E2D";
const CHART_GREEN = "#5EF472";
const CHART_MAGENTA = "#D44BAE";
const CHART_MUTED_RED = "#9F555C";
const CHART_POINT_LABEL = "#D7DEED";
const chartAlpha = (hex: string, alpha: number) => `${hex}${Math.round(alpha * 255).toString(16).padStart(2, "0")}`;
// Taller than the original 220px pass - a real report ("your charts are
// too small in height") compared this against the reference screenshots,
// where every MDA chart fills a panel with real vertical presence.
const EV_CHART_HEIGHT = 300;
const VPIP_CHART_HEIGHT = 300;
const ARCHETYPE_CHART_HEIGHT = 340;
const POSITION_ORDER = ["UTG", "MP", "CO", "BTN", "SB", "BB"] as const;
const POSITION_COLORS: Record<string, string> = {
  UTG: CHART_BLUE,
  MP: CHART_ORANGE,
  CO: CHART_GREEN,
  BTN: CHART_MAGENTA,
  SB: CHART_RED,
  BB: chartAlpha(CHART_GREEN, 0.72),
};

const MAIN_TABS = ["Preflop", "Flop", "Turn", "River"] as const;
type MainTab = (typeof MAIN_TABS)[number];

const GAME_TYPE_OPTIONS = [
  "NL Cash 6-max",
  "NL Cash FR",
  "NL Zoom 6-max",
  "NL Zoom FR",
  "PLO4 Cash 6-max",
  "PLO4 Cash FR",
  "PLO5 Cash 6-max",
  "PLO5 Cash FR",
  "PLO6 Cash 6-max",
  "PLO6 Cash FR",
  "NL MTT",
  "NL S&G",
  "PLO4 MTT",
  "PLO4 S&G",
  "PLO5 MTT",
  "PLO5 S&G",
  "PLO6 MTT",
  "PLO6 S&G",
] as const;
type GameType = (typeof GAME_TYPE_OPTIONS)[number];
const MdaGameTypeContext = createContext<GameType>("NL Cash 6-max");
const useMdaGameType = () => useContext(MdaGameTypeContext);
const mdaInvokeArgs = (gameType: GameType) => ({ gameType });

const PREFLOP_SUB_TABS = [
  "Defense vs RFI by Position",
  "Positional EV Leakage",
  "Cold-Call Frequency Imbalance",
  "Preflop Aggression Profitability",
  "Positional EV Realization",
  "Preflop Archetype Distribution",
  "Preflop EV Stability",
] as const;

const FLOP_SUB_TABS = [
  "Flop C-Bet Frequency",
  "Flop-to-Turn Aggression Continuity",
  "Archetype Flop Edge",
  "Flop OOP Resistance",
  "Flop Aggression Efficiency",
  "Flop Over-calling",
  "Post-Flop EV Continuity",
] as const;

const TURN_SUB_TABS = [
  "Turn Barrel Defense",
  "Turn Defense by Archetype",
  "Turn Aggression ROI",
  "Turn OOP Surrender Rate",
  "Bluff-to-Value Balance",
  "Turn Leverage Dominance",
] as const;

const RIVER_SUB_TABS = [
  "River Overbet Response",
  "River Bluff Imbalance",
  "River EV by Archetype",
  "Weak Showdown Index",
  "Thin Value Deficit",
  "River Sizing Polarization",
] as const;

type BarUnit = "percent" | "money" | "bb100";

function formatBarValue(value: number | null, unit: BarUnit): string {
  if (value === null) return "0.00";
  if (unit === "percent") return `${value.toFixed(1)}%`;
  if (unit === "bb100") return `${value >= 0 ? "+" : ""}${value.toFixed(1)}`;
  return formatMoney(value);
}

function barTitle(bar: MdaBar): string {
  return bar.sample_size > 0
    ? `${bar.sample_size} ${bar.sample_size === 1 ? "mână eligibilă" : "mâini eligibile"}`
    : "Încă nicio mână eligibilă pentru acest scenariu";
}

// A smoothed SVG path through a point sequence (Catmull-Rom converted to
// cubic Béziers, the standard uniform-tension conversion) instead of a
// plain `<polyline>` - the reference's own line charts round off every
// vertex instead of meeting at a sharp angle, which a straight-segment
// polyline can never produce regardless of point count.
function smoothLinePath(points: { x: number; y: number }[]): string {
  if (points.length === 0) return "";
  if (points.length < 3) {
    return points.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`).join(" ");
  }
  let d = `M ${points[0].x} ${points[0].y}`;
  for (let i = 0; i < points.length - 1; i++) {
    const p0 = points[i - 1] ?? points[i];
    const p1 = points[i];
    const p2 = points[i + 1];
    const p3 = points[i + 2] ?? p2;
    const c1x = p1.x + (p2.x - p0.x) / 6;
    const c1y = p1.y + (p2.y - p0.y) / 6;
    const c2x = p2.x - (p3.x - p1.x) / 6;
    const c2y = p2.y - (p3.y - p1.y) / 6;
    d += ` C ${c1x} ${c1y}, ${c2x} ${c2y}, ${p2.x} ${p2.y}`;
  }
  return d;
}

function LegendSwatch({ color, label }: { color: string; label: string }) {
  return (
    <span className="dh-mda-legend-item">
      <i style={{ background: color }} />
      {label}
    </span>
  );
}

function formatAxisTick(value: number, unit: BarUnit): string {
  if (unit === "money") return formatMoney(value);
  const abs = Math.abs(value);
  const compact = abs >= 1000 ? `${(abs / 1000).toFixed(abs >= 10000 ? 0 : 1)}K` : abs.toFixed(abs % 1 === 0 ? 0 : 1);
  return value < 0 ? `-${compact}` : compact;
}

function useSvgChartSize(fallbackWidth: number, fallbackHeight: number, minWidth = 240, minHeight = 140) {
  const chartRef = useRef<SVGSVGElement>(null);
  const [size, setSize] = useState({ width: fallbackWidth, height: fallbackHeight });

  useEffect(() => {
    const el = chartRef.current;
    if (!el) return;
    const update = () => {
      const bounds = el.getBoundingClientRect();
      const next = {
        width: Math.max(minWidth, Math.round(bounds.width || fallbackWidth)),
        height: Math.max(minHeight, Math.round(bounds.height || fallbackHeight)),
      };
      setSize((current) => (current.width === next.width && current.height === next.height ? current : next));
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, [fallbackHeight, fallbackWidth, minHeight, minWidth]);

  return [chartRef, size] as const;
}

// A real horizontal bar chart - axis line, tick marks, numeric scale, and
// vertical gridlines - not a CSS progress-bar list. This is what Drivetracker's
// own reference screenshots actually look like for every "Positional Win
// Rates"/"Fold to Steal"/"Call Open"/etc. panel, and a plain width-percent
// bar (this file's first pass) reads as a visibly different chart type
// next to them even with the same numbers. `moneyAxis` (already used by
// the line charts below) supplies the "nice round tick" auto-scaling, so a
// sequential 0-100% chart still gets headroom above 100 the same way
// Drivetracker's own axes do rather than hard-capping at the logical bound.
//
// `secondary` turns this into the two-series grouped variant ("Actual
// BB/100" vs. "All-In EV BB/100") - one extra thin bar per row, same
// shared axis, instead of a second chart component to keep in sync.
function AxisBarChart({
  rows,
  unit,
  mode,
  secondary,
  primaryLabel,
  secondaryLabel,
  color = CHART_BLUE,
  colorFor,
}: {
  rows: MdaBar[];
  unit: BarUnit;
  mode: "sequential" | "diverging";
  secondary?: MdaBar[];
  primaryLabel?: string;
  secondaryLabel?: string;
  /** Single-series (non-grouped) bar color - grouped charts always use
   * CHART_BLUE/CHART_ORANGE for primary/secondary regardless of this. */
  color?: string;
  /** Per-bar color override (e.g. "Positional EV Realization"'s Elite/
   * Strong/Weak/Leaking tiers, or a traffic-light frequency scale) - a
   * display-only threshold layered on the real value, takes priority over
   * `color` when set. Ignored on a grouped chart, same as `color`. */
  colorFor?: (value: number | null) => string;
}) {
  const grouped = secondary !== undefined;
  // The fallback keeps first paint stable; ResizeObserver immediately
  // replaces it with the real card height so the plot fills its panel.
  const fallbackHeight = rows.length <= 3 ? 210 : rows.length <= 5 ? 270 : 340;
  const [chartRef, chartSize] = useSvgChartSize(560, fallbackHeight, 260, 120);
  const width = chartSize.width;
  const height = chartSize.height;
  const top = 16;
  const bottom = 28;
  // Wide enough for the longest real category label ("SB Fold to BTN
  // Steal%") to sit fully left of the plot area.
  const left = 136;
  const right = 32;
  const plotHeight = height - top - bottom;
  const plotWidth = Math.max(40, width - left - right);
  const desiredRowHeight = grouped ? 48 : 56;
  const rowHeight = rows.length > 0 ? Math.min(desiredRowHeight, plotHeight / rows.length) : desiredRowHeight;
  const rowsHeight = rowHeight * rows.length;
  const rowsTop = top + Math.max(0, (plotHeight - rowsHeight) / 2);
  const barHeight = grouped ? Math.max(12, Math.min(16, rowHeight * 0.34)) : Math.max(18, Math.min(28, rowHeight * 0.5));
  const barRadius = Math.min(4, barHeight / 2);

  const realValues = rows
    .map((row) => row.value)
    .concat((secondary ?? []).map((row) => row.value))
    .filter((v): v is number => v !== null);
  const axis = mode === "diverging" ? moneyAxis(Math.min(0, ...realValues), Math.max(0, ...realValues)) : moneyAxis(0, Math.max(1, ...realValues));
  const xFor = (value: number) => left + ((value - axis.min) / axis.span) * plotWidth;
  const zeroX = xFor(0);
  const barStart = mode === "diverging" ? zeroX : left;
  // A rough monospace-ish width estimate (no DOM text measurement
  // available at render time) - just enough to keep a value label's own
  // estimated footprint from ever exiting the canvas, so it can still hug
  // the bar's own tip (matching the reference) in the normal case, and
  // only pull back toward the bar when the bar itself is close enough to
  // an axis edge that hugging it wouldn't fit.
  const estimateLabelWidth = (text: string) => text.length * 7 + 4;

  return (
    <div className="dh-mda-axis-chart">
      {grouped && (
        <div className="dh-mda-legend">
          <LegendSwatch color={CHART_BLUE} label={primaryLabel ?? ""} />
          <LegendSwatch color={CHART_ORANGE} label={secondaryLabel ?? ""} />
        </div>
      )}
      <svg ref={chartRef} viewBox={`0 0 ${width} ${height}`} role="img" aria-label={primaryLabel ?? "Bar chart"}>
        {axis.ticks.map((tick) => (
          <line
            key={tick}
            x1={xFor(tick)}
            x2={xFor(tick)}
            y1={top - 6}
            y2={height - bottom}
            stroke="rgba(135,153,190,0.22)"
            strokeWidth="1"
            strokeDasharray="3,3"
          />
        ))}
        {mode === "diverging" && <line x1={zeroX} x2={zeroX} y1={top - 6} y2={height - bottom} stroke="rgba(152,166,204,0.45)" strokeWidth="1.2" />}
        {rows.map((row, index) => {
          const rowTop = rowsTop + index * rowHeight;
          const rowMid = rowTop + rowHeight / 2;
          const secondaryBar = grouped ? secondary![index] : undefined;
          const series = grouped
            ? [
                { bar: row, color: CHART_BLUE, y: rowTop + rowHeight * 0.24 },
                { bar: secondaryBar, color: CHART_ORANGE, y: rowTop + rowHeight * 0.6 },
              ]
            : [
                {
                  bar: row,
                  // Diverging keeps its blue-positive/red-negative meaning
                  // regardless of `color` - only a plain sequential (0-100%)
                  // chart takes the per-panel color. `colorFor` (a per-bar
                  // tier/threshold) overrides both when supplied.
                  color: colorFor ? colorFor(row.value) : mode === "diverging" ? ((row.value ?? 0) >= 0 ? CHART_BLUE : CHART_RED) : color,
                  y: rowMid - barHeight / 2,
                },
              ];
          const label = grouped
            ? `${formatBarValue(row.value, unit)} / ${formatBarValue(secondaryBar?.value ?? null, unit)}`
            : formatBarValue(row.value, unit);
          const tooltip = grouped ? `${primaryLabel}: ${barTitle(row)} · ${secondaryLabel}: ${barTitle(secondaryBar!)}` : barTitle(row);

          // Hug whichever bar reaches farthest from the axis start, on the
          // correct side for its sign - the reference always prints the
          // value right at the bar's own tip, never in a detached column.
          // Clamped by the label's own estimated width so it can never
          // exit the canvas even for an extreme outlier value close to an
          // axis edge (the bug the very first, unclamped version of this
          // had) - in the ordinary case the clamp is a no-op and the label
          // sits exactly at the tip.
          const candidates = [row.value, secondaryBar?.value ?? null].filter((v): v is number => v !== null);
          const extreme = candidates.length > 0 ? candidates.reduce((a, b) => (Math.abs(b) > Math.abs(a) ? b : a), candidates[0]) : 0;
          const extremeIsNegative = mode === "diverging" && extreme < 0;
          const naturalLabelX = extremeIsNegative ? xFor(extreme) - 6 : xFor(extreme) + 6;
          const estWidth = estimateLabelWidth(label);
          // The floor for a negative-direction label is `left`, not 0 - a
          // real report showed "BTN-$17,561" fused together because the
          // earlier clamp only kept the label on-canvas, not clear of the
          // category-label column to its left (a separate collision from
          // the one that clamp was built for). Anchored "end", the text
          // renders to the *left* of this x, so keeping `labelX - estWidth
          // >= left` means `labelX >= left + estWidth`.
          const labelX = extremeIsNegative ? Math.max(naturalLabelX, left + estWidth) : Math.min(naturalLabelX, width - estWidth - 4);

          return (
            <g key={row.label}>
              <title>{tooltip}</title>
              <text x={left - 10} y={rowMid + 4} textAnchor="end" fill="#8998b8" fontSize="12.5">
                {row.label}
              </text>
              {series.map(({ bar, color, y }, seriesIndex) => {
                if (bar?.value == null) return null;
                const x = xFor(bar.value);
                const barX = Math.min(x, barStart);
                const barWidth = Math.max(1, Math.abs(x - barStart));
                return <rect key={seriesIndex} x={barX} y={y} width={barWidth} height={barHeight} rx={barRadius} fill={color} />;
              })}
              <text x={labelX} y={rowMid + 4} textAnchor={extremeIsNegative ? "end" : "start"} fill="#f4f6ff" fontSize={grouped ? 12 : 14} fontWeight={700}>
                {label}
              </text>
            </g>
          );
        })}
        <line x1={left} x2={left + plotWidth} y1={height - bottom} y2={height - bottom} stroke="rgba(135,153,190,0.3)" strokeWidth="1" />
        {axis.ticks.map((tick) => (
          <text key={tick} x={xFor(tick)} y={height - bottom + 16} textAnchor="middle" fill="#71809f" fontSize="12">
            {formatAxisTick(tick, unit)}
          </text>
        ))}
      </svg>
    </div>
  );
}

function GroupedVerticalBarChart({
  rows,
  secondary,
  primaryLabel,
  secondaryLabel,
  unit,
}: {
  rows: MdaBar[];
  secondary: MdaBar[];
  primaryLabel: string;
  secondaryLabel: string;
  unit: BarUnit;
}) {
  const [chartRef, chartSize] = useSvgChartSize(560, 560, 320, 160);
  const width = chartSize.width;
  const height = chartSize.height;

  const ordered = POSITION_ORDER.map((position) => ({
    position,
    primary: rows.find((row) => row.label === position) ?? { label: position, value: null, sample_size: 0 },
    secondary: secondary.find((row) => row.label === position) ?? { label: position, value: null, sample_size: 0 },
  }));
  const values = ordered
    .flatMap((row) => [row.primary.value, row.secondary.value])
    .filter((value): value is number => value !== null);
  const axis = moneyAxis(Math.min(0, ...values), Math.max(0, ...values));
  const left = 50;
  const right = 12;
  const top = 24;
  const bottom = 34;
  const plotWidth = width - left - right;
  const plotHeight = height - top - bottom;
  const xFor = (index: number) => left + (index + 0.5) * (plotWidth / ordered.length);
  const yFor = (value: number) => top + ((axis.max - value) / axis.span) * plotHeight;
  const zeroY = yFor(0);
  const slotWidth = plotWidth / ordered.length;
  const barWidth = Math.max(10, Math.min(28, slotWidth * 0.22));

  return (
    <div className="dh-mda-vgroup-chart">
      <div className="dh-mda-legend dh-mda-legend-right">
        <LegendSwatch color={CHART_BLUE} label={primaryLabel} />
        <LegendSwatch color={CHART_ORANGE} label={secondaryLabel} />
      </div>
      <svg ref={chartRef} viewBox={`0 0 ${width} ${height}`} role="img" aria-label={`${primaryLabel} vs ${secondaryLabel}`}>
        {axis.ticks.map((tick) => (
          <line
            key={tick}
            x1={left}
            x2={width - right}
            y1={yFor(tick)}
            y2={yFor(tick)}
            stroke="rgba(135,153,190,0.22)"
            strokeWidth="1"
            strokeDasharray="3,3"
          />
        ))}
        <line x1={left} x2={width - right} y1={zeroY} y2={zeroY} stroke="rgba(152,166,204,0.45)" strokeWidth="1.2" />
        <line x1={left} x2={left} y1={top} y2={height - bottom} stroke="rgba(135,153,190,0.34)" strokeWidth="1" />
        {axis.ticks.map((tick) => (
          <text key={tick} x={left - 8} y={yFor(tick) + 4} textAnchor="end" fill="#71809f" fontSize="11">
            {formatAxisTick(tick, unit)}
          </text>
        ))}
        {ordered.map((row, index) => {
          const groupX = xFor(index);
          const series = [
            { bar: row.primary, color: CHART_BLUE, x: groupX - barWidth - 2 },
            { bar: row.secondary, color: CHART_ORANGE, x: groupX + 2 },
          ];
          return (
            <g key={row.position}>
              {series.map(({ bar, color, x }, seriesIndex) => {
                if (bar.value == null) return null;
                const y = yFor(bar.value);
                const rectY = Math.min(y, zeroY);
                const rectHeight = Math.max(1, Math.abs(y - zeroY));
                const labelY = bar.value >= 0 ? rectY + Math.min(rectHeight - 5, 42) : rectY + rectHeight - 6;
                return (
                  <g key={seriesIndex}>
                    <rect x={x} y={rectY} width={barWidth} height={rectHeight} rx={3} fill={color}>
                      <title>{`${bar.label}: ${formatBarValue(bar.value, unit)} (${barTitle(bar)})`}</title>
                    </rect>
                    <text
                      x={x + barWidth / 2}
                      y={labelY}
                      textAnchor="middle"
                      fill="#ffffff"
                      fontSize="11"
                      fontWeight={700}
                      transform={`rotate(-90 ${x + barWidth / 2} ${labelY})`}
                    >
                      {formatBarValue(bar.value, unit)}
                    </text>
                  </g>
                );
              })}
              <text x={groupX} y={height - bottom + 20} textAnchor="middle" fill="#71809f" fontSize="12">
                {row.position}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

// Multi-line cumulative chart, one line per position - reuses the same
// `ChartGrid`/`moneyAxis`/`chartLabelIndexes` primitives the "Money Won"
// panel's own SVG line chart already established, instead of a new
// charting approach for this one panel.
function EvTrackingChart({ series }: { series: MdaEvSeries[] }) {
  const [chartRef, chartSize] = useSvgChartSize(640, EV_CHART_HEIGHT, 320, 160);
  const width = chartSize.width;
  const height = chartSize.height;

  const ordered = POSITION_ORDER.map((position) => series.find((s) => s.position === position)).filter(
    (s): s is MdaEvSeries => !!s && s.cumulative.length > 0
  );
  const pointCount = ordered[0]?.cumulative.length ?? 0;
  const left = 46;
  const right = 8;
  const top = 10;
  const bottom = 18;
  const plotWidth = width - left - right;
  const plotHeight = height - top - bottom;

  return (
    <div className="dh-mda-svg-chart">
      <div className="dh-mda-legend">
        {POSITION_ORDER.map((position) => (
          <LegendSwatch key={position} color={POSITION_COLORS[position]} label={position} />
        ))}
      </div>
      <svg ref={chartRef} viewBox={`0 0 ${width} ${height}`} role="img" aria-label="EV Tracking by Position (Cumulative)">
        {pointCount === 0 ? (
          <>
            <ChartGrid width={width} height={height} />
            <text x={width / 2} y={height / 2 + 5} textAnchor="middle" fill="#f4f6ff" fontSize="13">
              No data to plot
            </text>
          </>
        ) : (
          (() => {
            const allValues = ordered.flatMap((s) => s.cumulative);
            const axis = moneyAxis(Math.min(0, ...allValues), Math.max(0, ...allValues));
            const xFor = (index: number) => left + (pointCount === 1 ? plotWidth : (index / (pointCount - 1)) * plotWidth);
            const yFor = (value: number) => top + ((axis.max - value) / axis.span) * plotHeight;
            const zeroY = yFor(0);
            const labelIndexes = chartLabelIndexes(pointCount);
            return (
              <>
                <ChartGrid width={width} height={height} left={left} right={right} top={top} bottom={bottom} yTicks={axis.ticks} yFor={yFor} />
                <line x1={left} y1={zeroY} x2={width - right} y2={zeroY} stroke="rgba(152,166,204,0.38)" strokeWidth="1" />
                {ordered.map((s) => (
                  <path
                    key={s.position}
                    d={smoothLinePath(s.cumulative.map((value, index) => ({ x: xFor(index), y: yFor(value) })))}
                    fill="none"
                    stroke={POSITION_COLORS[s.position]}
                    strokeWidth={2}
                    strokeLinejoin="round"
                    strokeLinecap="round"
                  />
                ))}
                {axis.ticks.map((tick) => (
                  <text key={tick} x={left - 8} y={yFor(tick) + 4} textAnchor="end" fill="#71809f" fontSize="11">
                    {formatMoney(tick)}
                  </text>
                ))}
                {labelIndexes.map((index) => (
                  <text
                    key={index}
                    x={xFor(index)}
                    y={height - 5}
                    textAnchor={index === 0 ? "start" : index === pointCount - 1 ? "end" : "middle"}
                    fill="#71809f"
                    fontSize="11"
                  >
                    {index + 1}
                  </text>
                ))}
              </>
            );
          })()
        )}
      </svg>
    </div>
  );
}

// Combo chart: PFR as bars, VPIP as an overlaid line, both against a fixed
// 0-100% scale (unlike the money charts above, this axis never needs to
// adapt to the data - percentages are always 0-100).
function VpipPfrChart({ vpip, pfr }: { vpip: MdaBar[]; pfr: MdaBar[] }) {
  const [chartRef, chartSize] = useSvgChartSize(640, VPIP_CHART_HEIGHT, 320, 160);
  const width = chartSize.width;
  const height = chartSize.height;
  const left = 34;
  const right = 8;
  const top = 12;
  const bottom = 20;
  const plotWidth = width - left - right;
  const plotHeight = height - top - bottom;
  const n = POSITION_ORDER.length;
  const xFor = (index: number) => left + (index + 0.5) * (plotWidth / n);
  const yFor = (percent: number) => top + (1 - percent / 100) * plotHeight;
  const barWidth = (plotWidth / n) * 0.5;

  const vpipOrdered = POSITION_ORDER.map((position) => vpip.find((b) => b.label === position) ?? null);
  const pfrOrdered = POSITION_ORDER.map((position) => pfr.find((b) => b.label === position) ?? null);
  const linePoints = vpipOrdered
    .map((bar, index) => (bar?.value != null ? { x: xFor(index), y: yFor(bar.value) } : null))
    .filter((point): point is { x: number; y: number } => point !== null);

  return (
    <div className="dh-mda-svg-chart">
      <div className="dh-mda-legend">
        <LegendSwatch color={CHART_BLUE} label="VPIP" />
        <LegendSwatch color={CHART_ORANGE} label="PFR" />
      </div>
      <svg ref={chartRef} viewBox={`0 0 ${width} ${height}`} role="img" aria-label="Positional VPIP / PFR by Seat">
        <ChartGrid width={width} height={height} left={left} right={right} top={top} bottom={bottom} yTicks={[0, 25, 50, 75, 100]} yFor={yFor} />
        {pfrOrdered.map((bar, index) => {
          if (bar?.value == null) return null;
          const barHeight = (bar.value / 100) * plotHeight;
          // A short bar has no room for a label *inside* it - the
          // reference sits the number inside the bar near its top only
          // when there's space, otherwise just above it, never floating
          // detached from its own bar.
          const insideBar = barHeight >= 16;
          return (
            <g key={POSITION_ORDER[index]}>
              <rect x={xFor(index) - barWidth / 2} y={yFor(bar.value)} width={barWidth} height={barHeight} rx={3} fill={CHART_ORANGE}>
                <title>{`PFR ${POSITION_ORDER[index]}: ${bar.value.toFixed(1)}% (${barTitle(bar)})`}</title>
              </rect>
              <text
                x={xFor(index)}
                y={insideBar ? yFor(bar.value) + 13 : yFor(bar.value) - 5}
                textAnchor="middle"
                fill={insideBar ? "#ffffff" : "#f4f6ff"}
                fontSize="12"
                fontWeight={700}
              >
                {`${bar.value.toFixed(1)}%`}
              </text>
            </g>
          );
        })}
        {linePoints.length > 0 && (
          <path d={smoothLinePath(linePoints)} fill="none" stroke={CHART_BLUE} strokeWidth={2.2} strokeLinejoin="round" strokeLinecap="round" />
        )}
        {vpipOrdered.map((bar, index) =>
          bar?.value != null ? (
            <g key={POSITION_ORDER[index]}>
              <circle cx={xFor(index)} cy={yFor(bar.value)} r={4} fill={CHART_BLUE}>
                <title>{`VPIP ${POSITION_ORDER[index]}: ${bar.value.toFixed(1)}% (${barTitle(bar)})`}</title>
              </circle>
              <text x={xFor(index)} y={yFor(bar.value) - 10} textAnchor="middle" fill={CHART_POINT_LABEL} fontSize="12" fontWeight={700}>
                {`${bar.value.toFixed(1)}%`}
              </text>
            </g>
          ) : null
        )}
        {POSITION_ORDER.map((position, index) => (
          <text key={position} x={xFor(index)} y={height - 5} textAnchor="middle" fill="#71809f" fontSize="12">
            {position}
          </text>
        ))}
      </svg>
    </div>
  );
}

const ARCHETYPES = ["Nit", "Fish", "Standard Reg", "Tight Reg", "Bad LAG", "Tricky LAG", "Whale", "Nutball"] as const;

// One sequential hue (blue), opacity-scaled by magnitude, for a cell with
// real data; a plain muted dash for "no data yet" - see the file header
// for why this deliberately isn't the all-red-when-empty look Drivetracker's
// own reference heatmap uses.
// `unit="bb100"` reuses this same table for a diverging money metric
// ("Flop EV BB/100 by Archetype") instead of the usual 0-100% sequential
// one - formatted like every other bb/100 figure in this file instead of a
// misleading "%" suffix on a number that was never a percentage.
//
// `variant="flop"` is the Flop tabs' own look: full-bleed cells on a red ->
// green scale (the reference paints an empty grid solid red) with "EP" for
// the earliest seat. A cell with no eligible hands is painted the same red
// but reads "--", never a "0" - it still isn't a measured zero.
function ColdCallHeatmap({
  rows,
  unit = "percent",
  variant = "default",
}: {
  rows: MdaHeatmapRow[];
  unit?: "percent" | "bb100" | "number";
  variant?: "default" | "flop";
}) {
  const flop = variant === "flop";
  return (
    <div className="dh-mda-heatmap-wrap">
      <table className={flop ? "dh-mda-heatmap dh-mda-heatmap-flop" : "dh-mda-heatmap"}>
        <thead>
          <tr>
            <th />
            {ARCHETYPES.map((archetype) => (
              <th key={archetype}>{archetype}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.position}>
              <th scope="row">{flop && row.position === "UTG" ? "EP" : row.position}</th>
              {row.cells.map((cell) => {
                const hasData = cell.value !== null && cell.sample_size > 0;
                const value = cell.value ?? 0;
                let background: string | undefined;
                let text = "0";
                if (hasData && unit === "bb100") {
                  const alpha = 0.12 + (Math.min(100, Math.abs(value)) / 100) * 0.68;
                  if (flop) {
                    background = value >= 0 ? chartAlpha(CHART_GREEN, 0.4 + alpha / 2) : chartAlpha(CHART_MUTED_RED, 0.36 + alpha / 2);
                  } else {
                    background = value >= 0 ? chartAlpha(CHART_BLUE, alpha) : chartAlpha(CHART_MUTED_RED, alpha);
                  }
                  text = formatBarValue(value, "bb100");
                } else if (hasData) {
                  const clamped = unit === "number" ? Math.min(100, Math.max(0, (value / 5) * 100)) : Math.min(100, Math.max(0, value));
                  if (flop) {
                    background = chartAlpha(CHART_GREEN, 0.18 + (clamped / 100) * 0.72);
                  } else {
                    background = chartAlpha(CHART_BLUE, 0.12 + (clamped / 100) * 0.68);
                  }
                  text = unit === "number" ? value.toFixed(2) : `${value.toFixed(0)}%`;
                }
                const empty = !hasData;
                const className = empty
                  ? flop
                    ? "dh-mda-heatmap-cell dh-mda-heatmap-cell-flop-empty"
                    : "dh-mda-heatmap-cell dh-mda-heatmap-cell-empty"
                  : "dh-mda-heatmap-cell";
                return (
                  <td key={cell.label} className={className} style={background ? { background } : undefined} title={barTitle(cell)}>
                    {text}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// A single stacked bar splitting every "faced an open" response into
// Fold/Call/Raise shares - a categorical 3-way split of one dimension, per
// the color-formula "identity" rule, not a magnitude or diverging value.
function FoldCallRaiseBar({ data }: { data: { fold_percent: number | null; call_percent: number | null; raise_percent: number | null; sample_size: number } }) {
  const hasData = data.sample_size > 0;
  const fold = data.fold_percent ?? 0;
  const call = data.call_percent ?? 0;
  const raise = data.raise_percent ?? 0;
  return (
    <div className="dh-mda-fcr">
      <div className="dh-mda-legend">
        <LegendSwatch color={CHART_RED} label="Fold" />
        <LegendSwatch color={CHART_BLUE} label="Call" />
        <LegendSwatch color={CHART_ORANGE} label="Raise" />
      </div>
      {hasData ? (
        <div className="dh-mda-fcr-track" title={`n = ${data.sample_size}`}>
          <span style={{ width: `${fold}%`, background: CHART_RED }} title={`Fold ${fold.toFixed(1)}%`} />
          <span style={{ width: `${call}%`, background: CHART_BLUE }} title={`Call ${call.toFixed(1)}%`} />
          <span style={{ width: `${raise}%`, background: CHART_ORANGE }} title={`Raise ${raise.toFixed(1)}%`} />
        </div>
      ) : (
        <div className="dh-mda-fcr-empty">No data to plot</div>
      )}
      {hasData && (
        <div className="dh-mda-fcr-values">
          <span>Fold {fold.toFixed(1)}%</span>
          <span>Call {call.toFixed(1)}%</span>
          <span>Raise {raise.toFixed(1)}%</span>
        </div>
      )}
    </div>
  );
}

function ColdCallFrequencyImbalance() {
  const [data, setData] = useState<MdaColdCallFrequencyImbalance | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const result = await invoke<MdaColdCallFrequencyImbalance>("get_mda_cold_call_frequency_imbalance", mdaInvokeArgs(gameType));
      setData(result);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // Same reasoning as "Positional EV Leakage": classifying every distinct
    // opponent's archetype from their aggregate VPIP/PFR is real per-hand
    // work, not a cheap tally - a manual refresh covers "I just imported
    // more hands" without redoing it on a timer.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data) {
    return <div className="dh-mda-loading">Se calculează din istoricul importat...</div>;
  }

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mână importată" : "mâini importate"}. Doar{" "}
          {data.classified_opponents} {data.classified_opponents === 1 ? "adversar a strâns" : "adversari au strâns"}{" "}
          destule mâini (minimum 15) pentru a primi un arhetip - coloanele fără date arată „--", nu 0%.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculează..." : "Reîmprospătează"}
        </button>
      </div>
      <div className="dh-mda-grid-cold-call">
        <section className="dh-panel dh-mda-cold-frequency">
          <PanelHeader title="Cold Call Frequency (%)" />
          <ColdCallHeatmap rows={data.cold_call_frequency} />
        </section>
        <section className="dh-panel dh-mda-cold-vpip">
          <PanelHeader title="Positional VPIP / PFR by Seat" />
          <VpipPfrChart vpip={data.vpip_by_position} pfr={data.pfr_by_position} />
        </section>
        <section className="dh-panel dh-mda-cold-winrate">
          <PanelHeader title="Cold Call Win Rate (BB/100)" />
          <AxisBarChart rows={data.cold_call_win_rate_by_archetype} unit="bb100" mode="diverging" />
        </section>
        <section className="dh-panel dh-mda-cold-threebet">
          <PanelHeader title="3-Bet Frequency %" />
          <ArchetypeBarChart rows={data.three_bet_frequency_by_archetype} />
        </section>
        <section className="dh-panel dh-mda-cold-fold-open">
          <PanelHeader title="Fold vs Open Imbalance" />
          <FoldCallRaiseBar data={data.fold_vs_open} />
        </section>
        <section className="dh-panel dh-mda-cold-position">
          <PanelHeader title="Cold Call by Position (%)" />
          <AxisBarChart rows={data.cold_call_by_position} unit="percent" mode="sequential" colorFor={trafficLightColor} />
        </section>
      </div>
    </div>
  );
}

// A single-series vertical bar chart over the 8 fixed archetype slots -
// used for the "how often does this player type do X" panels. Sized 1:1 to
// its container (so the text stays the size it's written at instead of
// scaling with the panel), with a "nice" 4-interval y-axis, dashed
// gridlines, a value label above every measured bar (a measured 0 still
// prints "0.0" - only a bar with no eligible hands prints nothing), and
// diagonal category labels since "Standard Reg"/"Tricky LAG" don't fit flat.
function niceStep(raw: number): number {
  if (raw <= 0) return 0.25;
  const magnitude = Math.pow(10, Math.floor(Math.log10(raw)));
  const fraction = raw / magnitude;
  return (fraction <= 1 ? 1 : fraction <= 2 ? 2 : fraction <= 2.5 ? 2.5 : fraction <= 5 ? 5 : 10) * magnitude;
}

function ArchetypeBarChart({ rows, height: fallbackHeight = ARCHETYPE_CHART_HEIGHT }: { rows: MdaBar[]; height?: number }) {
  const [chartRef, chartSize] = useSvgChartSize(360, fallbackHeight, 240, 160);
  const width = chartSize.width;
  const height = chartSize.height;

  const left = 38;
  const right = 8;
  const top = 12;
  const bottom = 72;
  const plotWidth = width - left - right;
  const plotHeight = height - top - bottom;
  const n = Math.max(1, rows.length);
  const xFor = (index: number) => left + (index + 0.5) * (plotWidth / n);
  const barWidth = Math.min(34, (plotWidth / n) * 0.58);
  const tickStep = niceStep(Math.max(0, ...rows.map((row) => row.value ?? 0)) / 4);
  const axisMax = tickStep * 4;
  const ticks = [0, 1, 2, 3, 4].map((i) => tickStep * i);
  const yFor = (value: number) => top + (1 - value / axisMax) * plotHeight;
  const tickText = (value: number) => String(Number(value.toFixed(2)));

  return (
    <svg className="dh-mda-archetype-chart" ref={chartRef} viewBox={`0 0 ${width} ${height}`} role="img" aria-label="Archetype frequency chart">
      {ticks.map((tick) => (
        <g key={tick}>
          <line x1={left} x2={width - right} y1={yFor(tick)} y2={yFor(tick)} stroke="rgba(135,153,190,0.22)" strokeWidth="1" strokeDasharray="3,3" />
          <text x={left - 6} y={yFor(tick) + 4} textAnchor="end" fill="#71809f" fontSize="11">
            {tickText(tick)}
          </text>
        </g>
      ))}
      <line x1={left} x2={left} y1={top} y2={top + plotHeight} stroke="rgba(152,166,204,0.5)" strokeWidth="1" />
      <line x1={left} x2={width - right} y1={top + plotHeight} y2={top + plotHeight} stroke="rgba(152,166,204,0.5)" strokeWidth="1" />
      <text x={10} y={top + plotHeight / 2} textAnchor="middle" fill="#71809f" fontSize="11" transform={`rotate(-90 10 ${top + plotHeight / 2})`}>
        %
      </text>
      {rows.map((row, index) => {
        if (row.value === null) {
          return (
            <text key={row.label} x={xFor(index)} y={top + plotHeight - 5} textAnchor="middle" fill="#71809f" fontSize="11">
              0.00
            </text>
          );
        }
        const barHeight = (row.value / axisMax) * plotHeight;
        return (
          <g key={row.label}>
            <rect x={xFor(index) - barWidth / 2} y={yFor(row.value)} width={barWidth} height={barHeight} rx={2} fill={CHART_BLUE}>
              <title>{`${row.label}: ${row.value.toFixed(1)}% (${barTitle(row)})`}</title>
            </rect>
            <text x={xFor(index)} y={yFor(row.value) - 6} textAnchor="middle" fill="#f4f6ff" fontSize="11" fontWeight="700">
              {row.value.toFixed(1)}
            </text>
          </g>
        );
      })}
      {rows.map((row, index) => (
        <text
          key={row.label}
          x={xFor(index)}
          y={height - bottom + 14}
          textAnchor="end"
          fill="#71809f"
          fontSize="11"
          transform={`rotate(-90 ${xFor(index)} ${height - bottom + 14})`}
        >
          {row.label}
        </text>
      ))}
    </svg>
  );
}

function PreflopAggressionProfitability() {
  const [data, setData] = useState<MdaPreflopAggressionProfitability | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const result = await invoke<MdaPreflopAggressionProfitability>("get_mda_preflop_aggression_profitability", mdaInvokeArgs(gameType));
      setData(result);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // Same reasoning as the other two archetype-driven tabs: classifying
    // every opponent and walking the full preflop sequence per hand is
    // real work, not a cheap tally - refresh manually, not on a timer.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data) {
    return <div className="dh-mda-loading">Se calculează din istoricul importat...</div>;
  }

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mână importată" : "mâini importate"}. Doar{" "}
          {data.classified_opponents} {data.classified_opponents === 1 ? "adversar a strâns" : "adversari au strâns"}{" "}
          destule mâini (minimum 15) pentru a primi un arhetip.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculează..." : "Reîmprospătează"}
        </button>
      </div>
      <div className="dh-mda-grid-2">
        <section className="dh-panel">
          <PanelHeader title="RFI Aggression Profitability" />
          <ColdCallHeatmap rows={data.rfi_aggression_profitability} />
        </section>
        <section className="dh-panel">
          <PanelHeader title="Positional VPIP / PFR by Seat" />
          <VpipPfrChart vpip={data.vpip_by_position} pfr={data.pfr_by_position} />
        </section>
      </div>
      <div className="dh-mda-grid-5">
        <section className="dh-panel">
          <PanelHeader title="3-Bet Frequency %" />
          <ArchetypeBarChart rows={data.three_bet_frequency_by_archetype} />
        </section>
        <section className="dh-panel">
          <PanelHeader title="4-Bet Frequency %" />
          <ArchetypeBarChart rows={data.four_bet_frequency_by_archetype} />
        </section>
        <section className="dh-panel">
          <PanelHeader title="Cold Call Frequency %" />
          <ArchetypeBarChart rows={data.cold_call_frequency_by_archetype} />
        </section>
        <section className="dh-panel">
          <PanelHeader title="Overcall Frequency %" />
          <ArchetypeBarChart rows={data.overcall_frequency_by_archetype} />
        </section>
        <section className="dh-panel">
          <PanelHeader title="Squeeze Frequency %" />
          <ArchetypeBarChart rows={data.squeeze_frequency_by_archetype} />
        </section>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Preflop -> "Positional EV Realization"
// ---------------------------------------------------------------------
//
// A visual-only threshold layered over real numbers already computed
// elsewhere (`bb_per_100_by_position` is the same figure "Positional EV
// Leakage" shows) - not a separate statistic. The thresholds themselves
// are a display convention, not a tracker-standard figure, so they're
// documented here rather than claimed as anything more rigorous.
function evRealizationColor(value: number | null): string {
  if (value === null) return chartAlpha(CHART_BLUE, 0.28);
  if (value >= 10) return CHART_GREEN; // Elite
  if (value >= 0) return chartAlpha(CHART_GREEN, 0.68); // Strong
  if (value >= -10) return CHART_ORANGE; // Weak
  return CHART_RED; // Leaking
}

function trafficLightColor(value: number | null): string {
  if (value === null) return chartAlpha(CHART_BLUE, 0.28);
  if (value >= 60) return CHART_GREEN;
  if (value >= 35) return CHART_ORANGE;
  return CHART_RED;
}

function PositionalEvRealization() {
  const [data, setData] = useState<MdaPositionalEvRealization | null>(null);
  const [leakage, setLeakage] = useState<MdaPositionalEvLeakage | null>(null);
  const [archetype, setArchetype] = useState<MdaPreflopArchetypeDistribution | null>(null);
  const [aggression, setAggression] = useState<MdaPreflopAggressionProfitability | null>(null);
  const [defense, setDefense] = useState<MdaPreflopDefenseVsRfi | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const [result, leakageResult, archetypeResult, aggressionResult, defenseResult] = await Promise.all([
        invoke<MdaPositionalEvRealization>("get_mda_positional_ev_realization", mdaInvokeArgs(gameType)),
        invoke<MdaPositionalEvLeakage>("get_mda_positional_ev_leakage", mdaInvokeArgs(gameType)),
        invoke<MdaPreflopArchetypeDistribution>("get_mda_preflop_archetype_distribution", mdaInvokeArgs(gameType)),
        invoke<MdaPreflopAggressionProfitability>("get_mda_preflop_aggression_profitability", mdaInvokeArgs(gameType)),
        invoke<MdaPreflopDefenseVsRfi>("get_mda_preflop_defense_vs_rfi", mdaInvokeArgs(gameType)),
      ]);
      setData(result);
      setLeakage(leakageResult);
      setArchetype(archetypeResult);
      setAggression(aggressionResult);
      setDefense(defenseResult);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const fallbackBar = (label: string): MdaBar => ({ label, value: null, sample_size: 0 });
  const bar = (rows: MdaBar[] | undefined, label: string) => rows?.find((row) => row.label === label);
  const rfiRows = ["CO", "BTN"].map((position) => ({
    ...(bar(archetype?.pfr_by_position, position) ?? fallbackBar(position)),
    label: `RFI ${position}`,
  }));
  const stealRows = ["CO", "BTN", "SB"].map((position) => ({
    ...(bar(data?.steal_frequency_by_position, position) ?? fallbackBar(position)),
    label: `Steal from ${position}%`,
  }));
  const foldThreeBetRows = ["CO", "BTN"].map((position) => ({
    ...(bar(archetype?.fold_to_three_bet_by_position, position) ?? fallbackBar(position)),
    label: `Fold to 3Bet ${position}%`,
  }));
  const callThreeBetRows = ["CO", "BTN"].map((position) => {
    const source = bar(archetype?.fold_to_three_bet_by_position, position);
    return {
      label: `Call 3Bet ${position}%`,
      value: source?.value == null ? null : Math.max(0, 100 - source.value),
      sample_size: source?.sample_size ?? 0,
    };
  });
  const threeBetVsStealRows = [
    {
      ...(bar(defense?.three_bet_vs_open, "SB") ?? fallbackBar("SB")),
      label: "3Bet vs. Steal in SB %",
    },
    {
      ...(bar(defense?.three_bet_vs_open, "BB") ?? fallbackBar("BB")),
      label: "3Bet vs. Steal in BB %",
    },
  ];
  const foldBlindRows = [
    {
      ...(bar(defense?.fold_to_steal, "SB Fold to Steal%") ?? bar(defense?.fold_to_steal, "SB") ?? fallbackBar("SB")),
      label: "SB Fold to Steal%",
    },
    {
      ...(bar(defense?.fold_to_steal, "BB Fold to Steal%") ?? bar(defense?.fold_to_steal, "BB") ?? fallbackBar("BB")),
      label: "BB Fold to Steal%",
    },
  ];
  const coldCallRows = ["CO", "BTN"].map((position) => ({
    ...(bar(data?.cold_call_frequency_by_position, position) ?? fallbackBar(position)),
    label: `Cold Call ${position}%`,
  }));
  const bbMetric = weightedBars(data?.bb_per_100_by_position ?? []);
  const stealMetric = weightedBars(data?.steal_frequency_by_position ?? []);
  const coldMetric = weightedBars(data?.cold_call_frequency_by_position ?? []);
  const foldMetric = weightedBars(archetype?.fold_to_three_bet_by_position ?? []);

  if (!data || !leakage || !archetype || !aggression || !defense) {
    return <div className="dh-mda-loading">Se calculează din istoricul importat...</div>;
  }

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mână importată" : "mâini importate"}. Culorile
          barelor (Elite / Strong / Weak / Leaking pentru BB/100, verde/galben/roșu pentru frecvențe) sunt un prag
          vizual peste numerele reale, nu o statistică separată.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculează..." : "Reîmprospătează"}
        </button>
      </div>
      <div className="dh-mda-grid-4 dh-mda-grid-pos-real">
        <InsightPanel
          title="BB/100 by Position"
          insight={{
            tone: "red",
            title: "Population Exploit: Late-Position EV Leak",
            text: `BTN or CO winrates are below zero, indicating positional value is not being converted into profit. Current average: ${formatBarValue(
              bbMetric.value,
              "bb100"
            )} bb/100.`,
          }}
        >
          <div className="dh-mda-legend">
            <LegendSwatch color={CHART_GREEN} label="Elite (>=25)" />
            <LegendSwatch color={chartAlpha(CHART_GREEN, 0.68)} label="Strong (10-24)" />
            <LegendSwatch color={CHART_ORANGE} label="Weak (0-9)" />
            <LegendSwatch color={CHART_RED} label="Leaking (<0)" />
          </div>
          <AxisBarChart rows={data.bb_per_100_by_position} unit="bb100" mode="diverging" colorFor={evRealizationColor} />
        </InsightPanel>
        <InsightPanel
          title="Archetype-Sliced Positional Winrate"
          className="dh-mda-span-2"
          insight={{
            tone: "yellow",
            title: "Population Signal: Disciplined Players Underutilizing Position",
            text: "TAG and regular archetypes are failing to fully monetize late-position leverage, likely due to passive opens or weak defense versus 3-bet aggression.",
          }}
        >
          <ColdCallHeatmap rows={aggression.rfi_aggression_profitability} unit="bb100" variant="flop" />
        </InsightPanel>
        <InsightPanel
          title="All-In Adjusted EV by Position (BB/100)"
          className="dh-mda-panel-tall"
          insight={{
            tone: "red",
            title: "Population Signal: Late-Position EV Collapse",
            text: "All-in adjusted EV is negative in key late-position seats such as the CO or BTN. This indicates structural inefficiency in how positional advantage is being converted into profit.",
          }}
        >
          <GroupedVerticalBarChart
            rows={leakage.actual_bb_per_100}
            secondary={leakage.all_in_ev_bb_per_100}
            primaryLabel="Actual BB/100"
            secondaryLabel="All-In EV BB/100"
            unit="bb100"
          />
        </InsightPanel>
        <InsightPanel
          title="RFI by Position"
          insight={{
            tone: "yellow",
            title: "Population signal: positional advantage underused.",
            text: "CO and/or BTN opening frequencies fall below expected benchmarks. This suggests the population may not be fully leveraging late-position advantage.",
          }}
        >
          <AxisBarChart rows={rfiRows} unit="percent" mode="sequential" colorFor={trafficLightColor} />
        </InsightPanel>
        <InsightPanel
          title="Steal Frequency by Position"
          insight={{
            tone: "yellow",
            title: "Population signal: late-position pressure under-applied",
            text: `Steal frequency from late positions is ${formatBarValue(
              stealMetric.value,
              "percent"
            )}, which can fall below expected benchmarks. This suggests missed blind-pressure opportunities.`,
          }}
        >
          <AxisBarChart rows={stealRows} unit="percent" mode="sequential" colorFor={trafficLightColor} />
        </InsightPanel>
        <InsightPanel
          title="Fold to 3-Bet After Opening"
          insight={{
            tone: "blue",
            title: "Adjustment: Resistant Positional Defense",
            text: `Players continue versus 3-bets at a high rate after opening from late positions. Current fold-to-3bet pool read: ${formatBarValue(
              foldMetric.value,
              "percent"
            )}.`,
          }}
        >
          <AxisBarChart rows={foldThreeBetRows} unit="percent" mode="sequential" colorFor={trafficLightColor} />
        </InsightPanel>
        <InsightPanel
          title="Call 3-Bet After Opening"
          insight={{
            tone: "blue",
            title: "Population Tendency: Limited Calling versus 3-Bets",
            text: "Players rarely call 3-bets after opening, preferring to fold or escalate aggression instead. Keep pressure selective and value-heavy when calls appear.",
          }}
        >
          <AxisBarChart rows={callThreeBetRows} unit="percent" mode="sequential" colorFor={trafficLightColor} />
        </InsightPanel>
        <InsightPanel
          title="3-Bet vs Steal"
          insight={{
            tone: "red",
            title: "Population Exploit: Blinds Under-Pressure BTN Steals",
            text: "3-bet frequency from the blinds versus steals can fall below expected benchmarks. This indicates that late positions open too efficiently when blinds fail to punish.",
          }}
        >
          <AxisBarChart rows={threeBetVsStealRows} unit="percent" mode="sequential" colorFor={trafficLightColor} />
        </InsightPanel>
        <InsightPanel
          title="Fold Blind to Steal"
          insight={{
            tone: "blue",
            title: "Adjustment: Blinds are Sticky vs Steals",
            text: "The blinds are defending frequently versus opens, reducing auto-profit from steals. Shift toward value-heavy opening ranges and prioritize post-flop edge.",
          }}
        >
          <AxisBarChart rows={foldBlindRows} unit="percent" mode="sequential" colorFor={trafficLightColor} />
        </InsightPanel>
        <InsightPanel
          title="Cold Call Frequency by Position"
          insight={{
            tone: "red",
            title: "Population Signal: Excessive BTN Cold Calling",
            text: `Cold-call frequency is ${formatBarValue(
              coldMetric.value,
              "percent"
            )}. Passive calls from late positions can cap ranges and create structural leaks.`,
          }}
        >
          <AxisBarChart rows={coldCallRows} unit="percent" mode="sequential" colorFor={trafficLightColor} />
        </InsightPanel>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Preflop -> "Preflop Archetype Distribution"
// ---------------------------------------------------------------------

const ARCHETYPE_COLORS: Record<string, string> = {
  Nit: CHART_BLUE,
  Fish: CHART_ORANGE,
  "Standard Reg": CHART_GREEN,
  "Tight Reg": CHART_GREEN,
  "Bad LAG": CHART_RED,
  "Tricky LAG": CHART_MAGENTA,
  Whale: chartAlpha(CHART_ORANGE, 0.72),
  Nutball: chartAlpha(CHART_RED, 0.78),
};

const VPIP_TIER_COLORS: Record<string, string> = {
  Nit: CHART_BLUE,
  TAG: CHART_GREEN,
  LAG: CHART_ORANGE,
  "Loose/Whale": CHART_RED,
};

const PFR_TIER_COLORS: Record<string, string> = {
  Passive: CHART_BLUE,
  Balanced: CHART_ORANGE,
  Aggressive: CHART_GREEN,
};

function DonutChart({ rows, colors }: { rows: MdaBar[]; colors: Record<string, string> }) {
  const size = 240;
  const cx = size / 2;
  const cy = size / 2;
  const outerR = 96;
  const innerR = 58;
  const total = rows.reduce((sum, row) => sum + (row.value ?? 0), 0);

  const rad = (deg: number) => (deg * Math.PI) / 180;
  const arcPath = (startAngle: number, endAngle: number): string => {
    const largeArc = endAngle - startAngle > 180 ? 1 : 0;
    const x1o = cx + outerR * Math.cos(rad(startAngle));
    const y1o = cy + outerR * Math.sin(rad(startAngle));
    const x2o = cx + outerR * Math.cos(rad(endAngle));
    const y2o = cy + outerR * Math.sin(rad(endAngle));
    const x1i = cx + innerR * Math.cos(rad(endAngle));
    const y1i = cy + innerR * Math.sin(rad(endAngle));
    const x2i = cx + innerR * Math.cos(rad(startAngle));
    const y2i = cy + innerR * Math.sin(rad(startAngle));
    return `M ${x1o} ${y1o} A ${outerR} ${outerR} 0 ${largeArc} 1 ${x2o} ${y2o} L ${x1i} ${y1i} A ${innerR} ${innerR} 0 ${largeArc} 0 ${x2i} ${y2i} Z`;
  };

  let angle = -90;
  const arcs =
    total > 0
      ? rows
          .filter((row) => (row.value ?? 0) > 0)
          .map((row) => {
            const value = row.value ?? 0;
            const startAngle = angle;
            const endAngle = angle + (value / total) * 360;
            angle = endAngle;
            return { row, startAngle, endAngle, color: colors[row.label] ?? CHART_BLUE };
          })
      : [];

  return (
    <div className="dh-mda-donut-wrap">
      <svg viewBox={`0 0 ${size} ${size}`} role="img" aria-label="Preflop Archetype Distribution">
        {arcs.length === 0 ? (
          <>
            <circle cx={cx} cy={cy} r={(outerR + innerR) / 2} fill="none" stroke="rgba(135,153,190,0.25)" strokeWidth={outerR - innerR} />
            <text x={cx} y={cy + 5} textAnchor="middle" fill="#f4f6ff" fontSize="13">
              No data to plot
            </text>
          </>
        ) : (
          <>
            {arcs.map((arc) => {
              const percent = (((arc.row.value ?? 0) * 100) / total).toFixed(0);
              const sweep = arc.endAngle - arc.startAngle;
              const midAngle = arc.startAngle + sweep / 2;
              const labelR = (outerR + innerR) / 2;
              const labelX = cx + labelR * Math.cos(rad(midAngle));
              const labelY = cy + labelR * Math.sin(rad(midAngle)) + 4;
              const title = `${arc.row.label}: ${arc.row.value} (${(((arc.row.value ?? 0) * 100) / total).toFixed(1)}%)`;
              return (
                <g key={arc.row.label}>
                  {sweep >= 359.999 ? (
                    <circle cx={cx} cy={cy} r={labelR} fill="none" stroke={arc.color} strokeWidth={outerR - innerR}>
                      <title>{title}</title>
                    </circle>
                  ) : (
                    <path d={arcPath(arc.startAngle, arc.endAngle)} fill={arc.color}>
                      <title>{title}</title>
                    </path>
                  )}
                  {Number(percent) >= 4 && (
                    <text x={labelX} y={labelY} textAnchor="middle" fill="#ffffff" fontSize="14" fontWeight={700}>
                      {percent}%
                    </text>
                  )}
                </g>
              );
            })}
            <text x={cx} y={cy - 4} textAnchor="middle" fill="#f4f6ff" fontSize="26" fontWeight={600}>
              {total}
            </text>
            <text x={cx} y={cy + 17} textAnchor="middle" fill="#8b98b8" fontSize="11">
              adversari
            </text>
          </>
        )}
      </svg>
      <div className="dh-mda-donut-legend">
        {rows.map((row) => (
          <div key={row.label} className="dh-mda-donut-legend-row">
            <LegendSwatch color={colors[row.label] ?? CHART_BLUE} label={row.label} />
            <span className="dh-mda-donut-legend-value">
              {(row.value ?? 0) > 0 ? `${row.value} (${(((row.value ?? 0) * 100) / Math.max(1, total)).toFixed(1)}%)` : "--"}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

function barTotal(rows: MdaBar[]): number {
  return rows.reduce((sum, row) => sum + (row.value ?? 0), 0);
}

function dominantBar(rows: MdaBar[]): MdaBar | null {
  return rows.reduce<MdaBar | null>((best, row) => {
    if ((row.value ?? 0) <= 0) return best;
    if (!best || (row.value ?? 0) > (best.value ?? 0)) return row;
    return best;
  }, null);
}

function archetypeShareInsight(rows: MdaBar[], label: string, title: string, text: (share: string) => string, tone: InsightTone): Insight {
  const total = barTotal(rows);
  const row = rows.find((entry) => entry.label === label);
  const share = total > 0 ? (((row?.value ?? 0) * 100) / total).toFixed(1) : "0.0";
  return { tone, title, text: text(share) };
}

function pfrGapInsight(vpip: MdaBar[], pfr: MdaBar[]): Insight {
  const weighted = (rows: MdaBar[]) => weightedBars(rows).value ?? 0;
  const gap = Math.max(0, weighted(vpip) - weighted(pfr));
  return {
    tone: gap > 11 ? "yellow" : "green",
    title: gap > 11 ? "Population exploit: passive range construction" : "No major VPIP/PFR gap detected",
    text:
      gap > 11
        ? "A large portion of the population shows a VPIP/PFR gap above 11%, indicating that many players enter pots through calling rather than raising. This creates a passive ecosystem where capped ranges appear frequently."
        : `The population VPIP/PFR gap is ${gap.toFixed(1)}%, which does not show a clear passive-entry imbalance. Continue using standard position-based reads.`,
  };
}

function frequencyInsight(rows: MdaBar[], label: string, title: string, text: string, tone: InsightTone): Insight {
  const row = rows.find((entry) => entry.label === label) ?? dominantBar(rows);
  const sample = rows.reduce((sum, entry) => sum + entry.sample_size, 0);
  const provisional = sample < SMALL_SAMPLE_THRESHOLD ? "[Small Sample — treat as provisional] " : "";
  const value = row?.value ?? 0;
  return {
    tone,
    title: `${provisional}${title}`,
    text: text.replace("{v}", `${value.toFixed(1)}%`).replace("{label}", row?.label ?? label),
  };
}

function PreflopArchetypeDistribution() {
  const [data, setData] = useState<MdaPreflopArchetypeDistribution | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const result = await invoke<MdaPreflopArchetypeDistribution>("get_mda_preflop_archetype_distribution", mdaInvokeArgs(gameType));
      setData(result);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data) {
    return <div className="dh-mda-loading">Se calculează din istoricul importat...</div>;
  }

  const vpipInsight = archetypeShareInsight(
    data.vpip_tier_distribution,
    "Loose/Whale",
    "Population Signal: Loose-Dominated Pool",
    (share) =>
      `A large portion (${share}%) of the population has VPIP above 30%. This indicates a loose ecosystem that typically produces high occupancy multi-way pots. Strategic priority should shift toward value-heavy opens and isolation.`,
    "red"
  );
  const pfrInsight = archetypeShareInsight(
    data.pfr_tier_distribution,
    "Passive",
    "Population Exploit: Station-Heavy Participation",
    (share) =>
      `A large portion (${share}%) of the pool frequently enters pots but rarely raises. This indicates a passive, flat-heavy ecosystem dominated by weak ranges. Strategic priority should shift toward aggression and thin value.`,
    "red"
  );
  const poolInsight = archetypeShareInsight(
    data.archetype_counts,
    dominantBar(data.archetype_counts)?.label ?? "Fish",
    "Population Signal: Station-Dominated Ecosystem",
    (share) =>
      `A large portion (${share}%) of the population consists of station or loose-passive archetypes. This environment typically produces frequent multi-way pots and rewards disciplined value betting.`,
    "red"
  );

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mână importată" : "mâini importate"}. Doar{" "}
          {data.classified_opponents} {data.classified_opponents === 1 ? "adversar a strâns" : "adversari au strâns"}{" "}
          destule mâini (minimum 15) pentru a primi un arhetip - Hero nu este numărat, doar ceilalți jucători.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculează..." : "Reîmprospătează"}
        </button>
      </div>
      <div className="dh-mda-grid-4 dh-mda-grid-arch-dist">
        <InsightPanel title="VPIP% Ecosystem Breakdown" insight={vpipInsight}>
          <DonutChart rows={data.vpip_tier_distribution} colors={VPIP_TIER_COLORS} />
        </InsightPanel>
        <InsightPanel title="PFR% Aggression Breakdown" insight={pfrInsight}>
          <DonutChart rows={data.pfr_tier_distribution} colors={PFR_TIER_COLORS} />
        </InsightPanel>
        <InsightPanel title="Positional VPIP / PFR by Seat" className="dh-mda-span-2" insight={pfrGapInsight(data.vpip_by_position, data.pfr_by_position)}>
          <VpipPfrChart vpip={data.vpip_by_position} pfr={data.pfr_by_position} />
        </InsightPanel>
        <InsightPanel
          title="3-Bet Frequency %"
          insight={frequencyInsight(
            data.three_bet_frequency_by_archetype,
            "Nit",
            "Population Exploit: Low 3-Bet",
            "3-bet frequency is {v}, which is below standard benchmarks. The population rarely challenges open raises; this supports wider opening ranges until players adapt.",
            "yellow"
          )}
        >
          <ArchetypeBarChart rows={data.three_bet_frequency_by_archetype} />
        </InsightPanel>
        <InsightPanel
          title="4-Bet Frequency %"
          insight={frequencyInsight(
            data.four_bet_frequency_by_archetype,
            "Nit",
            "Population Exploit: 3-bet Aggression Goes Unpunished",
            "The pool 4-bet frequency is {v}, indicating that players almost never escalate against 3-bets. This supports pressure-heavy 3-betting, especially versus late opens.",
            "yellow"
          )}
        >
          <ArchetypeBarChart rows={data.four_bet_frequency_by_archetype} />
        </InsightPanel>
        <InsightPanel title="Pool Archetype Distribution" insight={poolInsight}>
          <DonutChart rows={data.archetype_counts} colors={ARCHETYPE_COLORS} />
        </InsightPanel>
        <div className="dh-mda-stack dh-mda-arch-stack">
          <InsightPanel
            title="Cold Call Frequency by Position (%)"
            insight={frequencyInsight(
              data.cold_call_frequency_by_position,
              "BTN",
              "Population Signal: Excessive BTN Cold Calling",
              "Button cold-call frequency is {v}, which exceeds standard benchmarks. This suggests the population frequently defends through passive calls rather than 3-betting.",
              "red"
            )}
          >
            <AxisBarChart rows={data.cold_call_frequency_by_position} unit="percent" mode="sequential" colorFor={trafficLightColor} />
          </InsightPanel>
          <InsightPanel
            title="Fold to 3-Bet After Opening (%)"
            insight={frequencyInsight(
              data.fold_to_three_bet_by_position,
              "CO",
              "Adjustment: Resistant Positional Defense",
              "Players continue versus 3-bets at a high rate after opening from late positions. Reduce bluff-heavy 3-bet strategies and shift toward value-driven 3-bet ranges.",
              "blue"
            )}
          >
            <AxisBarChart rows={data.fold_to_three_bet_by_position} unit="percent" mode="sequential" colorFor={trafficLightColor} />
          </InsightPanel>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Preflop -> "Preflop EV Stability"
// ---------------------------------------------------------------------

// A smooth filled-area cumulative curve, same skeleton as
// `EvTrackingChart` (shared axis math, ResizeObserver-driven aspect-ratio
// width) but for exactly two series - hero's actual cumulative result and
// hero's cumulative all-in-adjusted EV - with the area beneath each line
// filled down to the zero baseline so the gap between them (the hand's own
// "stability", i.e. how far actual results have drifted from what the
// cards were really worth) reads visually, not just from the legend.
function EvStabilityChart({
  actual,
  ev,
  actualLabel = "Actual (Cumulative)",
  evLabel = "All-In EV (Cumulative)",
  ariaLabel = "Preflop EV Stability",
  unit = "money",
}: {
  actual: number[];
  ev: number[];
  actualLabel?: string;
  evLabel?: string;
  ariaLabel?: string;
  unit?: "money" | "percent";
}) {
  const [chartRef, chartSize] = useSvgChartSize(640, EV_CHART_HEIGHT, 320, 160);
  const width = chartSize.width;
  const height = chartSize.height;

  const pointCount = actual.length;
  const left = 46;
  const right = 8;
  const top = 10;
  const bottom = 18;
  const plotWidth = width - left - right;
  const plotHeight = height - top - bottom;

  return (
    <div className="dh-mda-svg-chart">
      <div className="dh-mda-legend">
        <LegendSwatch color={CHART_BLUE} label={actualLabel} />
        <LegendSwatch color={CHART_ORANGE} label={evLabel} />
      </div>
      <svg ref={chartRef} viewBox={`0 0 ${width} ${height}`} role="img" aria-label={ariaLabel}>
        {pointCount === 0 ? (
          <>
            <ChartGrid width={width} height={height} />
            <text x={width / 2} y={height / 2 + 5} textAnchor="middle" fill="#f4f6ff" fontSize="13">
              No data to plot
            </text>
          </>
        ) : (
          (() => {
            const allValues = actual.concat(ev);
            const axis = moneyAxis(Math.min(0, ...allValues), Math.max(0, ...allValues));
            const xFor = (index: number) => left + (pointCount === 1 ? plotWidth : (index / (pointCount - 1)) * plotWidth);
            const yFor = (value: number) => top + ((axis.max - value) / axis.span) * plotHeight;
            const zeroY = yFor(0);
            const labelIndexes = chartLabelIndexes(pointCount);
            const points = (series: number[]) => series.map((value, index) => ({ x: xFor(index), y: yFor(value) }));
            const areaPath = (series: number[]) => {
              const pts = points(series);
              const first = pts[0];
              const last = pts[pts.length - 1];
              return `${smoothLinePath(pts)} L ${last.x} ${zeroY} L ${first.x} ${zeroY} Z`;
            };
            return (
              <>
                <ChartGrid width={width} height={height} left={left} right={right} top={top} bottom={bottom} yTicks={axis.ticks} yFor={yFor} />
                <line x1={left} y1={zeroY} x2={width - right} y2={zeroY} stroke="rgba(152,166,204,0.38)" strokeWidth="1" />
                <path d={areaPath(ev)} fill={CHART_ORANGE} opacity={0.14} stroke="none" />
                <path d={areaPath(actual)} fill={CHART_BLUE} opacity={0.16} stroke="none" />
                <path d={smoothLinePath(points(ev))} fill="none" stroke={CHART_ORANGE} strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />
                <path d={smoothLinePath(points(actual))} fill="none" stroke={CHART_BLUE} strokeWidth={2.4} strokeLinejoin="round" strokeLinecap="round" />
                {axis.ticks.map((tick) => (
                  <text key={tick} x={left - 8} y={yFor(tick) + 4} textAnchor="end" fill="#71809f" fontSize="11">
                    {unit === "percent" ? `${tick.toFixed(0)}%` : formatMoney(tick)}
                  </text>
                ))}
                {labelIndexes.map((index) => (
                  <text
                    key={index}
                    x={xFor(index)}
                    y={height - 5}
                    textAnchor={index === 0 ? "start" : index === pointCount - 1 ? "end" : "middle"}
                    fill="#71809f"
                    fontSize="11"
                  >
                    {index + 1}
                  </text>
                ))}
              </>
            );
          })()
        )}
      </svg>
    </div>
  );
}

function PreflopEvStability() {
  const [data, setData] = useState<MdaPreflopEvStability | null>(null);
  // "All-In Adjusted EV by Position" and "Positional EV Drift (CO/BTN)"
  // reuse "Positional EV Leakage"'s own already-computed fields (same
  // numbers, not a re-derivation) instead of duplicating that pass here.
  const [leakage, setLeakage] = useState<MdaPositionalEvLeakage | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const [stability, leakageResult] = await Promise.all([
        invoke<MdaPreflopEvStability>("get_mda_preflop_ev_stability", mdaInvokeArgs(gameType)),
        invoke<MdaPositionalEvLeakage>("get_mda_positional_ev_leakage", mdaInvokeArgs(gameType)),
      ]);
      setData(stability);
      setLeakage(leakageResult);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data || !leakage) {
    return <div className="dh-mda-loading">Se calculează din istoricul importat...</div>;
  }

  const coBtnDrift = leakage.ev_tracking.filter((s: MdaEvSeries) => s.position === "CO" || s.position === "BTN");

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mână importată" : "mâini importate"}. Linia EV
          urmează rezultatul real acolo unde nu a existat un all-in cu ambele mâini cunoscute (singurul caz în care se
          poate calcula un EV exact) - diferența dintre cele două linii arată varianța reală, nu una estimată. Cele
          trei grafice "Over Volume" grupează mâinile după prima decizie preflop a lui Hero (open / 3-bet / cold
          call), nu după arhetipul adversarului. "Contribution to Winrate" arată aceleași două linii ca procent din
          rezultatul final al lui Hero, iar "EV Volatility" arată deviația standard (pe fereastră extinsă) a
          diferenței actual-vs-EV pe fiecare mână.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculează..." : "Reîmprospătează"}
        </button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-ev-stability">
        <section className="dh-panel dh-mda-panel-tall">
          <PanelHeader title="All-In Adjusted EV by Position (BB/100)" />
          <GroupedVerticalBarChart
            rows={leakage.actual_bb_per_100}
            secondary={leakage.all_in_ev_bb_per_100}
            primaryLabel="Actual BB/100"
            secondaryLabel="All-In EV BB/100"
            unit="bb100"
          />
        </section>
        <section className="dh-panel">
          <PanelHeader title="Preflop EV Stability" />
          <EvStabilityChart actual={data.cumulative_actual} ev={data.cumulative_ev} />
        </section>
        <section className="dh-panel">
          <PanelHeader title="Preflop Contribution to Winrate (%)" />
          <EvStabilityChart
            actual={data.contribution_to_final_pct.cumulative_actual}
            ev={data.contribution_to_final_pct.cumulative_ev}
            actualLabel="Actual (% of Final)"
            evLabel="All-In EV (% of Final)"
            ariaLabel="Preflop Contribution to Winrate"
            unit="percent"
          />
        </section>
        <section className="dh-panel">
          <PanelHeader title="RFI EV Over Volume" />
          <EvStabilityChart
            actual={data.rfi_ev.cumulative_actual}
            ev={data.rfi_ev.cumulative_ev}
            actualLabel="RFI Net Won"
            evLabel="RFI EV (All-In Adj.)"
            ariaLabel="RFI EV Over Volume"
          />
        </section>
        <section className="dh-panel">
          <PanelHeader title="3-Bet EV Over Volume" />
          <EvStabilityChart
            actual={data.three_bet_ev.cumulative_actual}
            ev={data.three_bet_ev.cumulative_ev}
            actualLabel="Net Won"
            evLabel="EV (All-In Adjusted)"
            ariaLabel="3-Bet EV Over Volume"
          />
        </section>
        <section className="dh-panel">
          <PanelHeader title="Cold Call EV Over Volume" />
          <EvStabilityChart
            actual={data.cold_call_ev.cumulative_actual}
            ev={data.cold_call_ev.cumulative_ev}
            actualLabel="Net Won"
            evLabel="EV (All-In Adjusted)"
            ariaLabel="Cold Call EV Over Volume"
          />
        </section>
        <section className="dh-panel">
          <PanelHeader title="EV Volatility vs Sample Size" />
          <EvStabilityChart
            actual={data.cumulative_actual.map(() => 0)}
            ev={data.ev_gap_volatility}
            actualLabel="Baseline (0)"
            evLabel="EV Gap Std Dev ($)"
            ariaLabel="EV Volatility vs Sample Size"
          />
        </section>
        <section className="dh-panel">
          <PanelHeader title="Positional EV Drift (CO / BTN)" />
          <EvTrackingChart series={coBtnDrift} />
        </section>
      </div>
    </div>
  );
}

function DefenseVsRfiByPosition() {
  const [data, setData] = useState<MdaPreflopDefenseVsRfi | null>(null);
  const gameType = useMdaGameType();

  useEffect(() => {
    let cancelled = false;
    async function load() {
      const result = await invoke<MdaPreflopDefenseVsRfi>("get_mda_preflop_defense_vs_rfi", mdaInvokeArgs(gameType));
      if (!cancelled) setData(result);
    }
    load().catch(() => undefined);
    const interval = window.setInterval(() => load().catch(() => undefined), 4000);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, []);

  if (!data) {
    return <div className="dh-mda-loading">Se calculează din istoricul importat...</div>;
  }

  return (
    <div className="dh-mda-note-wrap">
      <p className="dh-mda-note">
        Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mână importată" : "mâini importate"}. Un bar cu
        „--" înseamnă că nu există încă nicio mână eligibilă pentru acel scenariu, nu că valoarea reală e zero.
      </p>
      <div className="dh-mda-grid-defense-rfi">
        <section className="dh-panel dh-mda-defense-win">
          <PanelHeader title="Positional Win Rates" />
          <AxisBarChart rows={data.positional_win_rates} unit="money" mode="diverging" />
        </section>
        <section className="dh-panel dh-mda-defense-fold">
          <PanelHeader title="Fold to Steal by Position (%)" />
          <AxisBarChart rows={data.fold_to_steal} unit="percent" mode="sequential" />
        </section>
        <section className="dh-panel dh-mda-defense-call">
          <PanelHeader title="Call Open by Position (%)" />
          <AxisBarChart rows={data.call_open} unit="percent" mode="sequential" color={CHART_RED} />
        </section>
        <section className="dh-panel dh-mda-defense-steal">
          <PanelHeader title="Steal Success Rate (Uncontested %)" />
          <AxisBarChart rows={data.steal_success} unit="percent" mode="sequential" color={CHART_GREEN} />
        </section>
        <section className="dh-panel dh-mda-defense-threebet">
          <PanelHeader title="3-Bet vs Open by Position (%)" />
          <AxisBarChart rows={data.three_bet_vs_open} unit="percent" mode="sequential" color={CHART_GREEN} />
        </section>
        <section className="dh-panel dh-mda-defense-late-steal">
          <PanelHeader title="Win Rate in BB vs Late Steal (BB/100)" />
          <AxisBarChart rows={data.win_rate_bb_vs_late_steal} unit="bb100" mode="diverging" />
        </section>
      </div>
    </div>
  );
}

function PositionalEvLeakage() {
  const [data, setData] = useState<MdaPositionalEvLeakage | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const result = await invoke<MdaPositionalEvLeakage>("get_mda_positional_ev_leakage", mdaInvokeArgs(gameType));
      setData(result);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // No auto-refresh interval here, unlike "Defense vs RFI by Position":
    // the All-In EV numbers need an exact-equity board enumeration per
    // qualifying all-in hand (see `two_hand_equity_percent` in
    // `mda.rs`), which is real work, not a cheap tally - re-running it
    // every few seconds regardless of whether new hands came in would
    // waste it for nothing. A manual refresh covers "I just imported more
    // hands."
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data) {
    return <div className="dh-mda-loading">Se calculează din istoricul importat...</div>;
  }

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mână importată" : "mâini importate"}, dintre
          care {data.all_in_hands_priced} au ajuns la un all-in cu ambele mâini cunoscute (singurul caz în care
          All-In EV se poate calcula exact). Un bar cu „--" înseamnă că nu există încă date pentru acel scenariu.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculează..." : "Reîmprospătează"}
        </button>
      </div>
      <div className="dh-mda-grid-3">
        <section className="dh-panel">
          <PanelHeader title="Positional Win Rates" />
          <AxisBarChart rows={data.positional_win_rates} unit="money" mode="diverging" />
        </section>
        <section className="dh-panel">
          <PanelHeader title="All-In Adjusted EV by Position (BB/100)" />
          <AxisBarChart
            rows={data.actual_bb_per_100}
            secondary={data.all_in_ev_bb_per_100}
            primaryLabel="Actual BB/100"
            secondaryLabel="All-In EV BB/100"
            unit="bb100"
            mode="diverging"
          />
        </section>
        <section className="dh-panel">
          <PanelHeader title="EV Tracking by Position (Cumulative)" />
          <EvTrackingChart series={data.ev_tracking} />
        </section>
        <section className="dh-panel">
          <PanelHeader title="Positional EV Over Volume (Cumulative EV Curve)" />
          <AxisBarChart rows={data.all_in_ev_bb_per_100} unit="bb100" mode="diverging" />
        </section>
        <section className="dh-panel dh-mda-panel-wide">
          <PanelHeader title="Positional VPIP / PFR by Seat" />
          <VpipPfrChart vpip={data.vpip_by_position} pfr={data.pfr_by_position} />
        </section>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Flop -> shared components
// ---------------------------------------------------------------------

const STAT_YELLOW = CHART_ORANGE;
const STAT_RED = CHART_MUTED_RED;
const STAT_GREEN = CHART_GREEN;
const SMALL_SAMPLE_THRESHOLD = 30;

// The reference prints a short colored headline plus a boxed sentence
// under every Flop panel. Here each one is a plain threshold rule over the
// panel's own real number - the sentence quotes that number and its sample
// size, a headline under `SMALL_SAMPLE_THRESHOLD` observations is prefixed
// "[Small sample]", and a panel with nothing to measure says so instead of
// inventing a verdict. The thresholds are display conventions (like the
// Elite/Strong/Weak bar tiers on Positional EV Realization), not a automatic advice
// or a tracker-standard benchmark.
type InsightTone = "red" | "yellow" | "green" | "blue" | "muted";

interface Insight {
  tone: InsightTone;
  title: string;
  text: string;
}

interface ThreeWayConfig {
  low: number;
  high: number;
  unit?: string;
  decimals?: number;
  tones: [InsightTone, InsightTone, InsightTone];
  titles: [string, string, string];
  texts: [string, string, string];
  /** Wording for a panel with nothing measured (else the generic one). */
  emptyTitle?: string;
  emptyText?: string;
}

function noDataInsight(): Insight {
  return {
    tone: "muted",
    title: "Not enough data yet",
    text: "No eligible hands for this scenario in the imported history yet, so there is nothing to read into.",
  };
}

// {v} = the value, {n} = its sample size, any other {key} = `extra[key]`.
function threeWayInsight(value: number | null, n: number, config: ThreeWayConfig, extra: Record<string, string> = {}): Insight {
  if (value === null || n <= 0) {
    return config.emptyTitle ? { tone: "blue", title: config.emptyTitle, text: config.emptyText ?? noDataInsight().text } : noDataInsight();
  }
  const slot = value < config.low ? 0 : value > config.high ? 2 : 1;
  const fill = (template: string) => {
    let out = template.replace(/\{v\}/g, `${value.toFixed(config.decimals ?? 1)}${config.unit ?? "%"}`).replace(/\{n\}/g, String(n));
    for (const [key, replacement] of Object.entries(extra)) out = out.replace(new RegExp(`\\{${key}\\}`, "g"), replacement);
    return out;
  };
  const provisional = n < SMALL_SAMPLE_THRESHOLD ? "[Small sample] " : "";
  return { tone: config.tones[slot], title: provisional + fill(config.titles[slot]), text: fill(config.texts[slot]) };
}

function InsightCallout({ insight }: { insight: Insight }) {
  return (
    <div className="dh-mda-insight">
      <div className={`dh-mda-insight-title dh-mda-insight-${insight.tone}`}>{insight.title}</div>
      <div className="dh-mda-insight-box">{insight.text}</div>
    </div>
  );
}

function InsightPanel({ title, insight, className, children }: { title: string; insight: Insight; className?: string; children: ReactNode }) {
  return (
    <section className={`dh-panel dh-mda-panel-rel${className ? ` ${className}` : ""}`}>
      <PanelHeader title={title} />
      {children}
      <InsightCallout insight={insight} />
    </section>
  );
}

function segmentPercent(row: MdaStatRow | undefined, key: string): number | null {
  return row?.segments.find((segment) => segment.key === key)?.percent ?? null;
}

// One percentage across several rows (IP + OOP), weighted by each row's own
// sample size - not an average of two averages.
function combinedPercent(rows: MdaStatRow[], key: string): { value: number | null; n: number } {
  let weighted = 0;
  let total = 0;
  for (const row of rows) {
    const percent = segmentPercent(row, key);
    if (percent !== null && row.sample_size > 0) {
      weighted += percent * row.sample_size;
      total += row.sample_size;
    }
  }
  return { value: total > 0 ? weighted / total : null, n: total };
}

function weightedBars(bars: MdaBar[]): { value: number | null; n: number } {
  let weighted = 0;
  let total = 0;
  for (const bar of bars) {
    if (bar.value !== null && bar.sample_size > 0) {
      weighted += bar.value * bar.sample_size;
      total += bar.sample_size;
    }
  }
  return { value: total > 0 ? weighted / total : null, n: total };
}

function weightedHeatmap(rows: MdaHeatmapRow[]): { value: number | null; n: number } {
  return weightedBars(rows.flatMap((row) => row.cells));
}

interface StatBarRowView {
  label: string;
  hasData: boolean;
  /** Width (0-100) of the colored lead segment of the bar. */
  barPercent: number | null;
  /** The value the Weak/Std/Over tag classifies. */
  tagValue: number | null;
  tagText?: string;
  barColor?: string;
  cells: { key: string; text: string }[];
}

function statCellText(percent: number | null): string {
  if (percent === null || Math.round(percent) === 0) return "-";
  return String(Math.round(percent));
}

// The bar's lead segment is the metric the row is *about*: the segment
// whose key equals the row label ("GU", "DC", "Δ"), else the first one.
function statRowViews(rows: MdaStatRow[]): StatBarRowView[] {
  return rows.map((row) => {
    const hasData = row.sample_size > 0 && row.segments.some((segment) => segment.percent !== null);
    const primaryIndex = Math.max(0, row.segments.findIndex((segment) => segment.key === row.label));
    const primary = row.segments[primaryIndex]?.percent ?? null;
    return {
      label: row.label,
      hasData,
      barPercent: primary,
      tagValue: primary,
      cells: row.segments.map((segment) => ({ key: segment.key, text: statCellText(segment.percent) })),
    };
  });
}

function archetypeMetricRows(rows: MdaHeatmapRow[], primaryKey: string, secondaryKey: string): StatBarRowView[] {
  return ARCHETYPES.map((archetype, index) => {
    const cells = rows.map((row) => row.cells[index]).filter((cell) => cell && cell.value !== null && cell.sample_size > 0);
    const sampleSize = cells.reduce((sum, cell) => sum + cell.sample_size, 0);
    const value = sampleSize > 0 ? cells.reduce((sum, cell) => sum + (cell.value ?? 0) * cell.sample_size, 0) / sampleSize : null;
    const inverse = value === null ? null : Math.max(0, 100 - value);
    return {
      label: archetype,
      hasData: value !== null,
      barPercent: value,
      tagValue: value,
      cells: [
        { key: primaryKey, text: statCellText(value) },
        { key: secondaryKey, text: statCellText(inverse) },
      ],
    };
  });
}

function aggressionFactorViews(af: MdaAggressionFactor): StatBarRowView[] {
  const total = af.aggressive_count + af.passive_count;
  const aggressive = total > 0 ? (af.aggressive_count * 100) / total : null;
  const passive = total > 0 ? (af.passive_count * 100) / total : null;
  return [
    {
      label: "AF",
      hasData: total > 0,
      barPercent: aggressive,
      tagValue: aggressive,
      cells: [
        { key: "A", text: statCellText(aggressive) },
        { key: "P", text: statCellText(passive) },
      ],
    },
  ];
}

function evRowViews(bar: MdaBar): StatBarRowView[] {
  const value = bar.value;
  const hasData = value !== null && bar.sample_size > 0;
  return [
    {
      label: "OOP",
      hasData,
      barPercent: value === null ? null : Math.min(100, Math.abs(value)),
      tagValue: value,
      tagText: hasData && value !== null ? `${value < 0 ? "Weak" : "Std"}:${Math.round(value)}` : undefined,
      barColor: value !== null && value < 0 ? STAT_RED : STAT_GREEN,
      cells: [
        { key: "EV", text: value === null ? "-" : value.toFixed(1) },
        { key: "N", text: String(bar.sample_size) },
      ],
    },
  ];
}

function evBarViews(bars: MdaBar[]): StatBarRowView[] {
  return bars.map((bar) => {
    const value = bar.value;
    const hasData = value !== null && bar.sample_size > 0;
    return {
      label: bar.label,
      hasData,
      barPercent: value === null ? null : Math.min(100, Math.abs(value)),
      tagValue: value,
      tagText: hasData && value !== null ? `${value < 0 ? "Weak" : "Std"}:${value.toFixed(1)}` : undefined,
      barColor: value !== null && value < 0 ? STAT_RED : STAT_GREEN,
      cells: [
        { key: "EV", text: value === null ? "-" : value.toFixed(1) },
        { key: "N", text: String(bar.sample_size) },
      ],
    };
  });
}

// Drivetracker's small "row label + bar + Weak:31 tag + lettered values"
// panels. The colored lead segment is the row's own metric; the rest of the
// bar is the neutral remainder. Weak/Std/Over is a display-only
// classification of that one number against `low`/`high`.
function StatBarPanel({
  rows,
  low = 25,
  high = 75,
  smallSample = false,
  labelWidth,
}: {
  rows: StatBarRowView[];
  low?: number;
  high?: number;
  smallSample?: boolean;
  labelWidth?: string;
}) {
  const headers = rows[0]?.cells.map((cell) => cell.key) ?? [];
  const gridStyle = { "--dh-cols": headers.length, ...(labelWidth ? { "--dh-label": labelWidth } : {}) } as CSSProperties;
  return (
    <div className="dh-mda-statbar" style={gridStyle}>
      <div className="dh-mda-statbar-head">
        <span className="dh-mda-statbar-badge-slot">{smallSample && <span className="dh-mda-small-sample">Small Sample</span>}</span>
        <div className="dh-mda-statbar-cols">
          {headers.map((header) => (
            <span key={header}>{header}</span>
          ))}
        </div>
      </div>
      {rows.map((row) => {
        const value = row.tagValue;
        const status = value === null ? null : value < low ? "Weak" : value > high ? "Over" : "Std";
        const tag = row.tagText ?? (row.hasData && value !== null ? `${status}:${Math.round(value)}` : "Weak:0");
        const width = row.hasData ? Math.max(0, Math.min(100, row.barPercent ?? 0)) : 0;
        const leadColor = row.barColor ?? (status === "Over" ? STAT_RED : STAT_GREEN);
        return (
          <div key={row.label} className="dh-mda-statbar-row">
            <span className="dh-mda-statbar-label">{row.label}</span>
            <div className={row.hasData ? "dh-mda-statbar-track" : "dh-mda-statbar-track dh-mda-statbar-track-empty"}>
              {width > 0 && <span className="dh-mda-statbar-lead" style={{ width: `${width}%`, background: leadColor }} />}
              <span className="dh-mda-statbar-rest" style={{ background: STAT_YELLOW }} />
            </div>
            <span className="dh-mda-statbar-tag">{tag}</span>
            <div className="dh-mda-statbar-cols">
              {row.cells.map((cell) => (
                <span key={cell.key}>{cell.text}</span>
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function LegendRing({ color, label }: { color: string; label: string }) {
  return (
    <span className="dh-mda-legend-item">
      <i className="dh-mda-legend-ring" style={{ borderColor: color }} />
      {label}
    </span>
  );
}

interface ChartSeries {
  name: string;
  color: string;
  values: number[];
}

// The reference's smooth filled-area line chart, for any number of series:
// full frame even with nothing measured (12 vertical gridlines with tick
// marks, both axis captions), ring legend in the panel's top-right corner,
// text sized 1:1 to its container. `mode="bb"` is a cumulative big-blind
// curve that starts at 0 (default -100..150 frame when empty);
// `mode="percent"` is a running percentage on a fixed 0-100 axis.
function SeriesChart({
  series,
  caption,
  height: fallbackHeight = 300,
  mode = "bb",
}: {
  series: ChartSeries[];
  caption: string;
  height?: number;
  mode?: "bb" | "percent";
}) {
  const [chartRef, chartSize] = useSvgChartSize(560, fallbackHeight, 320, 150);
  const width = chartSize.width;
  const height = chartSize.height;

  const pointCount = Math.max(0, ...series.map((s) => s.values.length));
  const hasData = pointCount > 0;
  const left = 40;
  const right = 8;
  const top = 8;
  const bottom = 22;
  const plotWidth = width - left - right;
  const plotHeight = height - top - bottom;
  const allValues = series.flatMap((s) => s.values);
  const axis =
    mode === "percent"
      ? { min: 0, max: 100, span: 100, ticks: [100, 75, 50, 25, 0] }
      : hasData
        ? moneyAxis(Math.min(0, ...allValues), Math.max(0, ...allValues))
        : { min: -100, max: 150, span: 250, ticks: [150, 100, 50, 0, -50, -100] };
  const total = mode === "percent" ? (hasData ? Math.max(1, pointCount - 1) : 4) : hasData ? pointCount : 48;
  const labelDivisions = mode === "percent" ? 4 : 3;
  const xFor = (index: number) => left + (index / total) * plotWidth;
  const yFor = (value: number) => top + ((axis.max - value) / axis.span) * plotHeight;
  const zeroY = yFor(0);
  const pointsOf = (values: number[]) => (mode === "percent" ? values : [0, ...values]).map((value, index) => ({ x: xFor(index), y: yFor(value) }));
  const areaPath = (values: number[]) => {
    const pts = pointsOf(values);
    return `${smoothLinePath(pts)} L ${pts[pts.length - 1].x} ${zeroY} L ${pts[0].x} ${zeroY} Z`;
  };
  const divisions = 12;
  const labelAt = (step: number) => String(Math.round((total * step) / labelDivisions) + (mode === "percent" ? 1 : 0));

  return (
    <>
      <div className="dh-mda-legend-corner">
        {series.map((s) => (
          <LegendRing key={s.name} color={s.color} label={s.name} />
        ))}
      </div>
      <div className="dh-mda-tallchart">
        <svg ref={chartRef} viewBox={`0 0 ${width} ${height}`} role="img" aria-label={caption}>
          {axis.ticks.map((tick) => (
            <line key={`h-${tick}`} x1={left} x2={width - right} y1={yFor(tick)} y2={yFor(tick)} stroke="rgba(135,153,190,0.14)" strokeWidth="1" />
          ))}
          {Array.from({ length: divisions + 1 }, (_, i) => {
            const x = left + (plotWidth * i) / divisions;
            return (
              <g key={`v-${i}`}>
                <line x1={x} x2={x} y1={top} y2={height - bottom} stroke="rgba(135,153,190,0.14)" strokeWidth="1" />
                <line x1={x} x2={x} y1={height - bottom} y2={height - bottom + 5} stroke="rgba(152,166,204,0.5)" strokeWidth="1" />
              </g>
            );
          })}
          <line x1={left} x2={left} y1={top} y2={height - bottom} stroke="rgba(152,166,204,0.5)" strokeWidth="1" />
          <line x1={left} x2={width - right} y1={height - bottom} y2={height - bottom} stroke="rgba(152,166,204,0.5)" strokeWidth="1" />
          {mode === "bb" && <line x1={left} x2={width - right} y1={zeroY} y2={zeroY} stroke="rgba(152,166,204,0.38)" strokeWidth="1" />}
          {hasData && series.map((s) => <path key={`${s.name}-area`} d={areaPath(s.values)} fill={s.color} opacity={0.2} stroke="none" />)}
          {hasData &&
            series.map((s) => (
              <path key={`${s.name}-line`} d={smoothLinePath(pointsOf(s.values))} fill="none" stroke={s.color} strokeWidth={2.4} strokeLinejoin="round" strokeLinecap="round" />
            ))}
          {axis.ticks.map((tick) => (
            <text key={`t-${tick}`} x={left - 8} y={yFor(tick) + 4} textAnchor="end" fill="#71809f" fontSize="11">
              {mode === "percent" ? String(tick) : formatAxisTick(tick, "bb100")}
            </text>
          ))}
          {Array.from({ length: labelDivisions + 1 }, (_, step) => (
            <text key={`x-${step}`} x={left + (plotWidth * step) / labelDivisions} y={height - 8} textAnchor="middle" fill="#71809f" fontSize="11">
              {labelAt(step)}
            </text>
          ))}
          <text x={left + 12} y={top + 16} fill="#71809f" fontSize="12">
            {caption}
          </text>
          <text x={width - right - 6} y={height - bottom - 10} textAnchor="end" fill="#71809f" fontSize="12">
            Hands Played
          </text>
          {!hasData && (
            <text x={left + plotWidth / 2} y={top + plotHeight / 2} textAnchor="middle" fill="#f4f6ff" fontSize="13">
              No data to plot
            </text>
          )}
        </svg>
      </div>
    </>
  );
}

// "EV BB/100 by Street": the three streets on one tall chart.
function EvByStreetChart({ flop, turn, river }: { flop: number[]; turn: number[]; river: number[] }) {
  return (
    <SeriesChart
      height={545}
      caption="Cumulatives BB"
      series={[
        { name: "Flop EV", color: CHART_GREEN, values: flop },
        { name: "Turn EV", color: CHART_ORANGE, values: turn },
        { name: "River EV", color: CHART_BLUE, values: river },
      ]}
    />
  );
}

function evByStreetInsight(ev: MdaEvByStreet): Insight {
  const { flop_bb_per_100: flop, turn_bb_per_100: turn, river_bb_per_100: river } = ev;
  if (flop === null || turn === null || river === null || ev.flop.length === 0) return noDataInsight();
  const fmt = (value: number) => `${value >= 0 ? "+" : ""}${value.toFixed(1)}`;
  const values: [string, number][] = [
    ["Flop", flop],
    ["Turn", turn],
    ["River", river],
  ];
  const dominant = [...values].sort((a, b) => Math.abs(b[1]) - Math.abs(a[1]))[0];
  const others = values.filter(([name]) => name !== dominant[0]).reduce((sum, [, value]) => sum + Math.abs(value), 0);
  const provisional = ev.flop.length < SMALL_SAMPLE_THRESHOLD ? "[Small sample] " : "";
  const summary = `Flop (${fmt(flop)}), turn (${fmt(turn)}), and river (${fmt(river)}) BB/100`;
  if (Math.abs(dominant[1]) >= 20 && Math.abs(dominant[1]) > 2 * others) {
    const losing = dominant[1] < 0;
    return {
      tone: losing ? "red" : "blue",
      title: `${provisional}${losing ? "Leak" : "Edge"} concentrated on the ${dominant[0].toLowerCase()}`,
      text: `${summary} show one street carrying most of the result. Review ${dominant[0].toLowerCase()} decisions first; the other streets are close to break-even.`,
    };
  }
  return {
    tone: "green",
    title: `${provisional}EV mixed across streets.`,
    text: `${summary} show no dominant single-street pattern. Default to standard post-flop fundamentals and apply spot-specific reads based on player tendencies in the hand history.`,
  };
}

// Every panel's Weak/Std/Over thresholds and headline/sentence templates
// live here, next to each other, so the classification a bar shows and the
// verdict printed under it can never disagree.
const CBET_FREQ: ThreeWayConfig = {
  low: 35,
  high: 70,
  tones: ["blue", "green", "yellow"],
  titles: ["Adjustment: C-Bets Are Used Sparingly", "C-bet frequency balanced", "Possible Exploit: Automatic C-Betting"],
  texts: [
    "Flop c-bet frequency is {v} across {n} opportunities, below typical ranges. Many flops are checked, so probing and floating can be profitable.",
    "Flop c-bet frequency is {v} across {n} opportunities, within typical ranges. No clear over- or under-use of continuation bets.",
    "Flop c-bet frequency is {v} across {n} opportunities, above typical ranges. Continuation bets look automatic; check-raising and floating wider can be profitable.",
  ],
};
const CBET_IP: ThreeWayConfig = {
  low: 35,
  high: 60,
  tones: ["yellow", "green", "blue"],
  titles: ["Possible Exploit: IP C-Bets Meet Resistance", "IP c-bet defence balanced", "Adjustment: Bluff More In Position"],
  texts: [
    "Opponents fold only {v} of the time to in-position c-bets ({n} responses). Defence is wide, so value-bet thinner and bluff less.",
    "Opponents fold {v} of the time to in-position c-bets ({n} responses), within typical ranges.",
    "Opponents fold {v} of the time to in-position c-bets ({n} responses). Over-folding invites more frequent c-bets from position.",
  ],
};
const CBET_OOP: ThreeWayConfig = {
  low: 35,
  high: 60,
  tones: ["yellow", "green", "blue"],
  titles: ["Possible Exploit: OOP C-Bets Meet Resistance", "OOP c-bet defence balanced", "Adjustment: Bluff More Out of Position"],
  texts: [
    "Opponents fold only {v} of the time to out-of-position c-bets ({n} responses). Defence is wide, so value-bet thinner and bluff less.",
    "Opponents fold {v} of the time to out-of-position c-bets ({n} responses), within typical ranges.",
    "Opponents fold {v} of the time to out-of-position c-bets ({n} responses). Over-folding invites more frequent c-bets out of position.",
  ],
};
const CHECK_AS_PFR: ThreeWayConfig = {
  low: 25,
  high: 55,
  tones: ["yellow", "green", "blue"],
  titles: ["Possible Exploit: Flops Are Rarely Checked", "Flop checking balanced", "Adjustment: Elevated Flop Checking"],
  texts: [
    "The preflop raiser checks the flop only {v} of the time ({n} flops). Betting almost every flop leaves checking ranges thin and easy to attack with check-raises.",
    "The preflop raiser checks the flop {v} of the time ({n} flops), within typical ranges.",
    "The preflop raiser checks the flop {v} of the time ({n} flops), above standard ranges. This may indicate more cautious continuation strategies.",
  ],
};
const CHECK_RAISE_DEFENCE: ThreeWayConfig = {
  low: 8,
  high: 25,
  tones: ["yellow", "green", "blue"],
  titles: ["Likely Exploit: Weak Check-Raise Defense", "Check-raise frequency balanced", "Adjustment: Frequent Check-Raising"],
  texts: [
    "After checking and facing a bet, the preflop raiser check-raises only {v} of the time ({n} spots), lower than standard ranges. Under-applied check-raises make defensive ranges predictable and bet-fold heavy.",
    "After checking and facing a bet, the preflop raiser check-raises {v} of the time ({n} spots), within typical ranges.",
    "After checking and facing a bet, the preflop raiser check-raises {v} of the time ({n} spots), above typical ranges. Expect resistance when betting into their checks.",
  ],
};
const CBET_SUCCESS: ThreeWayConfig = {
  low: 30,
  high: 60,
  tones: ["yellow", "green", "blue"],
  titles: ["Possible Exploit: Reduced C-Bet Fold Equity", "C-bet fold equity balanced", "Adjustment: C-Bets Generate Folds"],
  texts: [
    "Only {v} of c-bets take the pot immediately ({n} c-bets). If the trend persists over larger samples, defending flops more frequently may be profitable.",
    "{v} of c-bets take the pot immediately ({n} c-bets), within typical ranges.",
    "{v} of c-bets take the pot immediately ({n} c-bets). Flop c-bets are generating plenty of folds.",
  ],
};
const ONE_AND_DONE: ThreeWayConfig = {
  low: 30,
  high: 60,
  tones: ["yellow", "green", "blue"],
  titles: ["Likely Exploit: Weak Turn Follow-Through", "Turn follow-through balanced", "Adjustment: Strong Turn Follow-Through"],
  texts: [
    "Only {v} of called flop c-bets are followed by a turn bet ({n} spots). Floating the flop and attacking delayed turns can be profitable.",
    "{v} of called flop c-bets are followed by a turn bet ({n} spots), within typical ranges.",
    "{v} of called flop c-bets are followed by a turn bet ({n} spots). Expect frequent second barrels and tighten flop calling ranges.",
  ],
};
const FLOP_AF: ThreeWayConfig = {
  low: 1,
  high: 2.5,
  unit: "",
  decimals: 2,
  tones: ["blue", "green", "yellow"],
  titles: ["Adjustment: Passive Flop Play", "Flop aggression balanced", "Possible Exploit: Elevated Flop Aggression"],
  texts: [
    "Flop aggression factor ({v}) sits below normal ranges ({n} actions). Bets and raises are rare relative to calls, so continuation betting may go under-punished.",
    "Flop aggression factor ({v}) sits within normal ranges ({n} actions).",
    "Flop aggression factor ({v}) sits above normal ranges ({n} actions). If turn aggression stays limited, continuation betting may be over-automated; monitor for bluff-heavy lines.",
  ],
};
const TURN_BARREL: ThreeWayConfig = {
  low: 30,
  high: 60,
  tones: ["yellow", "green", "blue"],
  titles: ["Possible Exploit: Low Turn Continuation Frequency", "Turn continuation frequency balanced", "Adjustment: Strong Multi-Street Aggression"],
  texts: [
    "Turn barrel rate is {v} ({n} called c-bets). Turn continuation appears below typical levels. If the pattern persists, floating flops and attacking later streets may be profitable.",
    "Turn barrel rate ({v}, {n} called c-bets) appears consistent with balanced multi-street aggression relative to flop c-bet frequency. No structural one-and-done pattern detected. Default to standard post-flop fundamentals.",
    "Turn barrel rate is {v} ({n} called c-bets). Players keep firing on the turn; expect double barrels and tighten your flop calling range.",
  ],
};
const FLOP_CBET_SIMPLE: ThreeWayConfig = {
  ...CBET_FREQ,
  tones: ["yellow", "green", "yellow"],
  titles: ["Possible Exploit: Low Flop C-Bet Frequency", "Flop c-bet frequency balanced", "Possible Exploit: Automatic Flop C-Betting"],
};
const AGGRESSION_DELTA: ThreeWayConfig = {
  low: -15,
  high: 15,
  unit: " pts",
  tones: ["yellow", "blue", "yellow"],
  titles: ["Possible Exploit: Aggression Drops After the Flop", "Adjustment: Strong Multi-Street Aggression", "Possible Exploit: Aggression Escalates on the Turn"],
  texts: [
    "The turn barrel rate is {v} relative to the flop c-bet rate ({n} hands). Aggression fades on later streets; floating flops can be profitable.",
    "Aggression remains relatively consistent between flop and turn ({v}, {n} hands). Players appear willing to apply multi-street pressure.",
    "The turn barrel rate is {v} above the flop c-bet rate ({n} hands). Aggression escalates on the turn; expect more turn bluffs and tighten your flop floats.",
  ],
};
const DELAYED_CBET: ThreeWayConfig = {
  low: 8,
  high: 30,
  tones: ["blue", "green", "yellow"],
  titles: ["Adjustment: Delayed C-Bets Are Rare", "Delayed c-bet frequency balanced", "Possible Exploit: Frequent Delayed C-Bets"],
  texts: [
    "Delayed c-bet frequency is {v} ({n} checked-through flops). Flops that check through rarely get a turn bet, so stabbing at them can be profitable.",
    "Delayed c-bet frequency is {v} ({n} checked-through flops), within typical ranges.",
    "Delayed c-bet frequency is {v} ({n} checked-through flops). Checked-through flops often get a turn bet; expect delayed aggression and float accordingly.",
  ],
};
const TURN_GIVE_UP: ThreeWayConfig = {
  low: 30,
  high: 60,
  tones: ["blue", "green", "red"],
  titles: ["Adjustment: Low Turn Give-Up After Flop C-Bet", "Turn give-up balanced", "Exploit Detected: High Turn Give-Up After Flop C-Bet"],
  texts: [
    "Turn give-up rate is {v} after called flop c-bets ({n} hands) and flop c-bet success rate is {c}. Players usually continue on the turn, so floating alone is less profitable.",
    "Turn give-up rate is {v} after called flop c-bets ({n} hands) and flop c-bet success rate is {c}, within typical ranges.",
    "Turn give-up rate is {v} after called flop c-bets ({n} hands) and flop c-bet success rate is {c}. Players frequently abandon aggression after their flop continuation bets are called; floating the flop and attacking the turn should be profitable.",
  ],
};
const OOP_AGGRESSION: ThreeWayConfig = {
  low: 15,
  high: 40,
  tones: ["red", "green", "blue"],
  titles: ["Exploit detected: OOP players surrender too often", "OOP aggression balanced", "Adjustment: Aggressive OOP Play"],
  texts: [
    "OOP aggression is very low ({v} over {n} actions). The population applies little resistance on the flop, allowing IP players to realize equity too easily. OOP flop EV: {ev} bb/100.",
    "OOP aggression is {v} over {n} actions, within typical ranges.",
    "OOP aggression is {v} over {n} actions, above typical ranges. OOP players fight for the pot; expect check-raises and donk bets and adjust your c-betting range.",
  ],
};
const OOP_CHECK_FOLD: ThreeWayConfig = {
  low: 25,
  high: 55,
  tones: ["yellow", "green", "yellow"],
  titles: ["Possible exploit: inconsistent OOP defense", "OOP check-fold balanced", "Possible exploit: OOP over-folds after checking"],
  texts: [
    "Some players show unusually low check-fold frequencies ({v}, {n} spots). This may indicate over-defending or calling too wide in OOP situations.",
    "OOP players fold {v} of the time after checking into a bet ({n} spots), within typical ranges.",
    "OOP players fold {v} of the time after checking into a bet ({n} spots). Betting wide when they check should be profitable.",
  ],
};
const OOP_FOLD_TO_CBET: ThreeWayConfig = {
  low: 25,
  high: 60,
  tones: ["yellow", "green", "red"],
  titles: ["Possible exploit: inconsistent OOP defense vs c-bets", "OOP fold to c-bet balanced", "Exploit detected: OOP folds too often to c-bets"],
  texts: [
    "Some players show unusually low fold frequencies ({v}, {n} c-bets faced). This may indicate over-defending or continuing with poorly constructed ranges.",
    "OOP players fold {v} of the time to c-bets ({n} c-bets faced), within typical ranges.",
    "OOP players fold {v} of the time to c-bets ({n} c-bets faced). Frequent c-bets should print money against this pool.",
  ],
};
const OOP_CHECK_CALL: ThreeWayConfig = {
  low: 25,
  high: 60,
  tones: ["red", "green", "yellow"],
  titles: ["Exploit detected: OOP ranges collapsing on the flop", "OOP check-call balanced", "Possible exploit: OOP players call too much"],
  texts: [
    "OOP players are not defending enough through check-calling and are folding too frequently. Check-Call: {v} ({n} spots), Fold to C-Bet: {f}. This creates a structurally weak range that can be exploited with aggressive c-betting.",
    "OOP players check-call {v} of the time ({n} spots), within typical ranges.",
    "OOP players check-call {v} of the time ({n} spots). Value-bet thinner and cut bluffs against sticky OOP ranges.",
  ],
};
const OOP_CHECK_RAISE: ThreeWayConfig = {
  low: 8,
  high: 25,
  tones: ["yellow", "green", "blue"],
  titles: ["Likely exploit: insufficient OOP check-raise pressure", "OOP check-raise balanced", "Adjustment: Frequent OOP Check-Raising"],
  texts: [
    "The population check-raise frequency ({v}, {n} spots) is too low to effectively deter aggression. This allows in-position players to realize equity and apply pressure with wider ranges.",
    "OOP players check-raise {v} of the time ({n} spots), within typical ranges.",
    "OOP players check-raise {v} of the time ({n} spots). Expect resistance and respect check-raises more than usual.",
  ],
};
const OOP_DONK: ThreeWayConfig = {
  low: 5,
  high: 35,
  tones: ["green", "green", "yellow"],
  titles: ["Donk betting is rare", "Donk frequency balanced", "Over-Donking — Possible Miscalibration"],
  texts: [
    "OOP players lead into the preflop raiser only {v} of the time ({n} spots). Leads are rare, so a donk bet usually signals real strength.",
    "OOP players lead into the preflop raiser {v} of the time ({n} spots), within typical ranges.",
    "Donk bet frequency is {v} ({n} spots). Excessive donk betting without board-texture selectivity can inflate costs when opponents call or raise with strong ranges.",
  ],
};
const OOP_WWSF: ThreeWayConfig = {
  low: 35,
  high: 55,
  tones: ["yellow", "green", "blue"],
  titles: ["Likely exploit: weak OOP pot contesting", "OOP WWSF balanced", "Adjustment: OOP players win pots often"],
  texts: [
    "OOP players win only {v} of pots after seeing the flop ({n} hands). Low WWSF combined with passive tendencies creates opportunities to apply consistent pressure in position.",
    "OOP players win {v} of pots after seeing the flop ({n} hands), within typical ranges.",
    "OOP players win {v} of pots after seeing the flop ({n} hands). They contest pots effectively; pick spots for pressure carefully.",
  ],
};
const OOP_EV: ThreeWayConfig = {
  low: -20,
  high: 20,
  unit: " bb/100",
  tones: ["red", "green", "blue"],
  titles: ["Exploit detected: OOP play is losing significant EV", "OOP flop EV balanced", "Adjustment: OOP players profit on the flop"],
  texts: [
    "OOP players are losing substantial EV on the flop ({v} over {n} hands), driven by excessive folding or lack of resistance. This creates a strong opportunity to apply pressure in position. Fold to C-Bet: {f}.",
    "OOP flop EV is {v} over {n} hands, close to break-even.",
    "OOP flop EV is {v} over {n} hands. OOP players are profiting on the flop; be selective with in-position pressure.",
  ],
};
const HEAT_AGGRESSION: ThreeWayConfig = {
  low: 20,
  high: 55,
  tones: ["yellow", "green", "yellow"],
  titles: ["Possible exploit: passive flop play", "No aggression-based leak detected", "Possible exploit: over-aggressive flop play"],
  texts: [
    "Flop aggression is {v} over {n} actions, below typical ranges. Archetypes rarely bet or raise the flop, so pressure goes unanswered.",
    "No clear aggression-based leak detected. Flop aggression levels ({v} over {n} actions) do not show a meaningful imbalance across archetypes.",
    "Flop aggression is {v} over {n} actions, above typical ranges. Archetypes bet and raise the flop often; expect bluffs and widen your calling ranges.",
  ],
};
const HEAT_CALL: ThreeWayConfig = {
  low: 20,
  high: 65,
  tones: ["yellow", "green", "yellow"],
  titles: ["Possible exploit: under-defends vs c-bets", "No call vs c-bet leak detected", "Possible exploit: calls c-bets too wide"],
  texts: [
    "Archetypes call only {v} of c-bets ({n} responses). Defence is thin, so c-bet wide.",
    "No clear call vs c-bet leak detected. Defensive behavior versus c-bets ({v} calls over {n} responses) does not show a meaningful imbalance across archetypes.",
    "Archetypes call {v} of c-bets ({n} responses). Value-bet thinner and bluff less.",
  ],
};
const HEAT_FOLD: ThreeWayConfig = {
  low: 25,
  high: 60,
  tones: ["yellow", "green", "yellow"],
  titles: ["Possible exploit: under-folds to c-bets", "No fold vs c-bet leak detected", "Possible exploit: over-folds to c-bets"],
  texts: [
    "Archetypes fold only {v} to c-bets ({n} responses). Fold equity is low; favor value over bluffs.",
    "No clear fold vs c-bet leak detected. Flop fold behavior ({v} over {n} responses) does not show a meaningful imbalance across archetypes.",
    "Archetypes fold {v} to c-bets ({n} responses). Fold equity is high; c-bet wide.",
  ],
};
const HEAT_CHECK_RAISE: ThreeWayConfig = {
  low: 5,
  high: 30,
  tones: ["red", "green", "yellow"],
  titles: ["Exploit detected: check-raise strategy structurally broken", "Check-raise frequency balanced", "Possible exploit: over-check-raising"],
  texts: [
    "The population shows either extremely low check-raise frequency or highly unprofitable overuse. Check-raise frequency is {v} over {n} spots, a major structural imbalance in how pressure is applied on the flop.",
    "Check-raise frequency is {v} over {n} spots, within typical ranges across archetypes.",
    "Check-raise frequency is {v} over {n} spots. Check-raises are frequent; respect them less in bluff-heavy spots.",
  ],
};
const HEAT_CBET: ThreeWayConfig = {
  low: 35,
  high: 75,
  tones: ["yellow", "green", "yellow"],
  titles: ["Possible exploit: under c-betting", "No clear c-bet frequency leak detected", "Possible exploit: over c-betting"],
  texts: [
    "C-bet frequency is {v} over {n} opportunities, below typical ranges. Many flops are checked; probe and float.",
    "C-bet behavior ({v} over {n} opportunities) does not show a meaningful imbalance across archetypes.",
    "C-bet frequency is {v} over {n} opportunities, above typical ranges. C-bets look automatic; check-raise and float more.",
  ],
};

function heatmapEvInsight(rows: MdaHeatmapRow[], empty: Insight = noDataInsight()): Insight {
  const perArchetype = ARCHETYPES.map((archetype, index) => {
    const cells = rows.map((row) => row.cells[index]).filter((cell) => cell && cell.value !== null && cell.sample_size > 0);
    const n = cells.reduce((sum, cell) => sum + cell.sample_size, 0);
    const value = n > 0 ? cells.reduce((sum, cell) => sum + (cell.value ?? 0) * cell.sample_size, 0) / n : null;
    return { archetype, value, n };
  }).filter((entry): entry is { archetype: (typeof ARCHETYPES)[number]; value: number; n: number } => entry.value !== null);
  if (perArchetype.length === 0) return empty;
  const total = perArchetype.reduce((sum, entry) => sum + entry.n, 0);
  const provisional = total < SMALL_SAMPLE_THRESHOLD ? "[Small sample] " : "";
  const sorted = [...perArchetype].sort((a, b) => a.value - b.value);
  const worst = sorted[0];
  const best = sorted[sorted.length - 1];
  if (worst.value < -20) {
    return {
      tone: "red",
      title: `${provisional}Exploit detected: ${worst.archetype} flop EV collapse`,
      text: `${worst.archetype} players lose ${worst.value.toFixed(1)} bb/100 on the flop over ${worst.n} hands. Apply pressure against this player type.`,
    };
  }
  if (best.value - worst.value > 40) {
    return {
      tone: "yellow",
      title: `${provisional}Possible exploit: uneven flop EV across archetypes`,
      text: `Flop EV runs from ${worst.value.toFixed(1)} bb/100 (${worst.archetype}) to ${best.value.toFixed(1)} bb/100 (${best.archetype}) over ${total} hands. Target the weaker archetypes.`,
    };
  }
  return {
    tone: "green",
    title: `${provisional}Archetype flop EV relatively balanced`,
    text: "Flop profitability appears reasonably balanced across archetypes, with no major player type showing a clear structural EV collapse.",
  };
}

function smallSampleOf(rows: MdaStatRow[]): boolean {
  return rows.length === 0 || Math.min(...rows.map((row) => row.sample_size)) < SMALL_SAMPLE_THRESHOLD;
}

function FlopCbetFrequencyTab() {
  const [data, setData] = useState<MdaFlopCbetFrequency | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const result = await invoke<MdaFlopCbetFrequency>("get_mda_flop_cbet_frequency", mdaInvokeArgs(gameType));
      setData(result);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data) {
    return <div className="dh-mda-loading">Se calculează din istoricul importat...</div>;
  }

  const overall = weightedBars(data.cbet_frequency_by_archetype);
  const ipFold = segmentPercent(data.cbet_ip[0], "F");
  const oopFold = segmentPercent(data.cbet_oop[0], "F");
  const checkFreq = combinedPercent(data.check_frequency_as_pfr, "C");
  const checkRaise = segmentPercent(data.check_and_check_raise[0], "R");
  const success = segmentPercent(data.cbet_success_rate[0], "F");
  const barrel = combinedPercent(data.one_and_done, "B");
  const af = data.flop_aggression_factor;

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mână importată" : "mâini importate"}, dintre
          care {data.flop_hands_analyzed} au ajuns la un flop cu un preflop-raiser clar identificabil. IP/OOP se
          referă la poziția preflop-raiser-ului față de ceilalți jucători rămași, nu la fiecare adversar în parte.
          Verdictele de sub grafice sunt reguli simple de prag peste numerele de mai sus, nu analiză de automatic advice.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculează..." : "Reîmprospătează"}
        </button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-flop">
        <InsightPanel title="Flop C-Bet Frequency (Overall) %" className="dh-mda-panel-tall" insight={threeWayInsight(overall.value, overall.n, CBET_FREQ)}>
          <ArchetypeBarChart rows={data.cbet_frequency_by_archetype} height={520} />
        </InsightPanel>
        <InsightPanel title="Flop C-Bet IP" insight={threeWayInsight(ipFold, data.cbet_ip[0].sample_size, CBET_IP)}>
          <StatBarPanel rows={statRowViews(data.cbet_ip)} low={CBET_IP.low} high={CBET_IP.high} smallSample={smallSampleOf(data.cbet_ip)} />
        </InsightPanel>
        <InsightPanel title="Flop C-Bet OOP" insight={threeWayInsight(oopFold, data.cbet_oop[0].sample_size, CBET_OOP)}>
          <StatBarPanel rows={statRowViews(data.cbet_oop)} low={CBET_OOP.low} high={CBET_OOP.high} smallSample={smallSampleOf(data.cbet_oop)} />
        </InsightPanel>
        <InsightPanel title="Flop Check Frequency as PFR" insight={threeWayInsight(checkFreq.value, checkFreq.n, CHECK_AS_PFR)}>
          <StatBarPanel rows={statRowViews(data.check_frequency_as_pfr)} low={CHECK_AS_PFR.low} high={CHECK_AS_PFR.high} smallSample={smallSampleOf(data.check_frequency_as_pfr)} />
        </InsightPanel>
        <InsightPanel title="Flop Check & Check-Raise" insight={threeWayInsight(checkRaise, data.check_and_check_raise[0].sample_size, CHECK_RAISE_DEFENCE)}>
          <StatBarPanel rows={statRowViews(data.check_and_check_raise)} low={CHECK_RAISE_DEFENCE.low} high={CHECK_RAISE_DEFENCE.high} smallSample={smallSampleOf(data.check_and_check_raise)} />
        </InsightPanel>
        <InsightPanel title="C-Bet Success Rate" insight={threeWayInsight(success, data.cbet_success_rate[0].sample_size, CBET_SUCCESS)}>
          <StatBarPanel rows={statRowViews(data.cbet_success_rate)} low={CBET_SUCCESS.low} high={CBET_SUCCESS.high} smallSample={smallSampleOf(data.cbet_success_rate)} />
        </InsightPanel>
        <InsightPanel title="One-and-Done Indicator" insight={threeWayInsight(barrel.value, barrel.n, ONE_AND_DONE)}>
          <StatBarPanel rows={statRowViews(data.one_and_done)} low={ONE_AND_DONE.low} high={ONE_AND_DONE.high} smallSample={smallSampleOf(data.one_and_done)} />
        </InsightPanel>
        <InsightPanel
          title="Flop Aggression Factor"
          insight={threeWayInsight(af.value, af.aggressive_count + af.passive_count, FLOP_AF)}
        >
          <StatBarPanel rows={aggressionFactorViews(af)} low={20} high={60} smallSample={af.aggressive_count + af.passive_count < SMALL_SAMPLE_THRESHOLD} />
        </InsightPanel>
      </div>
    </div>
  );
}

function FlopToTurnContinuityTab() {
  const [data, setData] = useState<MdaFlopToTurnContinuity | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const result = await invoke<MdaFlopToTurnContinuity>("get_mda_flop_to_turn_continuity", mdaInvokeArgs(gameType));
      setData(result);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data) {
    return <div className="dh-mda-loading">Se calculează din istoricul importat...</div>;
  }

  const cb = data.flop_cbet_frequency[0];
  const tb = data.turn_barrel_frequency[0];
  const delta = data.flop_to_turn_aggression_delta[0];
  const dc = data.delayed_cbet_frequency[0];
  const gu = data.cbet_success_vs_turn_give_up[0];
  const guSuccess = segmentPercent(gu, "C");

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mână importată" : "mâini importate"}, dintre
          care {data.flop_hands_analyzed} au un preflop-raiser clar identificabil pe flop. „EV BB/100 by Street"
          atribuie rezultatul net al fiecărei mâini lui Hero străzii pe care s-a încheiat (flop/turn/river), în big
          blinds - nu este o defalcare reală a EV-ului pe fiecare stradă. Verdictele de sub grafice sunt reguli
          simple de prag peste numerele afișate, nu analiză de automatic advice.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculează..." : "Reîmprospătează"}
        </button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-flop">
        <InsightPanel title="EV BB/100 by Street" className="dh-mda-panel-wide dh-mda-panel-tall" insight={evByStreetInsight(data.ev_by_street)}>
          <EvByStreetChart flop={data.ev_by_street.flop} turn={data.ev_by_street.turn} river={data.ev_by_street.river} />
        </InsightPanel>
        <InsightPanel title="Flop C-Bet Frequency" insight={threeWayInsight(segmentPercent(cb, "CB"), cb.sample_size, FLOP_CBET_SIMPLE)}>
          <StatBarPanel rows={statRowViews(data.flop_cbet_frequency)} low={FLOP_CBET_SIMPLE.low} high={FLOP_CBET_SIMPLE.high} smallSample={smallSampleOf(data.flop_cbet_frequency)} />
        </InsightPanel>
        <InsightPanel title="Turn Barrel Frequency" insight={threeWayInsight(segmentPercent(tb, "TB"), tb.sample_size, TURN_BARREL)}>
          <StatBarPanel rows={statRowViews(data.turn_barrel_frequency)} low={TURN_BARREL.low} high={TURN_BARREL.high} smallSample={smallSampleOf(data.turn_barrel_frequency)} />
        </InsightPanel>
        <InsightPanel title="Flop → Turn Aggression Delta" insight={threeWayInsight(segmentPercent(delta, "Δ"), delta.sample_size, AGGRESSION_DELTA)}>
          <StatBarPanel rows={statRowViews(data.flop_to_turn_aggression_delta)} low={AGGRESSION_DELTA.low} high={AGGRESSION_DELTA.high} smallSample={smallSampleOf(data.flop_to_turn_aggression_delta)} />
        </InsightPanel>
        <InsightPanel title="Delayed C-Bet Frequency" insight={threeWayInsight(segmentPercent(dc, "DC"), dc.sample_size, DELAYED_CBET)}>
          <StatBarPanel rows={statRowViews(data.delayed_cbet_frequency)} low={DELAYED_CBET.low} high={DELAYED_CBET.high} smallSample={smallSampleOf(data.delayed_cbet_frequency)} />
        </InsightPanel>
        <InsightPanel
          title="C-Bet Success vs Turn Give-Up"
          insight={threeWayInsight(segmentPercent(gu, "GU"), gu.sample_size, TURN_GIVE_UP, { c: guSuccess === null ? "--" : `${guSuccess.toFixed(1)}%` })}
        >
          <StatBarPanel rows={statRowViews(data.cbet_success_vs_turn_give_up)} low={TURN_GIVE_UP.low} high={TURN_GIVE_UP.high} smallSample={smallSampleOf(data.cbet_success_vs_turn_give_up)} />
        </InsightPanel>
      </div>
    </div>
  );
}

function ArchetypeFlopEdgeTab() {
  const [data, setData] = useState<MdaArchetypeFlopEdge | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const result = await invoke<MdaArchetypeFlopEdge>("get_mda_archetype_flop_edge", mdaInvokeArgs(gameType));
      setData(result);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data) {
    return <div className="dh-mda-loading">Se calculează din istoricul importat...</div>;
  }

  const aggression = weightedHeatmap(data.aggression_percent_by_archetype);
  const call = weightedHeatmap(data.call_vs_cbet_by_archetype);
  const fold = weightedHeatmap(data.fold_to_cbet_by_archetype);
  const checkRaise = weightedHeatmap(data.check_raise_frequency_by_archetype);
  const cbet = weightedHeatmap(data.cbet_frequency_by_archetype);

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mână importată" : "mâini importate"}. Doar{" "}
          {data.classified_opponents} {data.classified_opponents === 1 ? "adversar a strâns" : "adversari au strâns"}{" "}
          destule mâini (minimum 15) pentru a primi un arhetip - Hero nu este numărat; celulele fără date arată „--".
          Verdictele de sub grafice sunt reguli simple de prag peste numerele afișate, nu analiză de automatic advice.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculează..." : "Reîmprospătează"}
        </button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-flop">
        <InsightPanel title="Flop EV BB/100 by Archetype" insight={heatmapEvInsight(data.ev_bb_per_100_by_archetype)}>
          <ColdCallHeatmap rows={data.ev_bb_per_100_by_archetype} unit="bb100" variant="flop" />
        </InsightPanel>
        <InsightPanel title="Flop Aggression % by Archetype" insight={threeWayInsight(aggression.value, aggression.n, HEAT_AGGRESSION)}>
          <ColdCallHeatmap rows={data.aggression_percent_by_archetype} variant="flop" />
        </InsightPanel>
        <InsightPanel title="Flop Call vs C-Bet % by Archetype" insight={threeWayInsight(call.value, call.n, HEAT_CALL)}>
          <ColdCallHeatmap rows={data.call_vs_cbet_by_archetype} variant="flop" />
        </InsightPanel>
        <InsightPanel title="Flop Fold to C-Bet % by Archetype" insight={threeWayInsight(fold.value, fold.n, HEAT_FOLD)}>
          <ColdCallHeatmap rows={data.fold_to_cbet_by_archetype} variant="flop" />
        </InsightPanel>
        <InsightPanel title="Flop Check-Raise Frequency by Archetype" insight={threeWayInsight(checkRaise.value, checkRaise.n, HEAT_CHECK_RAISE)}>
          <ColdCallHeatmap rows={data.check_raise_frequency_by_archetype} variant="flop" />
        </InsightPanel>
        <InsightPanel title="Flop C-Bet Frequency by Archetype" insight={threeWayInsight(cbet.value, cbet.n, HEAT_CBET)}>
          <ColdCallHeatmap rows={data.cbet_frequency_by_archetype} variant="flop" />
        </InsightPanel>
      </div>
    </div>
  );
}

function FlopOopResistanceTab() {
  const [data, setData] = useState<MdaFlopOopResistance | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const result = await invoke<MdaFlopOopResistance>("get_mda_flop_oop_resistance", mdaInvokeArgs(gameType));
      setData(result);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data) {
    return <div className="dh-mda-loading">Se calculează din istoricul importat...</div>;
  }

  const aggr = data.oop_flop_aggression[0];
  const checkFold = data.oop_check_fold[0];
  const foldCbet = data.oop_fold_to_cbet[0];
  const checkCall = data.oop_check_call[0];
  const checkRaise = data.oop_check_raise[0];
  const donk = data.oop_donk_bet[0];
  const wwsf = data.oop_wwsf[0];
  const ev = data.oop_flop_ev;
  const evText = ev.value === null ? "--" : ev.value.toFixed(1);
  const foldText = segmentPercent(foldCbet, "F") === null ? "--" : `${segmentPercent(foldCbet, "F")!.toFixed(1)}%`;
  const evHasData = ev.value !== null && ev.sample_size > 0;

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mână importată" : "mâini importate"}, dintre
          care {data.flop_hands_analyzed} au ajuns la un flop. Toți jucătorii sunt numărați (nu doar Hero); un jucător
          e OOP când cel puțin un alt jucător rămas acționează după el pe flop. Verdictele de sub grafice sunt
          reguli simple de prag peste numerele afișate, nu analiză de automatic advice.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculează..." : "Reîmprospătează"}
        </button>
      </div>
      <div className="dh-mda-grid-even2 dh-mda-grid-oop">
        <InsightPanel title="OOP Flop Aggression %" insight={threeWayInsight(segmentPercent(aggr, "A"), aggr.sample_size, OOP_AGGRESSION, { ev: evText })}>
          <StatBarPanel rows={statRowViews(data.oop_flop_aggression)} low={OOP_AGGRESSION.low} high={OOP_AGGRESSION.high} smallSample={smallSampleOf(data.oop_flop_aggression)} />
        </InsightPanel>
        <InsightPanel title="OOP Check-Fold % (Flop)" insight={threeWayInsight(segmentPercent(checkFold, "CF"), checkFold.sample_size, OOP_CHECK_FOLD)}>
          <StatBarPanel rows={statRowViews(data.oop_check_fold)} low={OOP_CHECK_FOLD.low} high={OOP_CHECK_FOLD.high} smallSample={smallSampleOf(data.oop_check_fold)} />
        </InsightPanel>
        <InsightPanel title="OOP Fold to C-Bet %" insight={threeWayInsight(segmentPercent(foldCbet, "F"), foldCbet.sample_size, OOP_FOLD_TO_CBET)}>
          <StatBarPanel rows={statRowViews(data.oop_fold_to_cbet)} low={OOP_FOLD_TO_CBET.low} high={OOP_FOLD_TO_CBET.high} smallSample={smallSampleOf(data.oop_fold_to_cbet)} />
        </InsightPanel>
        <InsightPanel title="OOP Check-Call % (Flop)" insight={threeWayInsight(segmentPercent(checkCall, "CC"), checkCall.sample_size, OOP_CHECK_CALL, { f: foldText })}>
          <StatBarPanel rows={statRowViews(data.oop_check_call)} low={OOP_CHECK_CALL.low} high={OOP_CHECK_CALL.high} smallSample={smallSampleOf(data.oop_check_call)} />
        </InsightPanel>
        <InsightPanel title="OOP Check-Raise % (Flop)" insight={threeWayInsight(segmentPercent(checkRaise, "XR"), checkRaise.sample_size, OOP_CHECK_RAISE)}>
          <StatBarPanel rows={statRowViews(data.oop_check_raise)} low={OOP_CHECK_RAISE.low} high={OOP_CHECK_RAISE.high} smallSample={smallSampleOf(data.oop_check_raise)} />
        </InsightPanel>
        <InsightPanel title="OOP Donk Bet % (Lead into PFR)" insight={threeWayInsight(segmentPercent(donk, "D"), donk.sample_size, OOP_DONK)}>
          <StatBarPanel rows={statRowViews(data.oop_donk_bet)} low={OOP_DONK.low} high={OOP_DONK.high} smallSample={smallSampleOf(data.oop_donk_bet)} />
        </InsightPanel>
        <InsightPanel title="OOP WWSF % (Won When Saw Flop)" insight={threeWayInsight(segmentPercent(wwsf, "W"), wwsf.sample_size, OOP_WWSF)}>
          <StatBarPanel rows={statRowViews(data.oop_wwsf)} low={OOP_WWSF.low} high={OOP_WWSF.high} smallSample={smallSampleOf(data.oop_wwsf)} />
        </InsightPanel>
        <InsightPanel title="OOP Flop EV (BB/100)" insight={threeWayInsight(evHasData ? ev.value : null, ev.sample_size, OOP_EV, { f: foldText })}>
          <StatBarPanel rows={evRowViews(ev)} smallSample={ev.sample_size < SMALL_SAMPLE_THRESHOLD} />
        </InsightPanel>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Flop -> Flop Aggression Efficiency / Flop Over-calling / Post-Flop EV
// Continuity
// ---------------------------------------------------------------------

// Horizontal bars over the 8 player types, Nutball at the top down to Nit
// (the reference's order) on a fixed-frame axis: 0-10 when sequential, a
// symmetric -10..10 when diverging, rescaled once real values need more.
// A measured 0 still prints its "0.0"; a player type with no eligible hands
// prints "--" at the axis instead.
function ArchetypeHBarChart({
  rows,
  mode,
  format,
  height: fallbackHeight = 372,
}: {
  rows: MdaBar[];
  mode: "sequential" | "diverging";
  format: (value: number) => string;
  height?: number;
}) {
  const [chartRef, chartSize] = useSvgChartSize(420, fallbackHeight, 260, 150);
  const width = chartSize.width;
  const height = chartSize.height;

  const ordered = [...rows].reverse();
  const left = 88;
  const right = 28;
  const top = 4;
  const bottom = 22;
  const plotWidth = width - left - right;
  const rowHeight = (height - top - bottom) / Math.max(1, ordered.length);
  const values = rows.map((row) => row.value).filter((value): value is number => value !== null);

  let axisMin = 0;
  let axisMax = 10;
  let ticks: number[] = [];
  if (mode === "sequential") {
    const peak = Math.max(0, ...values);
    const step = peak > 0 ? niceStep(peak / 10) : 1;
    axisMax = step * 10;
    ticks = Array.from({ length: 11 }, (_, i) => step * i);
  } else {
    const peak = Math.max(0, ...values.map((value) => Math.abs(value)));
    const step = peak > 0 ? niceStep(peak / 2) : 5;
    axisMin = -step * 2;
    axisMax = step * 2;
    ticks = [-step * 2, -step, 0, step, step * 2];
  }
  const xFor = (value: number) => left + ((value - axisMin) / (axisMax - axisMin)) * plotWidth;
  const zeroX = xFor(0);
  const tickText = (value: number) => String(Number(value.toFixed(2)));

  return (
    <div className="dh-mda-hbar">
      <svg ref={chartRef} viewBox={`0 0 ${width} ${height}`} role="img" aria-label="Player type chart">
        {ticks.map((tick) => (
          <g key={tick}>
            <line x1={xFor(tick)} x2={xFor(tick)} y1={top} y2={height - bottom} stroke="rgba(135,153,190,0.22)" strokeWidth="1" strokeDasharray="3,3" />
            <text x={xFor(tick)} y={height - 8} textAnchor="middle" fill="#71809f" fontSize="11">
              {tickText(tick)}
            </text>
          </g>
        ))}
        <line x1={left} x2={left} y1={top} y2={height - bottom} stroke="rgba(152,166,204,0.5)" strokeWidth="1" />
        <line x1={left} x2={width - right} y1={height - bottom} y2={height - bottom} stroke="rgba(152,166,204,0.5)" strokeWidth="1" />
        <line x1={zeroX} x2={zeroX} y1={top} y2={height - bottom} stroke="rgba(160,178,214,0.7)" strokeWidth="2" />
        {ordered.map((row, index) => {
          const centerY = top + rowHeight * (index + 0.5);
          const value = row.value;
          return (
            <g key={row.label}>
              <text x={left - 8} y={centerY + 4} textAnchor="end" fill="#8b98b8" fontSize="12">
                {row.label}
              </text>
              {value === null ? (
                <text x={zeroX + 5} y={centerY + 4} fill="#71809f" fontSize="12">
                  --
                </text>
              ) : (
                <>
                  {value !== 0 && (
                    <rect
                      x={Math.min(zeroX, xFor(value))}
                      y={centerY - 7}
                      width={Math.abs(xFor(value) - zeroX)}
                      height={14}
                      rx={2}
                      fill={value >= 0 ? CHART_BLUE : CHART_MUTED_RED}
                    >
                      <title>{`${row.label}: ${format(value)} (${barTitle(row)})`}</title>
                    </rect>
                  )}
                  <text
                    x={value >= 0 ? Math.max(zeroX, xFor(value)) + 5 : Math.max(Math.min(zeroX, xFor(value)) - 5, left + format(value).length * 7 + 6)}
                    y={centerY + 4}
                    textAnchor={value >= 0 ? "start" : "end"}
                    fill="#f4f6ff"
                    fontSize="12"
                  >
                    {format(value)}
                  </text>
                </>
              )}
            </g>
          );
        })}
      </svg>
    </div>
  );
}

// One small icon per player type - the reference draws its own; these are
// the closest built-in glyphs.
const ARCHETYPE_ICONS: Record<string, string> = {
  Nit: "\u{1F48E}",
  Fish: "\u{1F41F}",
  "Standard Reg": "\u{1F3AF}",
  "Tight Reg": "\u{1F42D}",
  "Bad LAG": "\u{1F41E}",
  "Tricky LAG": "\u{1F680}",
  Whale: "\u{1F433}",
  Nutball: "\u{1F330}",
};

function ArchetypeTable({ rows, metricLabel }: { rows: MdaArchetypeTableRow[]; metricLabel: string }) {
  return (
    <div className="dh-mda-archtable-wrap">
      <table className="dh-mda-archtable">
        <thead>
          <tr>
            <th>Player Type</th>
            <th>Hands</th>
            <th>Share (%)</th>
            <th>{metricLabel}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.archetype}>
              <td>
                <span className="dh-mda-archicon">{ARCHETYPE_ICONS[row.archetype] ?? ""}</span>
                {row.archetype}
              </td>
              <td>{row.hands}</td>
              <td>{row.share_percent.toFixed(2)}%</td>
              <td>{row.value === null ? "--" : row.value.toFixed(2)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function weightedTable(rows: MdaArchetypeTableRow[]): { value: number | null; n: number } {
  let weighted = 0;
  let total = 0;
  for (const row of rows) {
    if (row.value !== null && row.hands > 0) {
      weighted += row.value * row.hands;
      total += row.hands;
    }
  }
  return { value: total > 0 ? weighted / total : null, n: total };
}

const EMPTY_PATTERN = "does not present a clear or reliable pattern. Additional data is required to determine exploitability.";

const EFF_AGGRESSION: ThreeWayConfig = {
  low: 20,
  high: 55,
  tones: ["yellow", "green", "yellow"],
  titles: ["Possible exploit: passive player pool on the flop", "Flop aggression balanced", "Possible exploit: over-aggressive player pool"],
  texts: [
    "Player types bet or raise only {v} of their flop actions ({n} actions). Aggression is rare, so bets and raises carry real strength.",
    "Player types bet or raise {v} of their flop actions ({n} actions), within typical ranges.",
    "Player types bet or raise {v} of their flop actions ({n} actions). Expect frequent bluffs and widen your calling ranges.",
  ],
  emptyTitle: "Not Enough Data on Flop Aggression Efficiency",
  emptyText: "The available data does not provide a reliable relationship between aggression and profitability. Wait for a larger sample.",
};
const EFF_EV: ThreeWayConfig = {
  low: -20,
  high: 20,
  unit: " bb/100",
  tones: ["red", "green", "blue"],
  titles: ["Exploit detected: player types losing flop EV", "Flop EV balanced", "Adjustment: player types profiting on the flop"],
  texts: [
    "Player types lose {v} on the flop ({n} hands). Losing flop EV points to leaks you can apply pressure against.",
    "Flop EV is {v} across player types ({n} hands), close to break-even.",
    "Player types win {v} on the flop ({n} hands). They are profiting on the flop; be selective with pressure.",
  ],
  emptyTitle: "Insufficient signal: Flop EV inconclusive",
  emptyText: "Flop EV does not present a clear or reliable pattern. Sample size or signal strength is insufficient to determine a consistent exploit.",
};
const EFF_RATIO: ThreeWayConfig = {
  low: -1,
  high: 1,
  unit: " bb/100 per pt",
  decimals: 2,
  tones: ["yellow", "green", "blue"],
  titles: ["Possible exploit: aggression is costing EV", "Aggression and EV roughly aligned", "Adjustment: aggression is paying off"],
  texts: [
    "Each point of flop aggression comes with {v} ({n} hands). Aggression is not converting into profit for these player types.",
    "Each point of flop aggression comes with {v} ({n} hands), roughly aligned with results.",
    "Each point of flop aggression comes with {v} ({n} hands). Aggression is paying off for these player types.",
  ],
  emptyTitle: "Insufficient signal: Aggression-to-EV relationship unclear",
  emptyText: "The relationship between aggression and profitability is inconclusive. Additional data is required to determine whether aggression is effective.",
};
const EFF_CBET: ThreeWayConfig = {
  ...CBET_FREQ,
  emptyTitle: "Insufficient signal: C-bet efficiency unclear",
  emptyText: `Flop c-bet frequency ${EMPTY_PATTERN}`,
};
const EFF_CHECK_RAISE: ThreeWayConfig = {
  ...CHECK_RAISE_DEFENCE,
  emptyTitle: "Insufficient signal: Check-raise efficiency unclear",
  emptyText: `Check-raise frequency ${EMPTY_PATTERN}`,
};
const EFF_WWSF: ThreeWayConfig = {
  ...OOP_WWSF,
  emptyTitle: "Insufficient signal: WWSF inconclusive",
  emptyText: `WWSF ${EMPTY_PATTERN}`,
};
const EFF_FOLD_TO_RAISE: ThreeWayConfig = {
  low: 30,
  high: 65,
  tones: ["yellow", "green", "red"],
  titles: ["Possible exploit: bettors rarely fold to raises", "Fold to raise balanced", "Exploit detected: bettors fold too often to raises"],
  texts: [
    "Bettors fold only {v} when raised on the flop ({n} spots). Bluff-raises get little fold equity; raise for value.",
    "Bettors fold {v} when raised on the flop ({n} spots), within typical ranges.",
    "Bettors fold {v} when raised on the flop ({n} spots). Raising their bets should be profitable.",
  ],
  emptyTitle: "Insufficient signal: Defense vs raises unclear",
  emptyText: `Fold to raise ${EMPTY_PATTERN}`,
};
const CALL_CBET: ThreeWayConfig = {
  ...HEAT_CALL,
  emptyTitle: "Insufficient signal: Flop calling inconclusive",
  emptyText: `Flop calling frequency ${EMPTY_PATTERN}`,
};
const WTSD: ThreeWayConfig = {
  low: 20,
  high: 35,
  tones: ["yellow", "green", "yellow"],
  titles: ["Possible exploit: players rarely reach showdown", "WTSD balanced", "Possible exploit: players over-call to showdown"],
  texts: [
    "Only {v} of players who see the flop reach showdown ({n} hands). They fold too often; bluff more.",
    "{v} of players who see the flop reach showdown ({n} hands), within typical ranges.",
    "{v} of players who see the flop reach showdown ({n} hands). They over-call; value-bet thinner and bluff less.",
  ],
  emptyTitle: "Insufficient signal: WTSD inconclusive",
  emptyText: `WTSD ${EMPTY_PATTERN}`,
};
const WSD: ThreeWayConfig = {
  low: 40,
  high: 60,
  tones: ["red", "green", "blue"],
  titles: ["Exploit detected: showdown losses", "W$SD balanced", "Adjustment: strong showdown results"],
  texts: [
    "Players win only {v} of their showdowns ({n} showdowns). They reach showdown with weak holdings; call them down lighter.",
    "Players win {v} of their showdowns ({n} showdowns), within typical ranges.",
    "Players win {v} of their showdowns ({n} showdowns). Their showdown ranges are strong; respect their bets.",
  ],
  emptyTitle: "Insufficient signal: Showdown data inconclusive",
  emptyText: `Showdown metrics do ${EMPTY_PATTERN.replace("does ", "")}`,
};
const FOLD_TO_CBET_ARCH: ThreeWayConfig = {
  ...HEAT_FOLD,
  emptyTitle: "Insufficient signal: Flop folding inconclusive",
  emptyText: `Fold to c-bet ${EMPTY_PATTERN}`,
};
const TURN_FOLD: ThreeWayConfig = {
  low: 15,
  high: 45,
  tones: ["yellow", "green", "red"],
  titles: ["Possible exploit: flop callers rarely fold the turn", "Turn fold after flop call balanced", "Exploit detected: flop callers fold the turn too often"],
  texts: [
    "Only {v} of flop callers fold the turn ({n} spots). They keep calling; value-bet turns thinner.",
    "{v} of flop callers fold the turn ({n} spots), within typical ranges.",
    "{v} of flop callers fold the turn ({n} spots). A turn barrel after a flop call should be profitable.",
  ],
  emptyTitle: "Insufficient signal: Turn behavior inconclusive",
  emptyText: `Turn fold behavior ${EMPTY_PATTERN}`,
};
const CHECK_CALL_OOP: ThreeWayConfig = {
  low: 25,
  high: 60,
  tones: ["red", "green", "yellow"],
  titles: ["Exploit detected: OOP defense collapsing", "OOP check-call balanced", "Possible exploit: OOP players call too much"],
  texts: [
    "OOP players check-call only {v} of the time ({n} spots). They fold too often against bets; bet wide.",
    "OOP players check-call {v} of the time ({n} spots), within typical ranges.",
    "OOP players check-call {v} of the time ({n} spots). Value-bet thinner and cut bluffs against sticky OOP ranges.",
  ],
  emptyTitle: "Insufficient signal: OOP defense inconclusive",
  emptyText: `Check-call frequency ${EMPTY_PATTERN}`,
};
const PF_CBET: ThreeWayConfig = {
  ...CBET_FREQ,
  emptyTitle: "Insufficient signal: C-bet efficiency unclear",
  emptyText: `C-bet frequency ${EMPTY_PATTERN}`,
};
const PF_BARREL: ThreeWayConfig = {
  ...TURN_BARREL,
  emptyTitle: "Insufficient signal: Turn barrel inconclusive",
  emptyText: `Turn barrel frequency ${EMPTY_PATTERN}`,
};
const PF_EV_DELTA: ThreeWayConfig = {
  low: -20,
  high: 20,
  unit: " bb/100",
  tones: ["yellow", "blue", "green"],
  titles: ["Possible exploit: EV drops from flop to turn", "Flop-to-turn EV stable", "Turn EV outperforms flop EV"],
  texts: [
    "Turn EV runs {v} versus the flop ({n} postflop hands). Results fade on later streets; review turn decisions.",
    "Turn EV runs {v} versus the flop ({n} postflop hands), a stable pattern across streets.",
    "Turn EV runs {v} versus the flop ({n} postflop hands). Later streets are outperforming the flop.",
  ],
  emptyTitle: "Insufficient signal: EV delta inconclusive",
  emptyText: `EV change from flop to turn ${EMPTY_PATTERN}`,
};
const PF_FLOP_EV: ThreeWayConfig = {
  low: -20,
  high: 20,
  unit: " bb/100",
  tones: ["red", "blue", "blue"],
  titles: ["Exploit detected: Flop EV leak", "Flop EV stable", "Edge on the flop"],
  texts: [
    "Flop EV is {v} over {n} postflop hands. The flop is where results are being lost; review flop decisions first.",
    "Flop EV is {v} over {n} postflop hands, close to break-even.",
    "Flop EV is {v} over {n} postflop hands. The flop is a source of profit.",
  ],
  emptyTitle: "Insufficient signal: EV continuity inconclusive",
  emptyText: "EV patterns across streets do not present a clear or reliable trend. Additional data is required to determine exploitability.",
};

// Turn/River are read against the flop: matching or beating it is "stable".
function streetVsFlopInsight(street: "Turn" | "River", value: number | null, flop: number | null, n: number): Insight {
  if (value === null || n <= 0) {
    return {
      tone: "blue",
      title: `Insufficient signal: ${street} EV inconclusive`,
      text: `${street} EV ${EMPTY_PATTERN}`,
    };
  }
  const provisional = n < SMALL_SAMPLE_THRESHOLD ? "[Small sample] " : "";
  if (flop === null || value >= flop) {
    return {
      tone: "green",
      title: `${provisional}No exploit edge: ${street} play is stable`,
      text: `${street} EV aligns with or exceeds flop performance, indicating consistent and effective play. ${street} EV: ${value.toFixed(2)} bb/100.`,
    };
  }
  return {
    tone: "yellow",
    title: `${provisional}Possible exploit: ${street} EV trails the flop`,
    text: `${street} EV (${value.toFixed(2)} bb/100) trails flop EV (${flop.toFixed(2)} bb/100) over ${n} postflop hands. Results fade after the flop; review ${street.toLowerCase()} decisions.`,
  };
}

function FlopAggressionEfficiencyTab() {
  const [data, setData] = useState<MdaFlopAggressionEfficiency | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const result = await invoke<MdaFlopAggressionEfficiency>("get_mda_flop_aggression_efficiency", mdaInvokeArgs(gameType));
      setData(result);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data) {
    return <div className="dh-mda-loading">Se calculează din istoricul importat...</div>;
  }

  const aggression = weightedBars(data.aggression_percent_by_archetype);
  const ev = weightedBars(data.ev_bb_per_100_by_archetype);
  const ratio = weightedBars(data.aggression_to_ev_ratio_by_archetype);
  const cbet = weightedBars(data.cbet_frequency_as_pfr_by_archetype);
  const checkRaise = weightedBars(data.check_raise_frequency_by_archetype);
  const wwsf = weightedBars(data.wwsf_by_archetype);
  const foldToRaise = weightedBars(data.fold_to_raise_by_archetype);
  const pct = (value: number) => `${value.toFixed(1)}%`;
  const two = (value: number) => value.toFixed(2);

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mână importată" : "mâini importate"}, dintre
          care {data.flop_hands_analyzed} au ajuns la un flop. Doar {data.classified_opponents}{" "}
          {data.classified_opponents === 1 ? "adversar a strâns" : "adversari au strâns"} destule mâini (minimum 15)
          pentru a primi un tip - Hero nu este numărat. „Aggression-to-EV Ratio" = EV bb/100 împărțit la aggression %
          (EV pe fiecare punct de agresiune). Verdictele sunt reguli simple de prag, nu analiză de automatic advice.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculează..." : "Reîmprospătează"}
        </button>
      </div>
      <div className="dh-mda-grid-4">
        <InsightPanel title="Flop Aggression % by Player Type" className="dh-mda-span-2" insight={threeWayInsight(aggression.value, aggression.n, EFF_AGGRESSION)}>
          <ArchetypeHBarChart rows={data.aggression_percent_by_archetype} mode="sequential" format={pct} />
        </InsightPanel>
        <InsightPanel title="Flop EV BB/100 by Player Type" insight={threeWayInsight(ev.value, ev.n, EFF_EV)}>
          <ArchetypeHBarChart rows={data.ev_bb_per_100_by_archetype} mode="diverging" format={two} />
        </InsightPanel>
        <InsightPanel title="Aggression-to-EV Ratio by Player Type" insight={threeWayInsight(ratio.value, ratio.n, EFF_RATIO)}>
          <ArchetypeHBarChart rows={data.aggression_to_ev_ratio_by_archetype} mode="diverging" format={two} />
        </InsightPanel>
        <InsightPanel title="Flop C-Bet Frequency (as PFR)" insight={threeWayInsight(cbet.value, cbet.n, EFF_CBET)}>
          <ArchetypeHBarChart rows={data.cbet_frequency_as_pfr_by_archetype} mode="diverging" format={pct} />
        </InsightPanel>
        <InsightPanel title="Check-Raise Frequency (as Defender)" insight={threeWayInsight(checkRaise.value, checkRaise.n, EFF_CHECK_RAISE)}>
          <ArchetypeHBarChart rows={data.check_raise_frequency_by_archetype} mode="diverging" format={pct} />
        </InsightPanel>
        <InsightPanel title="WWSF % (Won When Saw Flop)" insight={threeWayInsight(wwsf.value, wwsf.n, EFF_WWSF)}>
          <ArchetypeHBarChart rows={data.wwsf_by_archetype} mode="diverging" format={pct} />
        </InsightPanel>
        <InsightPanel title="Flop Fold to Raise (Aggressor Defense)" insight={threeWayInsight(foldToRaise.value, foldToRaise.n, EFF_FOLD_TO_RAISE)}>
          <ArchetypeHBarChart rows={data.fold_to_raise_by_archetype} mode="diverging" format={pct} />
        </InsightPanel>
      </div>
    </div>
  );
}

function FlopOverCallingTab() {
  const [data, setData] = useState<MdaFlopOverCalling | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const result = await invoke<MdaFlopOverCalling>("get_mda_flop_over_calling", mdaInvokeArgs(gameType));
      setData(result);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data) {
    return <div className="dh-mda-loading">Se calculează din istoricul importat...</div>;
  }

  const call = weightedTable(data.call_vs_cbet);
  const wtsd = weightedTable(data.wtsd);
  const wsd = weightedTable(data.won_at_showdown);
  const checkCall = weightedTable(data.check_call_oop);
  const fold = combinedPercent(data.fold_to_flop_cbet, "F");
  const turnFold = combinedPercent(data.turn_fold_after_flop_call, "F");

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mână importată" : "mâini importate"}, dintre
          care {data.flop_hands_analyzed} au ajuns la un flop. Doar {data.classified_opponents}{" "}
          {data.classified_opponents === 1 ? "adversar a strâns" : "adversari au strâns"} destule mâini (minimum 15)
          pentru a primi un tip - Hero nu este numărat. „Hands" = câte mâini eligibile stau în spatele fiecărui
          procent. Verdictele sunt reguli simple de prag, nu analiză de automatic advice.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculează..." : "Reîmprospătează"}
        </button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-flop dh-mda-grid-overcall">
        <InsightPanel title="Flop Call vs C-Bet %" insight={threeWayInsight(call.value, call.n, CALL_CBET)}>
          <ArchetypeTable rows={data.call_vs_cbet} metricLabel="Call %" />
        </InsightPanel>
        <InsightPanel title="WTSD %" insight={threeWayInsight(wtsd.value, wtsd.n, WTSD)}>
          <ArchetypeTable rows={data.wtsd} metricLabel="WTSD %" />
        </InsightPanel>
        <div className="dh-mda-stack">
          <InsightPanel title="Fold to Flop C-Bet %" insight={threeWayInsight(fold.value, fold.n, FOLD_TO_CBET_ARCH)}>
            <StatBarPanel
              rows={statRowViews(data.fold_to_flop_cbet)}
              low={FOLD_TO_CBET_ARCH.low}
              high={FOLD_TO_CBET_ARCH.high}
              smallSample={smallSampleOf(data.fold_to_flop_cbet)}
              labelWidth="6.6rem"
            />
          </InsightPanel>
          <InsightPanel title="Turn Fold After Flop Call %" insight={threeWayInsight(turnFold.value, turnFold.n, TURN_FOLD)}>
            <StatBarPanel
              rows={statRowViews(data.turn_fold_after_flop_call)}
              low={TURN_FOLD.low}
              high={TURN_FOLD.high}
              smallSample={smallSampleOf(data.turn_fold_after_flop_call)}
              labelWidth="6.6rem"
            />
          </InsightPanel>
        </div>
        <InsightPanel
          title="Flop EV BB/100 by Archetype"
          insight={heatmapEvInsight(data.flop_ev_by_archetype, {
            tone: "blue",
            title: "Insufficient signal: Flop EV inconclusive",
            text: `Flop EV ${EMPTY_PATTERN}`,
          })}
        >
          <ColdCallHeatmap rows={data.flop_ev_by_archetype} unit="bb100" variant="flop" />
        </InsightPanel>
        <InsightPanel title="W$SD %" insight={threeWayInsight(wsd.value, wsd.n, WSD)}>
          <ArchetypeTable rows={data.won_at_showdown} metricLabel="W$SD %" />
        </InsightPanel>
        <InsightPanel title="Flop Check-Call % (OOP)" insight={threeWayInsight(checkCall.value, checkCall.n, CHECK_CALL_OOP)}>
          <ArchetypeTable rows={data.check_call_oop} metricLabel="Check-Call %" />
        </InsightPanel>
      </div>
    </div>
  );
}

function PostFlopEvContinuityTab() {
  const [data, setData] = useState<MdaPostFlopEvContinuity | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const result = await invoke<MdaPostFlopEvContinuity>("get_mda_post_flop_ev_continuity", mdaInvokeArgs(gameType));
      setData(result);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data) {
    return <div className="dh-mda-loading">Se calculează din istoricul importat...</div>;
  }

  const ev = data.ev_by_street;
  const n = ev.flop.length;
  const turnGap = ev.turn_bb_per_100 !== null && ev.flop_bb_per_100 !== null ? ev.turn_bb_per_100 - ev.flop_bb_per_100 : null;
  const cbetNow = data.cbet_percent_series.length > 0 ? data.cbet_percent_series[data.cbet_percent_series.length - 1] : null;
  const barrelNow = data.turn_barrel_percent_series.length > 0 ? data.turn_barrel_percent_series[data.turn_barrel_percent_series.length - 1] : null;
  const flopSeries: ChartSeries = { name: "Flop EV", color: CHART_GREEN, values: ev.flop };
  const turnSeries: ChartSeries = { name: "Turn EV", color: CHART_ORANGE, values: ev.turn };
  const riverSeries: ChartSeries = { name: "River EV", color: CHART_BLUE, values: ev.river };

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mână importată" : "mâini importate"}, dintre
          care {data.flop_hands_analyzed} au un preflop-raiser clar identificabil pe flop. Curbele EV atribuie
          rezultatul net al fiecărei mâini lui Hero străzii pe care s-a încheiat, în big blinds; „C-Bet %" și „Turn
          Barrel %" sunt procente curente pe toate mâinile cu flop, nu doar ale lui Hero. Verdictele sunt reguli
          simple de prag, nu analiză de automatic advice.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculează..." : "Reîmprospătează"}
        </button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-flop">
        <InsightPanel title="Flop EV BB/100" insight={threeWayInsight(ev.flop_bb_per_100, n, PF_FLOP_EV)}>
          <SeriesChart series={[flopSeries]} caption="Cumulative BB" />
        </InsightPanel>
        <InsightPanel title="Turn EV BB/100" insight={streetVsFlopInsight("Turn", ev.turn_bb_per_100, ev.flop_bb_per_100, n)}>
          <SeriesChart series={[turnSeries]} caption="Cumulative BB" />
        </InsightPanel>
        <InsightPanel title="River EV BB/100" insight={streetVsFlopInsight("River", ev.river_bb_per_100, ev.flop_bb_per_100, n)}>
          <SeriesChart series={[riverSeries]} caption="Cumulative BB" />
        </InsightPanel>
        <InsightPanel title="Flop-to-Turn EV Delta" insight={threeWayInsight(turnGap, n, PF_EV_DELTA)}>
          <SeriesChart series={[flopSeries, turnSeries]} caption="Cumulative BB" />
        </InsightPanel>
        <InsightPanel title="Flop C-Bet %" insight={threeWayInsight(cbetNow, data.cbet_percent_series.length, PF_CBET)}>
          <SeriesChart series={[{ name: "C-Bet %", color: CHART_ORANGE, values: data.cbet_percent_series }]} caption="C-Bet %" mode="percent" />
        </InsightPanel>
        <InsightPanel title="Turn Barrel %" insight={threeWayInsight(barrelNow, data.turn_barrel_percent_series.length, PF_BARREL)}>
          <SeriesChart series={[{ name: "Turn Barrel %", color: CHART_BLUE, values: data.turn_barrel_percent_series }]} caption="Turn Barrel %" mode="percent" />
        </InsightPanel>
      </div>
    </div>
  );
}

const TURN_BARREL_DEFENSE_CFG: ThreeWayConfig = {
  low: 25,
  high: 55,
  tones: ["yellow", "green", "red"],
  titles: ["Possible exploit: Low turn fold frequency", "No exploit edge: Turn defense is balanced", "Exploit detected: High turn fold frequency"],
  texts: [
    "Players fold only {v} to turn barrels ({n} spots). Barrels get less fold equity; value-bet thinner and bluff less.",
    "Turn fold frequency appears within a healthy range over {n} barrel spots.",
    "Players fold {v} to turn barrels ({n} spots). Sustained turn pressure is generating fold equity.",
  ],
  emptyTitle: "Insufficient signal: Turn defense inconclusive",
  emptyText: "Turn fold frequency does not present a clear or reliable pattern. Additional data is required to determine exploitability.",
};

const TURN_CALL_CFG: ThreeWayConfig = {
  low: 25,
  high: 60,
  tones: ["yellow", "green", "yellow"],
  titles: ["Possible exploit: Low turn call frequency", "Turn call frequency balanced", "Adjustment: sticky turn callers"],
  texts: [
    "Turn call frequency is below optimal levels ({v} over {n} spots). This may indicate early-stage resistance issues.",
    "Turn call frequency is {v} over {n} spots, within typical ranges.",
    "Turn call frequency is high ({v} over {n} spots). Value-bet thinner and reduce low-equity barrels.",
  ],
  emptyTitle: "Insufficient signal: Continuation inconclusive",
  emptyText: "Turn continuation behavior does not present a clear or reliable pattern. Additional data is required to determine exploitability.",
};

const TURN_XR_CFG: ThreeWayConfig = {
  low: 8,
  high: 22,
  tones: ["red", "green", "yellow"],
  titles: ["Exploit detected: No turn deterrence", "Turn check-raise balanced", "Adjustment: Elevated turn check-raise"],
  texts: [
    "Players rarely check-raise on the turn ({v} over {n} spots), providing little resistance to double barrels.",
    "Turn check-raise frequency is {v} over {n} spots, within typical ranges.",
    "Turn check-raise frequency is elevated ({v} over {n} spots). Pressure still works, but avoid autopiloting weak barrels.",
  ],
  emptyTitle: "Insufficient signal: Turn XR inconclusive",
  emptyText: "Turn check-raise patterns do not present a clear or reliable pattern. Additional data is required to determine exploitability.",
};

const TURN_AGG_CFG: ThreeWayConfig = {
  low: 20,
  high: 55,
  tones: ["yellow", "green", "yellow"],
  titles: ["Possible exploit: Low turn aggression", "Turn aggression balanced", "Adjustment: high turn aggression"],
  texts: [
    "Turn aggression is below optimal levels ({v} over {n} actions). This may indicate early-stage passivity.",
    "Turn aggression is {v} over {n} actions, within typical ranges.",
    "Turn aggression is high ({v} over {n} actions). Expect more pressure and widen bluff-catching selectively.",
  ],
  emptyTitle: "Insufficient signal: Turn aggression inconclusive",
  emptyText: "Turn aggression does not present a clear or reliable pattern.",
};

const TURN_EV_CFG: ThreeWayConfig = {
  low: -20,
  high: 20,
  unit: " bb/100",
  tones: ["red", "green", "blue"],
  titles: ["Exploit detected: Turn EV leak", "Turn EV near neutral", "Likely exploit: Turn EV outperforming results"],
  texts: [
    "Turn EV is {v} over {n} turn samples. Results are being lost on this street.",
    "Turn EV is near neutral ({v} over {n} samples) and may hide smaller underlying inefficiencies.",
    "Turn EV is {v} over {n} samples, suggesting turn decisions are generating meaningful value.",
  ],
  emptyTitle: "Insufficient signal: Turn EV inconclusive",
  emptyText: "Turn EV does not present a clear or reliable pattern. Additional data is required to determine exploitability.",
};

function TurnBarrelDefenseTab() {
  const [data, setData] = useState<MdaTurnBarrelDefense | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const result = await invoke<MdaTurnBarrelDefense>("get_mda_turn_barrel_defense", mdaInvokeArgs(gameType));
      setData(result);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data) {
    return <div className="dh-mda-loading">Se calculeaza din istoricul importat...</div>;
  }

  const fold = segmentPercent(data.turn_fold_vs_barrel[0], "F");
  const call = segmentPercent(data.turn_call_vs_barrel[0], "C");
  const xr = segmentPercent(data.turn_check_raise[0], "XR");
  const agg = combinedPercent(data.turn_aggression, "Agg");
  const ev = weightedBars(data.turn_ev);
  const wwsf = segmentPercent(data.wwsf_vs_turn_aggression[0], "W");

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} {data.hands_analyzed === 1 ? "mana importata" : "maini importate"}, dintre
          care {data.turn_hands_analyzed} au ajuns la turn. Doar {data.classified_opponents} adversari au destule maini
          pentru arhetip; Hero este exclus din panourile population-wide.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculeaza..." : "Reimprospateaza"}
        </button>
      </div>
      <div className="dh-mda-grid-even2 dh-mda-grid-turn-top">
        <InsightPanel title="Turn Fold vs Barrel %" insight={threeWayInsight(fold, data.turn_fold_vs_barrel[0]?.sample_size ?? 0, TURN_BARREL_DEFENSE_CFG)}>
          <StatBarPanel rows={statRowViews(data.turn_fold_vs_barrel)} low={TURN_BARREL_DEFENSE_CFG.low} high={TURN_BARREL_DEFENSE_CFG.high} smallSample={smallSampleOf(data.turn_fold_vs_barrel)} labelWidth="5.6rem" />
        </InsightPanel>
        <InsightPanel title="Flop Call -> Turn Fold %" insight={threeWayInsight(fold, data.flop_call_turn_fold[0]?.sample_size ?? 0, TURN_BARREL_DEFENSE_CFG)}>
          <StatBarPanel rows={statRowViews(data.flop_call_turn_fold)} low={TURN_BARREL_DEFENSE_CFG.low} high={TURN_BARREL_DEFENSE_CFG.high} smallSample={smallSampleOf(data.flop_call_turn_fold)} labelWidth="5.6rem" />
        </InsightPanel>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-turn">
        <InsightPanel title="Turn Call vs Barrel %" insight={threeWayInsight(call, data.turn_call_vs_barrel[0]?.sample_size ?? 0, TURN_CALL_CFG)}>
          <StatBarPanel rows={statRowViews(data.turn_call_vs_barrel)} low={TURN_CALL_CFG.low} high={TURN_CALL_CFG.high} smallSample={smallSampleOf(data.turn_call_vs_barrel)} />
        </InsightPanel>
        <InsightPanel title="Turn Check-Raise %" insight={threeWayInsight(xr, data.turn_check_raise[0]?.sample_size ?? 0, TURN_XR_CFG)}>
          <StatBarPanel rows={statRowViews(data.turn_check_raise)} low={TURN_XR_CFG.low} high={TURN_XR_CFG.high} smallSample={smallSampleOf(data.turn_check_raise)} />
        </InsightPanel>
        <InsightPanel title="Turn Aggression %" insight={threeWayInsight(agg.value, agg.n, TURN_AGG_CFG)}>
          <StatBarPanel rows={statRowViews(data.turn_aggression)} low={TURN_AGG_CFG.low} high={TURN_AGG_CFG.high} smallSample={smallSampleOf(data.turn_aggression)} />
        </InsightPanel>
        <InsightPanel title="Turn EV BB/100" insight={threeWayInsight(ev.value, ev.n, TURN_EV_CFG)}>
          <StatBarPanel rows={evBarViews(data.turn_ev)} low={TURN_EV_CFG.low} high={TURN_EV_CFG.high} smallSample={ev.n < SMALL_SAMPLE_THRESHOLD} />
        </InsightPanel>
        <InsightPanel title="WWSF vs Turn Aggression" className="dh-mda-panel-wide" insight={threeWayInsight(wwsf, data.wwsf_vs_turn_aggression[0]?.sample_size ?? 0, EFF_WWSF)}>
          <StatBarPanel rows={statRowViews(data.wwsf_vs_turn_aggression)} low={TURN_AGG_CFG.low} high={TURN_AGG_CFG.high} smallSample={smallSampleOf(data.wwsf_vs_turn_aggression)} />
        </InsightPanel>
      </div>
    </div>
  );
}

function TurnDefenseByArchetypeTab() {
  const [data, setData] = useState<MdaTurnDefenseByArchetype | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const result = await invoke<MdaTurnDefenseByArchetype>("get_mda_turn_defense_by_archetype", mdaInvokeArgs(gameType));
      setData(result);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data) {
    return <div className="dh-mda-loading">Se calculeaza din istoricul importat...</div>;
  }

  const fold = weightedHeatmap(data.turn_fold_vs_barrel_by_archetype);
  const callFold = weightedHeatmap(data.flop_call_turn_fold_by_archetype);
  const xr = weightedHeatmap(data.turn_check_raise_by_archetype);
  const agg = weightedHeatmap(data.turn_aggression_by_archetype);
  const ev = heatmapEvInsight(data.turn_ev_bb_per_100_by_archetype, {
    tone: "blue",
    title: "Insufficient signal: Turn EV inconclusive",
    text: "Turn EV is near neutral or unmeasured for these archetypes. Additional data is required.",
  });
  const wwsf = weightedHeatmap(data.wwsf_by_archetype);
  const delayed = weightedHeatmap(data.fold_to_delayed_turn_cbet_by_archetype);
  const af = weightedHeatmap(data.turn_aggression_factor_by_archetype);

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} maini importate si {data.turn_hands_analyzed} maini care au ajuns la turn.
          Celulele fara sample arata "--"; nu sunt tratate ca 0 real.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculeaza..." : "Reimprospateaza"}
        </button>
      </div>
      <div className="dh-mda-grid-4 dh-mda-grid-turn-arch">
        <InsightPanel title="Turn Fold vs Barrel % (by Archetype)" insight={threeWayInsight(fold.value, fold.n, TURN_BARREL_DEFENSE_CFG)}>
          <ColdCallHeatmap rows={data.turn_fold_vs_barrel_by_archetype} variant="flop" />
        </InsightPanel>
        <InsightPanel title="Flop Call -> Turn Fold % (by Archetype)" insight={threeWayInsight(callFold.value, callFold.n, TURN_BARREL_DEFENSE_CFG)}>
          <ColdCallHeatmap rows={data.flop_call_turn_fold_by_archetype} variant="flop" />
        </InsightPanel>
        <InsightPanel title="Turn Check-Raise % (by Archetype)" insight={threeWayInsight(xr.value, xr.n, TURN_XR_CFG)}>
          <ColdCallHeatmap rows={data.turn_check_raise_by_archetype} variant="flop" />
        </InsightPanel>
        <InsightPanel title="Turn Aggression % (by Archetype)" insight={threeWayInsight(agg.value, agg.n, TURN_AGG_CFG)}>
          <ColdCallHeatmap rows={data.turn_aggression_by_archetype} variant="flop" />
        </InsightPanel>
        <InsightPanel title="Turn EV BB/100 (by Archetype)" insight={ev}>
          <ColdCallHeatmap rows={data.turn_ev_bb_per_100_by_archetype} unit="bb100" variant="flop" />
        </InsightPanel>
        <InsightPanel title="WWSF % (by Archetype)" insight={threeWayInsight(wwsf.value, wwsf.n, EFF_WWSF)}>
          <ColdCallHeatmap rows={data.wwsf_by_archetype} variant="flop" />
        </InsightPanel>
        <InsightPanel title="Fold to Delayed Turn C-Bet % (by Archetype)" insight={threeWayInsight(delayed.value, delayed.n, TURN_BARREL_DEFENSE_CFG)}>
          <ColdCallHeatmap rows={data.fold_to_delayed_turn_cbet_by_archetype} variant="flop" />
        </InsightPanel>
        <InsightPanel title="Turn Aggression Factor (by Archetype)" insight={threeWayInsight(af.value, af.n, { ...TURN_AGG_CFG, low: 0.8, high: 2.2, unit: "", decimals: 2 })}>
          <ColdCallHeatmap rows={data.turn_aggression_factor_by_archetype} unit="number" variant="flop" />
        </InsightPanel>
      </div>
    </div>
  );
}

function roiSeriesInsight(values: number[], label: string): Insight {
  if (values.length === 0) {
    return { tone: "blue", title: `Insufficient signal: ${label} inconclusive`, text: `${label} does not present a clear or reliable pattern.` };
  }
  const last = values[values.length - 1];
  if (last > 20) return { tone: "yellow", title: `Likely exploit: ${label} outperforming baseline`, text: `${label} is trending positive (${last.toFixed(1)} cumulative BB), suggesting aggression is converting into results.` };
  if (last < -20) return { tone: "red", title: `Exploit detected: ${label} underperforming`, text: `${label} is trending negative (${last.toFixed(1)} cumulative BB). Turn pressure is not converting into profit in this sample.` };
  return { tone: "blue", title: `Insufficient signal: ${label} inconclusive`, text: `${label} does not present a clear or reliable pattern yet.` };
}

function TurnAggressionRoiTab() {
  const [data, setData] = useState<MdaTurnAggressionRoi | null>(null);
  const [archetypeData, setArchetypeData] = useState<MdaTurnDefenseByArchetype | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();

  async function load() {
    setLoading(true);
    try {
      const [result, archetypeResult] = await Promise.all([
        invoke<MdaTurnAggressionRoi>("get_mda_turn_aggression_roi", mdaInvokeArgs(gameType)),
        invoke<MdaTurnDefenseByArchetype>("get_mda_turn_defense_by_archetype", mdaInvokeArgs(gameType)),
      ]);
      setData(result);
      setArchetypeData(archetypeResult);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!data || !archetypeData) {
    return <div className="dh-mda-loading">Se calculeaza din istoricul importat...</div>;
  }

  const foldByType = weightedHeatmap(archetypeData.turn_fold_vs_barrel_by_archetype);
  const barrelByType = weightedHeatmap(archetypeData.turn_aggression_by_archetype);
  const xrByType = weightedHeatmap(archetypeData.turn_check_raise_by_archetype);
  const turnEv = { name: "Turn EV", color: CHART_ORANGE, values: data.turn_aggression_ev_series };
  const net = { name: "Net Winrate", color: CHART_BLUE, values: data.net_winrate_series };
  const flop = { name: "Flop EV", color: CHART_BLUE, values: data.flop_ev_series };
  const turnStreet = { name: "Turn EV", color: CHART_GREEN, values: data.turn_ev_series };
  const total = { name: "Total EV", color: CHART_BLUE, values: data.total_ev_series };
  const growth = { name: "Turn EV", color: CHART_GREEN, values: data.turn_ev_growth_series };

  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">
          Bazat pe {data.hands_analyzed} maini importate. Curbele sunt cumulative in big blinds si folosesc aceeasi
          rama/legenda ca graficele MDA deja construite.
        </p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>
          {loading ? "Se calculeaza..." : "Reimprospateaza"}
        </button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-turn-roi">
        <div className="dh-mda-stack">
          <InsightPanel title="Turn Fold vs Barrel % (by Player Type)" insight={threeWayInsight(foldByType.value, foldByType.n, TURN_BARREL_DEFENSE_CFG)}>
            <StatBarPanel
              rows={archetypeMetricRows(archetypeData.turn_fold_vs_barrel_by_archetype, "F", "C")}
              low={TURN_BARREL_DEFENSE_CFG.low}
              high={TURN_BARREL_DEFENSE_CFG.high}
              smallSample={foldByType.n < SMALL_SAMPLE_THRESHOLD}
              labelWidth="6.3rem"
            />
          </InsightPanel>
          <InsightPanel title="Turn Barrel % (by Player Type)" insight={threeWayInsight(barrelByType.value, barrelByType.n, TURN_AGG_CFG)}>
            <StatBarPanel
              rows={archetypeMetricRows(archetypeData.turn_aggression_by_archetype, "B", "NB")}
              low={TURN_AGG_CFG.low}
              high={TURN_AGG_CFG.high}
              smallSample={barrelByType.n < SMALL_SAMPLE_THRESHOLD}
              labelWidth="6.3rem"
            />
          </InsightPanel>
          <InsightPanel title="Turn Check-Raise % (Defense Deterrence)" insight={threeWayInsight(xrByType.value, xrByType.n, TURN_XR_CFG)}>
            <StatBarPanel
              rows={archetypeMetricRows(archetypeData.turn_check_raise_by_archetype, "XR", "No XR")}
              low={TURN_XR_CFG.low}
              high={TURN_XR_CFG.high}
              smallSample={xrByType.n < SMALL_SAMPLE_THRESHOLD}
              labelWidth="6.3rem"
            />
          </InsightPanel>
        </div>
        <InsightPanel title="Turn Aggression % (by Player Type)" insight={roiSeriesInsight(data.turn_aggression_ev_series, "Turn aggression ROI")}>
          <SeriesChart series={[turnEv, net]} caption="Cumulative BB" />
        </InsightPanel>
        <InsightPanel title="Turn EV BB/100 (Street-Specific)" insight={roiSeriesInsight(data.turn_ev_series, "Street-specific turn EV")}>
          <SeriesChart series={[flop, turnStreet]} caption="Cumulative BB" />
        </InsightPanel>
        <InsightPanel title="Turn EV Contribution % of Total EV" insight={roiSeriesInsight(data.turn_ev_series, "EV contribution")}>
          <SeriesChart series={[turnStreet, total]} caption="Cumulative BB" />
        </InsightPanel>
        <InsightPanel title="EV Growth Rate vs Net Winrate" insight={roiSeriesInsight(data.turn_ev_growth_series, "EV growth rate")}>
          <SeriesChart series={[growth, net]} caption="Cumulative BB" />
        </InsightPanel>
      </div>
    </div>
  );
}

function turnDataNote(hands: number, turnHands: number, opponents: number) {
  return `Bazat pe ${hands} maini importate, ${turnHands} ajunse la turn si ${opponents} adversari clasificati. Valorile fara sample raman --.`;
}

function TurnOopSurrenderRateTab() {
  const [data, setData] = useState<MdaTurnOopSurrenderRate | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();
  async function load() {
    setLoading(true);
    try { setData(await invoke<MdaTurnOopSurrenderRate>("get_mda_turn_oop_surrender_rate", mdaInvokeArgs(gameType))); } finally { setLoading(false); }
  }
  useEffect(() => { load().catch(() => undefined); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, []);
  if (!data) return <div className="dh-mda-loading">Se calculeaza din istoricul importat...</div>;

  const panel = (title: string, rows: MdaStatRow[], key: string, config: ThreeWayConfig, className?: string) => {
    const metric = combinedPercent(rows, key);
    return (
      <InsightPanel title={title} className={className} insight={threeWayInsight(metric.value, metric.n, config)}>
        <StatBarPanel rows={statRowViews(rows)} low={config.low} high={config.high} smallSample={metric.n < SMALL_SAMPLE_THRESHOLD} labelWidth="5.4rem" />
      </InsightPanel>
    );
  };
  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">{turnDataNote(data.hands_analyzed, data.turn_hands_analyzed, data.classified_opponents)}</p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>{loading ? "Se calculeaza..." : "Reimprospateaza"}</button>
      </div>
      <div className="dh-mda-grid-even2 dh-mda-grid-turn-oop-top">
        {panel("OOP Turn Aggression %", data.oop_turn_aggression, "Agg", TURN_AGG_CFG)}
        {panel("OOP Turn Bet %", data.oop_turn_bet, "Bet", TURN_AGG_CFG)}
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-turn-oop-mid">
        {panel("OOP Turn Check-Fold %", data.oop_turn_check_fold, "XF", TURN_BARREL_DEFENSE_CFG)}
        {panel("OOP Fold vs Turn Barrel %", data.oop_fold_vs_turn_barrel, "F", TURN_BARREL_DEFENSE_CFG)}
        {panel("OOP Flop Call -> Turn Fold %", data.oop_flop_call_turn_fold, "FC->TF", TURN_BARREL_DEFENSE_CFG)}
      </div>
      <div className="dh-mda-grid-even2 dh-mda-grid-turn-oop-bottom">
        {panel("OOP Turn Check-Raise %", data.oop_turn_check_raise, "XR", TURN_XR_CFG)}
        {panel("OOP Turn WWSF %", data.oop_turn_wwsf, "WWSF", EFF_WWSF)}
      </div>
    </div>
  );
}

function heatmapSignal(rows: MdaHeatmapRow[], label: string): Insight {
  const metric = weightedHeatmap(rows);
  if (metric.value === null || metric.n === 0) return { tone: "blue", title: `Insufficient signal: ${label} inconclusive.`, text: `${label} does not present a clear or reliable pattern. Additional data is required.` };
  const provisional = metric.n < SMALL_SAMPLE_THRESHOLD ? "[Small sample] " : "";
  return { tone: metric.value > 55 ? "yellow" : "blue", title: `${provisional}${metric.value > 55 ? "Signal detected" : "Insufficient signal"}: ${label}.`, text: `${label} is ${metric.value.toFixed(1)}% across ${metric.n} eligible observations.` };
}

function TurnBluffValueBalanceTab() {
  const [data, setData] = useState<MdaTurnBluffValueBalance | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();
  async function load() {
    setLoading(true);
    try { setData(await invoke<MdaTurnBluffValueBalance>("get_mda_turn_bluff_value_balance", mdaInvokeArgs(gameType))); } finally { setLoading(false); }
  }
  useEffect(() => { load().catch(() => undefined); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, []);
  if (!data) return <div className="dh-mda-loading">Se calculeaza din istoricul importat...</div>;
  const heatmap = (title: string, rows: MdaHeatmapRow[], unit: "percent" | "bb100" = "percent") => (
    <InsightPanel title={title} insight={heatmapSignal(rows, title)}>
      <ColdCallHeatmap rows={rows} unit={unit} variant="flop" />
    </InsightPanel>
  );
  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">{turnDataNote(data.hands_analyzed, data.turn_hands_analyzed, data.classified_opponents)}</p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>{loading ? "Se calculeaza..." : "Reimprospateaza"}</button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-turn-balance">
        <div className="dh-mda-stack">
          {heatmap("Turn Barrel %", data.turn_barrel)}
          {heatmap("Won at Showdown %", data.won_at_showdown)}
          {heatmap("Aggression Drop-Off %", data.aggression_drop_off)}
        </div>
        {heatmap("River Barrel After Turn Bet %", data.river_barrel_after_turn_bet)}
        {heatmap("Double Barrel -> Showdown %", data.double_barrel_showdown)}
        {heatmap("Fold to Turn Raise %", data.fold_to_turn_raise)}
        {heatmap("EV Volatility (StdDev BB/100)", data.ev_volatility, "bb100")}
      </div>
    </div>
  );
}

function leverageInsight(rows: MdaBar[], label: string): Insight {
  const metric = weightedBars(rows);
  if (metric.value === null) return { tone: "blue", title: `Insufficient signal: ${label} inconclusive.`, text: `${label} cannot be measured reliably from the imported sample yet.` };
  const tone: InsightTone = metric.value > 10 ? "yellow" : metric.value < -10 ? "red" : "green";
  return { tone, title: `${metric.n < SMALL_SAMPLE_THRESHOLD ? "[Small sample] " : ""}${label}`, text: `${label} averages ${metric.value.toFixed(2)} across ${metric.n} classified observations.` };
}

function TurnLeverageDominanceTab() {
  const [data, setData] = useState<MdaTurnLeverageDominance | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();
  async function load() {
    setLoading(true);
    try { setData(await invoke<MdaTurnLeverageDominance>("get_mda_turn_leverage_dominance", mdaInvokeArgs(gameType))); } finally { setLoading(false); }
  }
  useEffect(() => { load().catch(() => undefined); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, []);
  if (!data) return <div className="dh-mda-loading">Se calculeaza din istoricul importat...</div>;
  const chart = (title: string, rows: MdaBar[], unit: BarUnit) => (
    <InsightPanel title={title} insight={leverageInsight(rows, title)}>
      <AxisBarChart rows={[...rows].reverse()} unit={unit} mode="diverging" />
    </InsightPanel>
  );
  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">{turnDataNote(data.hands_analyzed, data.turn_hands_analyzed, data.classified_opponents)}</p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>{loading ? "Se calculeaza..." : "Reimprospateaza"}</button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-turn-leverage">
        {chart("Flop EV BB/100", data.flop_ev_bb_per_100, "bb100")}
        {chart("Turn EV BB/100", data.turn_ev_bb_per_100, "bb100")}
        {chart("Street EV Contribution %", data.street_ev_contribution, "percent")}
        {chart("EV Slope Acceleration", data.ev_slope_acceleration, "percent")}
        {chart("Decision Density", data.decision_density, "percent")}
        {chart("Turn Aggression Efficiency", data.turn_aggression_efficiency, "percent")}
      </div>
    </div>
  );
}

const RIVER_FOLD_CFG: ThreeWayConfig = {
  low: 35, high: 65, tones: ["yellow", "green", "red"],
  titles: ["Possible exploit: River calls are sticky", "River defense is balanced", "Exploit detected: River over-folding"],
  texts: ["River fold frequency is {v} over {n} sized-bet responses.", "River fold frequency is balanced at {v} over {n} responses.", "River folds reach {v} over {n} responses, creating additional bluffing room."],
  emptyTitle: "Insufficient signal: River patterns inconclusive.",
  emptyText: "River response patterns do not yet present a reliable trend. Accumulate more hands to confirm the sizing response.",
};

function RiverOverbetResponseTab() {
  const [data, setData] = useState<MdaRiverOverbetResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();
  async function load() { setLoading(true); try { setData(await invoke<MdaRiverOverbetResponse>("get_mda_river_overbet_response", mdaInvokeArgs(gameType))); } finally { setLoading(false); } }
  useEffect(() => { load().catch(() => undefined); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, []);
  if (!data) return <div className="dh-mda-loading">Se calculeaza din istoricul importat...</div>;
  const percentPanel = (title: string, rows: MdaBar[], color = CHART_GREEN) => {
    const metric = weightedBars(rows);
    return <InsightPanel title={title} insight={threeWayInsight(metric.value, metric.n, RIVER_FOLD_CFG)}><AxisBarChart rows={rows} unit="percent" mode="sequential" color={color} /></InsightPanel>;
  };
  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">Bazat pe {data.hands_analyzed} maini importate; {data.river_hands_analyzed} au actiune pe river. Pragurile sunt calculate ca bet raportat la potul dinaintea betului.</p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>{loading ? "Se calculeaza..." : "Reimprospateaza"}</button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-river-overbet-top">
        {percentPanel("River Fold vs Large Bet (>= 75% Pot)", data.fold_vs_large_bet, STAT_YELLOW)}
        {percentPanel("River Fold vs Overbet (>= 110% Pot)", data.fold_vs_overbet, STAT_YELLOW)}
        {percentPanel("River Call % vs Large Bets (>= 75% Pot)", data.call_vs_large_bet)}
      </div>
      <div className="dh-mda-grid-4 dh-mda-grid-river-overbet-bottom">
        {percentPanel("River Check-Raise %", data.river_check_raise, STAT_RED)}
        {percentPanel("River WWSF % (Won When Saw Flop)", data.river_wwsf, STAT_RED)}
        <InsightPanel title="River EV BB/100 (Street-Specific)" insight={leverageInsight(data.river_ev, "River EV signal")}><AxisBarChart rows={data.river_ev} unit="bb100" mode="diverging" /></InsightPanel>
        {percentPanel("River Bet Size Sensitivity", data.bet_size_sensitivity)}
      </div>
    </div>
  );
}

function riverPairInsight(pair: MdaRiverSeriesPair, label: string): Insight {
  if (pair.net_won.length === 0) return { tone: "blue", title: `Insufficient signal: ${label} inconclusive.`, text: `${label} has too few eligible river instances. Accumulate more hands.` };
  const last = pair.net_won[pair.net_won.length - 1];
  return { tone: Math.abs(last) > 20 ? "yellow" : "blue", title: `${pair.net_won.length < SMALL_SAMPLE_THRESHOLD ? "[Small sample] " : ""}${label}`, text: `${label} currently ends at ${last.toFixed(1)} cumulative BB across ${pair.net_won.length} observations.` };
}

function RiverBluffImbalanceTab() {
  const [data, setData] = useState<MdaRiverBluffImbalance | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();
  async function load() { setLoading(true); try { setData(await invoke<MdaRiverBluffImbalance>("get_mda_river_bluff_imbalance", mdaInvokeArgs(gameType))); } finally { setLoading(false); } }
  useEffect(() => { load().catch(() => undefined); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, []);
  if (!data) return <div className="dh-mda-loading">Se calculeaza din istoricul importat...</div>;
  const panel = (title: string, pair: MdaRiverSeriesPair, className?: string) => (
    <InsightPanel title={title} className={className} insight={riverPairInsight(pair, title)}>
      <SeriesChart series={[{ name: "Net Won", color: CHART_BLUE, values: pair.net_won }, { name: "AIEV", color: CHART_ORANGE, values: pair.all_in_ev }]} caption="Cumulative BB" />
    </InsightPanel>
  );
  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">Bazat pe {data.hands_analyzed} maini importate si {data.river_hands_analyzed} maini ajunse la river. AIEV deviaza doar cand all-in-ul poate fi evaluat din cartile cunoscute.</p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>{loading ? "Se calculeaza..." : "Reimprospateaza"}</button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-river-bluff">
        {panel("River Barrel % (After Turn Bet)", data.river_barrel, "dh-mda-river-span-2")}
        {panel("Triple Barrel Frequency %", data.triple_barrel)}
        {panel("River W$SD % (Won at Showdown)", data.won_at_showdown, "dh-mda-river-span-2")}
        {panel("River Fold vs Bet %", data.fold_vs_bet)}
        {panel("EV vs Net Win Divergence (River-Influenced)", data.ev_divergence)}
        {panel("River Bet Size Distribution", data.bet_size_distribution)}
        {panel("River Check-Raise Bluff %", data.check_raise_bluff)}
      </div>
    </div>
  );
}

function RiverEvByArchetypeTab() {
  const [data, setData] = useState<MdaRiverEvByArchetype | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();
  async function load() { setLoading(true); try { setData(await invoke<MdaRiverEvByArchetype>("get_mda_river_ev_by_archetype", mdaInvokeArgs(gameType))); } finally { setLoading(false); } }
  useEffect(() => { load().catch(() => undefined); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, []);
  if (!data) return <div className="dh-mda-loading">Se calculeaza din istoricul importat...</div>;
  const heatmap = (title: string, rows: MdaHeatmapRow[], unit: "percent" | "bb100" = "percent") => (
    <InsightPanel title={title} insight={heatmapSignal(rows, title)}><ColdCallHeatmap rows={rows} unit={unit} variant="flop" /></InsightPanel>
  );
  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">Bazat pe {data.hands_analyzed} maini, {data.river_hands_analyzed} river-uri si {data.classified_opponents} adversari clasificati. Celulele fara sample raman --.</p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>{loading ? "Se calculeaza..." : "Reimprospateaza"}</button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-river-arch">
        {heatmap("River EV BB/100 by Archetype", data.river_ev_bb_per_100, "bb100")}
        {heatmap("River Fold vs Bet % by Archetype", data.river_fold_vs_bet)}
        {heatmap("River Call vs Bet % by Archetype", data.river_call_vs_bet)}
        {heatmap("River W$SD % by Archetype", data.river_won_at_showdown)}
        {heatmap("WTSD % by Archetype", data.wtsd)}
        {heatmap("River Aggression % by Archetype", data.river_aggression)}
      </div>
    </div>
  );
}

function RiverWeakShowdownIndexTab() {
  const [data, setData] = useState<MdaRiverWeakShowdownIndex | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();
  async function load() { setLoading(true); try { setData(await invoke<MdaRiverWeakShowdownIndex>("get_mda_river_weak_showdown_index", mdaInvokeArgs(gameType))); } finally { setLoading(false); } }
  useEffect(() => { load().catch(() => undefined); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, []);
  if (!data) return <div className="dh-mda-loading">Se calculeaza din istoricul importat...</div>;
  const heatmap = (title: string, rows: MdaHeatmapRow[], unit: "percent" | "bb100" = "percent") => (
    <InsightPanel title={title} insight={heatmapSignal(rows, title)}><ColdCallHeatmap rows={rows} unit={unit} variant="flop" /></InsightPanel>
  );
  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">Bazat pe {data.hands_analyzed} maini, {data.river_hands_analyzed} river-uri si {data.classified_opponents} adversari clasificati.</p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>{loading ? "Se calculeaza..." : "Reimprospateaza"}</button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-river-arch">
        {heatmap("WTSD % (Weak Showdown Index)", data.wtsd)}
        {heatmap("W$SD % (Weak Showdown Index)", data.won_at_showdown)}
        {heatmap("BB/100 by Archetype (Weak Showdown Index)", data.river_ev_bb_per_100, "bb100")}
        {heatmap("River Call % vs Bet (Weak Showdown Index)", data.river_call_vs_bet)}
        {heatmap("Flop Call -> Showdown %", data.flop_call_to_showdown)}
        {heatmap("Turn Call -> Showdown %", data.turn_call_to_showdown)}
      </div>
    </div>
  );
}

function RiverThinValueDeficitTab() {
  const [data, setData] = useState<MdaRiverThinValueDeficit | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();
  async function load() { setLoading(true); try { setData(await invoke<MdaRiverThinValueDeficit>("get_mda_river_thin_value_deficit", mdaInvokeArgs(gameType))); } finally { setLoading(false); } }
  useEffect(() => { load().catch(() => undefined); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, []);
  if (!data) return <div className="dh-mda-loading">Se calculeaza din istoricul importat...</div>;
  const heatmap = (title: string, rows: MdaHeatmapRow[], unit: "percent" | "bb100" = "percent") => (
    <InsightPanel title={title} insight={heatmapSignal(rows, title)}><ColdCallHeatmap rows={rows} unit={unit} variant="flop" /></InsightPanel>
  );
  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">Bazat pe {data.hands_analyzed} maini, {data.river_hands_analyzed} river-uri si {data.classified_opponents} adversari clasificati. Small si Medium folosesc raportul exact bet/pot.</p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>{loading ? "Se calculeaza..." : "Reimprospateaza"}</button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-river-arch">
        {heatmap("River Bet % (Overall)", data.river_bet_overall)}
        {heatmap("Small River Bet Freq (<= 50% Pot)", data.small_river_bet)}
        {heatmap("Medium River Bet Freq (50-80% Pot)", data.medium_river_bet)}
        {heatmap("River W$SD % (Won at Showdown)", data.won_at_showdown)}
        {heatmap("River Check-Back -> Showdown Win %", data.check_back_showdown_win)}
        {heatmap("River EV BB/100", data.river_ev_bb_per_100, "bb100")}
      </div>
    </div>
  );
}

function riverStatInsight(rows: MdaStatRow[], key: string, title: string): Insight {
  const metric = combinedPercent(rows, key);
  if (metric.value === null) return { tone: "blue", title: `Insufficient signal: ${title} inconclusive.`, text: `${title} has too few eligible river observations.` };
  return { tone: metric.value > 55 ? "yellow" : "blue", title: `${metric.n < SMALL_SAMPLE_THRESHOLD ? "[Small sample] " : ""}${title}`, text: `${title} is ${metric.value.toFixed(1)}% across ${metric.n} observations.` };
}

function RiverSizingPolarizationTab() {
  const [data, setData] = useState<MdaRiverSizingPolarization | null>(null);
  const [loading, setLoading] = useState(false);
  const gameType = useMdaGameType();
  async function load() { setLoading(true); try { setData(await invoke<MdaRiverSizingPolarization>("get_mda_river_sizing_polarization", mdaInvokeArgs(gameType))); } finally { setLoading(false); } }
  useEffect(() => { load().catch(() => undefined); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, []);
  if (!data) return <div className="dh-mda-loading">Se calculeaza din istoricul importat...</div>;
  const stat = (title: string, rows: MdaStatRow[], key: string, className?: string) => (
    <InsightPanel title={title} className={className} insight={riverStatInsight(rows, key, title)}>
      <StatBarPanel rows={statRowViews(rows)} low={20} high={60} smallSample={smallSampleOf(rows)} labelWidth="4.2rem" />
    </InsightPanel>
  );
  return (
    <div className="dh-mda-note-wrap">
      <div className="dh-mda-note-row">
        <p className="dh-mda-note">Bazat pe {data.hands_analyzed} maini importate si {data.river_hands_analyzed} river-uri. Sizing: S &lt;=50%, M 50-80%, L 80-110%, OB &gt;=110% pot.</p>
        <button type="button" className="dh-mda-refresh" onClick={() => load()} disabled={loading}>{loading ? "Se calculeaza..." : "Reimprospateaza"}</button>
      </div>
      <div className="dh-mda-grid-3 dh-mda-grid-river-sizing-top">
        {stat("River Bet Size Distribution %", data.bet_size_distribution, "S")}
        {stat("Overbet Frequency %", data.overbet_frequency, "OB")}
        {stat("Large Bet Frequency (>= 80% Pot)", data.large_bet_frequency, "L+")}
      </div>
      <div className="dh-mda-grid-even2 dh-mda-grid-river-sizing-wide">
        {stat("River Aggression % (Final Street)", data.street_aggression, "Agg")}
        {stat("Fold Elasticity vs Sizing", data.fold_elasticity, "F")}
        {stat("W$SD % After Large Bets", data.won_at_showdown_after_large_bets, "W")}
        <InsightPanel title="River EV by Sizing Tier" insight={leverageInsight(data.river_ev_by_sizing_tier, "River sizing EV")}>
          <StatBarPanel rows={evBarViews(data.river_ev_by_sizing_tier)} low={-10} high={20} smallSample={data.river_ev_by_sizing_tier.reduce((sum: number, bar: MdaBar) => sum + bar.sample_size, 0) < SMALL_SAMPLE_THRESHOLD} labelWidth="4.2rem" />
        </InsightPanel>
      </div>
    </div>
  );
}

function FlopTab() {
  const [subTab, setSubTab] = useState<(typeof FLOP_SUB_TABS)[number]>("Flop C-Bet Frequency");
  return (
    <div className="dh-workspace">
      <div className="dh-mda-subtabs">
        {FLOP_SUB_TABS.map((tab) => (
          <button key={tab} type="button" className={tab === subTab ? "active" : ""} onClick={() => setSubTab(tab)}>
            {tab}
          </button>
        ))}
      </div>
      {subTab === "Flop C-Bet Frequency" && <FlopCbetFrequencyTab />}
      {subTab === "Flop-to-Turn Aggression Continuity" && <FlopToTurnContinuityTab />}
      {subTab === "Archetype Flop Edge" && <ArchetypeFlopEdgeTab />}
      {subTab === "Flop OOP Resistance" && <FlopOopResistanceTab />}
      {subTab === "Flop Aggression Efficiency" && <FlopAggressionEfficiencyTab />}
      {subTab === "Flop Over-calling" && <FlopOverCallingTab />}
      {subTab === "Post-Flop EV Continuity" && <PostFlopEvContinuityTab />}
    </div>
  );
}

function TurnTab() {
  const [subTab, setSubTab] = useState<(typeof TURN_SUB_TABS)[number]>("Turn Barrel Defense");
  return (
    <div className="dh-workspace">
      <div className="dh-mda-subtabs">
        {TURN_SUB_TABS.map((tab) => (
          <button key={tab} type="button" className={tab === subTab ? "active" : ""} onClick={() => setSubTab(tab)}>
            {tab}
          </button>
        ))}
      </div>
      {subTab === "Turn Barrel Defense" && <TurnBarrelDefenseTab />}
      {subTab === "Turn Defense by Archetype" && <TurnDefenseByArchetypeTab />}
      {subTab === "Turn Aggression ROI" && <TurnAggressionRoiTab />}
      {subTab === "Turn OOP Surrender Rate" && <TurnOopSurrenderRateTab />}
      {subTab === "Bluff-to-Value Balance" && <TurnBluffValueBalanceTab />}
      {subTab === "Turn Leverage Dominance" && <TurnLeverageDominanceTab />}
    </div>
  );
}

function RiverTab() {
  const [subTab, setSubTab] = useState<(typeof RIVER_SUB_TABS)[number]>("River Overbet Response");
  return (
    <div className="dh-workspace">
      <div className="dh-mda-subtabs">
        {RIVER_SUB_TABS.map((tab) => <button key={tab} type="button" className={tab === subTab ? "active" : ""} onClick={() => setSubTab(tab)}>{tab}</button>)}
      </div>
      {subTab === "River Overbet Response" && <RiverOverbetResponseTab />}
      {subTab === "River Bluff Imbalance" && <RiverBluffImbalanceTab />}
      {subTab === "River EV by Archetype" && <RiverEvByArchetypeTab />}
      {subTab === "Weak Showdown Index" && <RiverWeakShowdownIndexTab />}
      {subTab === "Thin Value Deficit" && <RiverThinValueDeficitTab />}
      {subTab === "River Sizing Polarization" && <RiverSizingPolarizationTab />}
    </div>
  );
}

function PreflopTab() {
  const [subTab, setSubTab] = useState<(typeof PREFLOP_SUB_TABS)[number]>("Defense vs RFI by Position");
  return (
    <div className="dh-workspace">
      <div className="dh-mda-subtabs">
        {PREFLOP_SUB_TABS.map((tab) => (
          <button
            key={tab}
            type="button"
            className={tab === subTab ? "active" : ""}
            onClick={() => setSubTab(tab)}
          >
            {tab}
          </button>
        ))}
      </div>
      {subTab === "Defense vs RFI by Position" && <DefenseVsRfiByPosition />}
      {subTab === "Positional EV Leakage" && <PositionalEvLeakage />}
      {subTab === "Cold-Call Frequency Imbalance" && <ColdCallFrequencyImbalance />}
      {subTab === "Preflop Aggression Profitability" && <PreflopAggressionProfitability />}
      {subTab === "Positional EV Realization" && <PositionalEvRealization />}
      {subTab === "Preflop Archetype Distribution" && <PreflopArchetypeDistribution />}
      {subTab === "Preflop EV Stability" && <PreflopEvStability />}
    </div>
  );
}

export default function MdaWorkspace() {
  const [mainTab, setMainTab] = useState<MainTab>("Preflop");
  const [gameType, setGameType] = useState<GameType>("NL Cash 6-max");

  return (
    <div className="dh-mda-page">
      <div className="dh-mda-toolbar">
        <div className="dh-mda-main-tabs">
          {MAIN_TABS.map((tab) => (
            <button key={tab} type="button" className={tab === mainTab ? "active" : ""} onClick={() => setMainTab(tab)}>
              {tab}
            </button>
          ))}
        </div>
        <label className="dh-mda-game-type">
          Game Type
          <select value={gameType} onChange={(event) => setGameType(event.target.value as GameType)}>
            {GAME_TYPE_OPTIONS.map((option) => (
              <option key={option} value={option}>
                {option}
              </option>
            ))}
          </select>
        </label>
      </div>
      <MdaGameTypeContext.Provider value={gameType}>
        <div key={`${mainTab}-${gameType}`} className="dh-mda-game-scope">
          {mainTab === "Preflop" ? <PreflopTab /> : mainTab === "Flop" ? <FlopTab /> : mainTab === "Turn" ? <TurnTab /> : <RiverTab />}
        </div>
      </MdaGameTypeContext.Provider>
    </div>
  );
}
