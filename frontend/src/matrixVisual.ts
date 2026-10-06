export type RangeDisplayMode = "strategy" | "strategy_ev" | "strategy_eq" | "ev" | "eq" | "range";

export type StrategyActionVisual = {
  label: string;
  frequency: number;
};

export type EvDomain = {
  min: number;
  neutral: number;
  max: number;
};

export const RANGE_FILL_COLOR = "#f59e0b";
export const RANGE_EMPTY_COLOR = "#252525";

const ACTION_PALETTE = {
  fold: "#f3333c",
  check: "#4c96cf",
  call: "#62c56f",
  allin: "#f6b728",
  betSizes: ["#b276ff", "#ff7f16", "#f45cbf", "#20c7b7", "#d9b14a"],
  raiseSizes: ["#77a0f3", "#a4d65e", "#ff6b6b", "#36a2ff", "#c084fc"],
};

export function clamp(value: number, min = 0, max = 1): number {
  return Math.max(min, Math.min(max, value));
}

export function actionKind(label: string): "fold" | "check" | "call" | "bet" | "raise" | "allin" {
  const value = label.toUpperCase();
  if (value.includes("FOLD")) return "fold";
  if (value.includes("ALLIN") || value.includes("ALL-IN")) return "allin";
  if (value.includes("RAISE")) return "raise";
  if (value.includes("BET")) return "bet";
  if (value.includes("CALL")) return "call";
  return "check";
}

function stableIndex(label: string, size: number): number {
  let hash = 0;
  for (let index = 0; index < label.length; index += 1) {
    hash = (hash * 31 + label.charCodeAt(index)) >>> 0;
  }
  return hash % size;
}

export function actionColor(label: string): string {
  const kind = actionKind(label);
  if (kind === "fold") return ACTION_PALETTE.fold;
  if (kind === "check") return ACTION_PALETTE.check;
  if (kind === "call") return ACTION_PALETTE.call;
  if (kind === "allin") return ACTION_PALETTE.allin;
  if (kind === "bet") return ACTION_PALETTE.betSizes[stableIndex(label, ACTION_PALETTE.betSizes.length)];
  return ACTION_PALETTE.raiseSizes[stableIndex(label, ACTION_PALETTE.raiseSizes.length)];
}

function hexToRgb(hex: string): [number, number, number] {
  const normalized = hex.replace("#", "");
  return [
    parseInt(normalized.slice(0, 2), 16),
    parseInt(normalized.slice(2, 4), 16),
    parseInt(normalized.slice(4, 6), 16),
  ];
}

function rgbToHex([r, g, b]: [number, number, number]): string {
  return `#${[r, g, b].map((channel) => Math.round(channel).toString(16).padStart(2, "0")).join("")}`;
}

function mixColor(from: string, to: string, amount: number): string {
  const a = hexToRgb(from);
  const b = hexToRgb(to);
  const t = clamp(amount);
  return rgbToHex([
    a[0] + (b[0] - a[0]) * t,
    a[1] + (b[1] - a[1]) * t,
    a[2] + (b[2] - a[2]) * t,
  ]);
}

function scaleColor(value: number, stops: [number, string][]): string {
  const t = clamp(value);
  for (let index = 0; index < stops.length - 1; index += 1) {
    const [leftStop, leftColor] = stops[index];
    const [rightStop, rightColor] = stops[index + 1];
    if (t >= leftStop && t <= rightStop) {
      const localT = (t - leftStop) / Math.max(0.0001, rightStop - leftStop);
      return mixColor(leftColor, rightColor, localT);
    }
  }
  return stops[stops.length - 1][1];
}

export function equityColor(equityPercent: number): string {
  return scaleColor(clamp(equityPercent / 100), [
    [0, "#d92d2d"],
    [0.25, "#f97316"],
    [0.5, "#facc15"],
    [0.75, "#a3d95f"],
    [1, "#22c55e"],
  ]);
}

export function evColor(value: number, domain: EvDomain): string {
  const min = Math.min(domain.min, domain.max);
  const max = Math.max(domain.min, domain.max);
  if (Math.abs(max - min) < 0.0001) return "#facc15";
  if (min < domain.neutral && max > domain.neutral) {
    if (value <= domain.neutral) {
      return mixColor("#d92d2d", "#facc15", (value - min) / Math.max(0.0001, domain.neutral - min));
    }
    return mixColor("#facc15", "#22c55e", (value - domain.neutral) / Math.max(0.0001, max - domain.neutral));
  }
  if (min >= domain.neutral) {
    return mixColor("#facc15", "#22c55e", (value - min) / Math.max(0.0001, max - min));
  }
  return mixColor("#d92d2d", "#facc15", (value - min) / Math.max(0.0001, max - min));
}

export function strategyGradient(actions: StrategyActionVisual[]): string {
  let offset = 0;
  const segments = actions.map((action, index) => {
    const start = offset;
    const end = index === actions.length - 1 ? 100 : Math.min(100, start + action.frequency);
    offset = end;
    return `${actionColor(action.label)} ${start}% ${end}%`;
  });
  return `linear-gradient(90deg, ${segments.join(", ")})`;
}

export function rangeFillGradient(weightPercent: number): string {
  const pct = clamp(weightPercent / 100) * 100;
  return `linear-gradient(90deg, ${RANGE_FILL_COLOR} 0% ${pct}%, ${RANGE_EMPTY_COLOR} ${pct}% 100%)`;
}

export function modeBackground(mode: RangeDisplayMode, actions: StrategyActionVisual[], eq: number, ev: number, range: number, evDomain: EvDomain): string {
  if (mode === "strategy" || mode === "strategy_ev" || mode === "strategy_eq") return strategyGradient(actions);
  if (mode === "eq") return equityColor(eq);
  if (mode === "ev") return evColor(ev, evDomain);
  return rangeFillGradient(range);
}
