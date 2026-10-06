import { useEffect, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import GaugeCluster from "./GaugeCluster";
import SettingsPanel from "./SettingsPanel";
import ViewHandWindow from "./ViewHandWindow";
import ReplayHandWindow from "./ReplayHandWindow";
import MdaWorkspace from "./Mda";
import { openReplayHandWindow, openViewHandWindow } from "./handWindows";
import { pokerTermTooltip } from "./pokerTerms";
import type {
  HandHistoryBackupResult,
  HandHistoryRecentHand,
  HandHistoryStatRow,
  HandHistoryStats,
  HandHistoryStatus,
} from "./types";
import "./styles.css";

type ViewId = "cash" | "zoom" | "tournament" | "mda" | "settings";
type NavIconId = "cash" | "zoom" | "tournament" | "mda" | "settings";
type DashboardMode = "cash" | "zoom" | "tournament";

const NAV_ITEMS: Array<{ id: ViewId; icon: NavIconId; label: string }> = [
  { id: "cash", icon: "cash", label: "Cash" },
  { id: "zoom", icon: "zoom", label: "Zoom" },
  { id: "tournament", icon: "tournament", label: "Tournament" },
  { id: "mda", icon: "mda", label: "MDA" },
];

const RO_WEEKDAYS = ["Dum", "Lun", "Mar", "Mie", "Joi", "Vin", "Sam"];
const RO_MONTHS = ["Ian", "Feb", "Mar", "Apr", "Mai", "Iun", "Iul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const CARD_SUITS = {
  c: { symbol: "♣", className: "dh-card-green" },
  s: { symbol: "♠", className: "dh-card-gray" },
  d: { symbol: "♦", className: "dh-card-blue" },
  h: { symbol: "♥", className: "dh-card-red" },
} as const;

const TOOLTIP = {
  gauges: "Statisticile sunt calculate doar din istoricul de mâini salvat/importat. Aplicația nu citește masa live.",
  moneyWon: "Câștigul net cumulat din hand history. Verde: total; SD: mâini ajunse la showdown; NSD: mâini fără showdown.",
  handHistory: "Istoricul mâinilor importate: cărțile tale, cărțile comune, acțiunile, câștigul net și poziția.",
};

interface LiveSummary {
  handHistory: HandHistoryStatus | null;
}

export function formatNumber(value: number | null | undefined, suffix = "") {
  if (value === null || value === undefined || Number.isNaN(value)) return "--";
  return `${value.toFixed(value % 1 === 0 ? 0 : 1)}${suffix}`;
}

export function formatMoney(value: number | null | undefined) {
  if (value === null || value === undefined || Number.isNaN(value)) return "--";
  const sign = value < 0 ? "-" : value > 0 ? "+" : "";
  const amount = Math.abs(value);
  const digits = amount % 1 === 0 ? 0 : 2;
  return `${sign}$${amount.toLocaleString(undefined, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  })}`;
}

export function formatChips(value: number | null | undefined) {
  if (value === null || value === undefined || Number.isNaN(value)) return "--";
  const amount = Math.abs(value);
  const digits = amount % 1 === 0 ? 0 : 2;
  return `$${amount.toLocaleString(undefined, { minimumFractionDigits: digits, maximumFractionDigits: digits })}`;
}

export function moneyAxis(minValue: number, maxValue: number) {
  const minBase = Math.min(0, minValue);
  const maxBase = Math.max(0, maxValue);
  const range = Math.max(1, maxBase - minBase);
  const padding = range * 0.12;
  const step = niceMoneyStep((range + padding * 2) / 4);
  const min = Math.floor((minBase - padding) / step) * step;
  const max = Math.ceil((maxBase + padding) / step) * step;
  const ticks: number[] = [];
  for (let tick = min; tick <= max + step / 2; tick += step) {
    ticks.push(Math.abs(tick) < step / 1000 ? 0 : tick);
  }
  return { min, max, span: Math.max(1, max - min), ticks };
}

export function chartLabelIndexes(count: number) {
  if (count <= 0) return [];
  if (count <= 3) return Array.from({ length: count }, (_, index) => index);
  return Array.from(new Set([0, Math.floor((count - 1) / 2), count - 1]));
}

export function ChartGrid({
  width,
  height,
  left = 34,
  right = 8,
  top = 12,
  bottom = 20,
  yTicks = [0, 25, 50, 75, 100],
  yFor,
}: {
  width: number;
  height: number;
  left?: number;
  right?: number;
  top?: number;
  bottom?: number;
  yTicks?: number[];
  yFor?: (value: number) => number;
}) {
  const plotHeight = Math.max(1, height - top - bottom);
  const y = yFor ?? ((value: number) => top + (1 - value / 100) * plotHeight);
  return (
    <>
      {yTicks.map((tick) => (
        <line
          key={tick}
          x1={left}
          y1={y(tick)}
          x2={width - right}
          y2={y(tick)}
          stroke="rgba(114, 129, 164, 0.22)"
          strokeDasharray="4 4"
          strokeWidth="1"
        />
      ))}
      <line x1={left} y1={top} x2={left} y2={height - bottom} stroke="rgba(114, 129, 164, 0.34)" strokeWidth="1" />
      <line x1={left} y1={height - bottom} x2={width - right} y2={height - bottom} stroke="rgba(114, 129, 164, 0.34)" strokeWidth="1" />
    </>
  );
}

export default function App() {
  const [view, setView] = useState<ViewId>("cash");
  const [summary, setSummary] = useState<LiveSummary>({ handHistory: null });

  useEffect(() => {
    let cancelled = false;
    async function refresh() {
      const handHistory = await invoke<HandHistoryStatus>("get_hand_history_status");
      if (!cancelled) setSummary({ handHistory });
    }
    refresh().catch(() => undefined);
    const interval = window.setInterval(() => refresh().catch(() => undefined), 1200);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, []);

  const handWindowParams = new URLSearchParams(window.location.search);
  const handWindowKind = handWindowParams.get("window");
  const handWindowId = handWindowParams.get("hand");
  if (handWindowKind === "view-hand" && handWindowId) return <ViewHandWindow handId={handWindowId} />;
  if (handWindowKind === "replay-hand" && handWindowId) return <ReplayHandWindow handId={handWindowId} />;

  return (
    <div className="app-shell dh-shell">
      <TitleBar />
      <Sidebar activeView={view} onNavigate={setView} />
      <main className="dh-main">
        {view === "cash" && <HistoryDashboard summary={summary} mode="cash" />}
        {view === "zoom" && <HistoryDashboard summary={summary} mode="zoom" />}
        {view === "tournament" && <HistoryDashboard summary={summary} mode="tournament" />}
        {view === "mda" && <MdaWorkspace />}
        {view === "settings" && <SettingsWorkspace />}
      </main>
    </div>
  );
}

function TitleBar() {
  return (
    <header className="dh-titlebar">
      <div className="dh-titlebar-logo">SOFT-POKER TRACKER</div>
      <div className="dh-window-buttons" aria-hidden="true"><span /><span /><span /></div>
    </header>
  );
}

function Sidebar({ activeView, onNavigate }: { activeView: ViewId; onNavigate: (view: ViewId) => void }) {
  return (
    <aside className="dh-sidebar">
      <div className="dh-collapse">{"<<"}</div>
      <div className="dh-brand">
        <div className="dh-brand-mark">SP</div>
        <div><strong>SoftPoker</strong><span>History Tracker</span></div>
      </div>
      <nav className="dh-nav" aria-label="Main navigation">
        {NAV_ITEMS.map((item) => (
          <button
            key={item.id}
            type="button"
            className={activeView === item.id ? "dh-nav-item dh-nav-item-active" : "dh-nav-item"}
            onClick={() => onNavigate(item.id)}
            title={item.label}
            aria-label={item.label}
          >
            <span className="dh-nav-icon"><NavIcon id={item.icon} /></span>
            <span className="dh-nav-copy"><strong>{item.label}</strong></span>
          </button>
        ))}
      </nav>
      <div className="dh-sidebar-footer">
        <button type="button" className="dh-nav-item" onClick={() => onNavigate("settings")}>
          <span className="dh-nav-icon"><NavIcon id="settings" /></span>
          <span className="dh-nav-copy"><strong>Settings</strong></span>
        </button>
      </div>
    </aside>
  );
}

function NavIcon({ id }: { id: NavIconId }) {
  const common = { fill: "none", stroke: "currentColor", strokeWidth: 2, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      {id === "cash" && <><path {...common} d="M4 7h16v10H4z" /><path {...common} d="M8 12h.01M16 12h.01" /><path {...common} d="M12 9v6" /></>}
      {id === "zoom" && <><path {...common} d="M4 12h7l-2 7 11-11h-7l2-5z" /></>}
      {id === "tournament" && <><path {...common} d="M8 4h8v3a4 4 0 0 1-8 0z" /><path {...common} d="M6 5H4v2a3 3 0 0 0 4 3" /><path {...common} d="M18 5h2v2a3 3 0 0 1-4 3" /><path {...common} d="M12 11v5" /><path {...common} d="M9 20h6" /></>}
      {id === "mda" && <><path {...common} d="M5 19V8" /><path {...common} d="M12 19V5" /><path {...common} d="M19 19v-9" /><path {...common} d="M3 19h18" /></>}
      {id === "settings" && <><circle {...common} cx="12" cy="12" r="3" /><path {...common} d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M19.1 4.9 17 7M7 17l-2.1 2.1" /></>}
    </svg>
  );
}

function HistoryDashboard({ summary, mode }: { summary: LiveSummary; mode: DashboardMode }) {
  const [activeTab, setActiveTab] = useState("Overall");
  const [tableFilter, setTableFilter] = useState("All");
  const [stats, setStats] = useState<HandHistoryStats | null>(null);
  const [backupPath, setBackupPath] = useState("");
  const [selectedBackupFileName, setSelectedBackupFileName] = useState("");
  const [selectedBackupJson, setSelectedBackupJson] = useState("");
  const [backupBusy, setBackupBusy] = useState(false);
  const [backupMessage, setBackupMessage] = useState<string | null>(null);
  const backupFileInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function refreshStats() {
      const nextStats = await invoke<HandHistoryStats>("get_hand_history_stats_for_game_type", { gameType: mode });
      if (!cancelled) setStats(nextStats);
    }
    setStats(null);
    refreshStats().catch(() => undefined);
    const interval = window.setInterval(() => refreshStats().catch(() => undefined), 1200);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [mode]);

  const recentHands = stats?.recent_hands ?? [];
  const tableNames = stats?.table_names ?? [];
  const filteredRecentHands = tableFilter === "All"
    ? recentHands
    : recentHands.filter((hand) => hand.table_name === tableFilter);

  async function exportHistory() {
    setBackupBusy(true);
    setBackupMessage(null);
    try {
      const result = await invoke<HandHistoryBackupResult>("export_hand_history_backup", { path: backupPath.trim() || null });
      setBackupPath(result.path);
      setBackupMessage(`Exported ${result.total_hands} hands to ${result.path}`);
    } catch (error) {
      setBackupMessage(`Export failed: ${String(error)}`);
    } finally {
      setBackupBusy(false);
    }
  }

  async function importHistory() {
    if (selectedBackupJson) {
      setBackupBusy(true);
      setBackupMessage(null);
      try {
        const result = await invoke<HandHistoryBackupResult>("import_hand_history_backup_json", {
          fileName: selectedBackupFileName,
          json: selectedBackupJson,
        });
        setBackupMessage(`Imported ${result.total_hands} saved hands from ${result.path}`);
      } catch (error) {
        setBackupMessage(`Import failed: ${String(error)}`);
      } finally {
        setBackupBusy(false);
      }
      return;
    }

    const path = backupPath.trim();
    if (!path) {
      setBackupMessage("Alege un fișier .json sau scrie calea backup-ului, apoi apasă Import JSON.");
      return;
    }
    setBackupBusy(true);
    setBackupMessage(null);
    try {
      const result = await invoke<HandHistoryBackupResult>("import_hand_history_backup", { path });
      setBackupMessage(`Imported ${result.total_hands} saved hands from ${result.path}`);
    } catch (error) {
      setBackupMessage(`Import failed: ${String(error)}`);
    } finally {
      setBackupBusy(false);
    }
  }

  async function selectBackupFile(file: File | null) {
    if (!file) return;
    setBackupMessage(null);
    try {
      const json = await file.text();
      setSelectedBackupFileName(file.name);
      setSelectedBackupJson(json);
      setBackupPath("");
      setBackupMessage(`Fișier pregătit: ${file.name}. Apasă Import JSON ca să îl încarci în soft.`);
    } catch (error) {
      setSelectedBackupFileName("");
      setSelectedBackupJson("");
      setBackupMessage(`Nu am putut citi fișierul: ${String(error)}`);
    } finally {
      if (backupFileInputRef.current) backupFileInputRef.current.value = "";
    }
  }

  return (
    <div className="dh-workspace">
      <section className="dh-top-grid">
        <div className="dh-panel dh-gauges-panel" title={TOOLTIP.gauges}>
          <div className="dh-gauges-header">
            <PanelHeader title={`Gauges - ${modeLabel(mode)}`} tooltip={TOOLTIP.gauges} />
            <div className="dh-gauge-summary">
              <span>Total Hands: <strong>{stats?.total_hands ?? 0}</strong></span>
              <span>Money Won: <strong>{formatMoney(stats?.total_won)}</strong></span>
              <span>bb/100: <strong>{formatNumber(stats?.bb_per_100)}</strong></span>
            </div>
          </div>
          <GaugeCluster stats={stats} />
        </div>
        <MoneyWonPanel hands={filteredRecentHands} className="dh-panel dh-chart-panel" />
      </section>

      <div className="dh-filter-bar">
        <span>Filters:</span>
        <select aria-label="Game type" value={mode} disabled>
          <option value="cash">Cash</option>
          <option value="zoom">Zoom</option>
          <option value="tournament">Tournament</option>
        </select>
        <select aria-label="Table name" value={tableFilter} onChange={(event) => setTableFilter(event.target.value)}>
          <option value="All">All tables</option>
          {tableNames.map((table) => <option key={table} value={table}>{table}</option>)}
        </select>
        <input
          className="dh-backup-path"
          type="text"
          value={backupPath}
          placeholder="Backup .json path"
          onChange={(event) => {
            setBackupPath(event.target.value);
            setSelectedBackupFileName("");
            setSelectedBackupJson("");
          }}
        />
        <button type="button" className="dh-filter-button" disabled={backupBusy} onClick={exportHistory}>Export JSON</button>
        <label className="dh-backup-upload">
          <span>Alege JSON</span>
          <strong>{selectedBackupFileName || "Niciun fișier selectat"}</strong>
          <input
            ref={backupFileInputRef}
            type="file"
            accept="application/json,.json"
            disabled={backupBusy}
            onChange={(event) => void selectBackupFile(event.target.files?.[0] ?? null)}
          />
        </label>
        <button type="button" className="dh-filter-button" disabled={backupBusy} onClick={importHistory}>Import JSON</button>
      </div>
      {backupMessage && <p className="dh-backup-message">{backupMessage}</p>}

      <section className="dh-panel dh-data-panel">
        <DashboardTabs active={activeTab} onSelect={setActiveTab} />
        <StatsTable activeTab={activeTab} summary={summary} stats={stats} />
      </section>

      <section className="dh-panel dh-hand-history-panel" title={TOOLTIP.handHistory}>
        <HandHistoryTable hands={filteredRecentHands} />
      </section>
    </div>
  );
}

function modeLabel(mode: DashboardMode) {
  if (mode === "zoom") return "Zoom";
  if (mode === "tournament") return "Tournament";
  return "Cash";
}

export function PanelHeader({ title, action, tooltip }: { title: string; action?: string; tooltip?: string }) {
  return (
    <div className="dh-panel-header" title={tooltip}>
      <h2>{title}</h2>
      {action && <button type="button" title={tooltip}>{action}</button>}
    </div>
  );
}

function DashboardTabs({ active, onSelect }: { active: string; onSelect: (tab: string) => void }) {
  const tabs = ["Overall", "Position", "Sessions", "Stakes", "Hole Cards", "Time", "Showdown Hands", "Poker Site"];
  return (
    <div className="dh-dashboard-tabs">
      {tabs.map((tab) => (
        <button key={tab} type="button" className={tab === active ? "active" : ""} onClick={() => onSelect(tab)}>{tab}</button>
      ))}
    </div>
  );
}

function MoneyWonPanel({ hands, className }: { hands: HandHistoryRecentHand[]; className: string }) {
  const [unit, setUnit] = useState<MoneyChartUnit>("bb");
  const [period, setPeriod] = useState<MoneyChartPeriod>("hands");
  const [lines, setLines] = useState<Record<MoneyLineKey, boolean>>({ total: true, sd: true, nsd: true });
  const points = buildMoneyChartPoints(hands, unit, period);
  const yValues = points.flatMap((point) => [point.total, point.sd, point.nsd]);
  const domain = niceMoneyDomain(yValues);
  const width = 760;
  const height = 250;
  const pad = { left: 56, right: 18, top: 20, bottom: 32 };
  const innerW = width - pad.left - pad.right;
  const innerH = height - pad.top - pad.bottom;
  const x = (index: number) => pad.left + (points.length <= 1 ? innerW / 2 : (index / (points.length - 1)) * innerW);
  const y = (value: number) => pad.top + ((domain.max - value) / domain.span) * innerH;
  const pathFor = (key: MoneyLineKey) => points.map((point, index) => `${index === 0 ? "M" : "L"} ${x(index)} ${y(point[key])}`).join(" ");

  return (
    <section className={className} title={TOOLTIP.moneyWon}>
      <PanelHeader title="Money Won" tooltip={TOOLTIP.moneyWon} />
      <div className="dh-money-toolbar">
        <div className="dh-money-segment">
          {(["bb", "currency"] as MoneyChartUnit[]).map((nextUnit) => (
            <button key={nextUnit} type="button" className={unit === nextUnit ? "active" : ""} onClick={() => setUnit(nextUnit)}>
              {nextUnit === "bb" ? "BB" : "$"}
            </button>
          ))}
        </div>
        <select value={period} onChange={(event) => setPeriod(event.target.value as MoneyChartPeriod)} aria-label="Money chart period">
          <option value="hands">By hand</option>
          <option value="week">Week</option>
          <option value="month">Month</option>
          <option value="year">Year</option>
        </select>
        <div className="dh-money-line-controls">
          {(["total", "sd", "nsd"] as MoneyLineKey[]).map((line) => (
            <button
              key={line}
              type="button"
              className={`dh-money-line-toggle dh-money-line-${line}${lines[line] ? " active" : ""}`}
              aria-pressed={lines[line]}
              onClick={() => setLines((prev) => ({ ...prev, [line]: !prev[line] }))}
            >
              {MONEY_LINE_INFO[line].label}
            </button>
          ))}
        </div>
      </div>
      <svg className="dh-money-chart" viewBox={`0 0 ${width} ${height}`} role="img" aria-label="Money won chart">
        {domain.ticks.map((tick) => (
          <g key={tick}>
            <line x1={pad.left} x2={width - pad.right} y1={y(tick)} y2={y(tick)} className="dh-chart-grid" />
            <text x={pad.left - 8} y={y(tick)} className="dh-money-axis" textAnchor="end" dominantBaseline="middle">
              {unit === "bb" ? `${tick}` : `$${tick}`}
            </text>
          </g>
        ))}
        <line x1={pad.left} x2={width - pad.right} y1={y(0)} y2={y(0)} className="dh-chart-zero" />
        {points.length === 0 ? (
          <text x={width / 2} y={height / 2} className="dh-chart-empty" textAnchor="middle">No hands yet</text>
        ) : (
          (["total", "sd", "nsd"] as MoneyLineKey[]).map((line) => lines[line] && (
            <path key={line} d={pathFor(line)} className={`dh-money-path dh-money-path-${line}`} fill="none" />
          ))
        )}
      </svg>
    </section>
  );
}

type MoneyChartUnit = "bb" | "currency";
type MoneyChartPeriod = "hands" | "week" | "month" | "year";
type MoneyLineKey = "total" | "sd" | "nsd";
type MoneyChartPoint = { label: string; total: number; sd: number; nsd: number };

const MONEY_LINE_INFO: Record<MoneyLineKey, { label: string }> = {
  total: { label: "Total" },
  sd: { label: "SD" },
  nsd: { label: "NSD" },
};

function buildMoneyChartPoints(hands: HandHistoryRecentHand[], unit: MoneyChartUnit, period: MoneyChartPeriod): MoneyChartPoint[] {
  const ordered = [...hands].reverse();
  const buckets = new Map<string, { label: string; hands: HandHistoryRecentHand[] }>();
  for (const hand of ordered) {
    const date = parseHandDate(hand.timestamp);
    const key = period === "hands" ? hand.hand_id : bucketKey(date, period);
    const label = period === "hands" ? String(buckets.size + 1) : key;
    const bucket = buckets.get(key) ?? { label, hands: [] };
    bucket.hands.push(hand);
    buckets.set(key, bucket);
  }

  let total = 0;
  let sd = 0;
  let nsd = 0;
  return Array.from(buckets.values()).map((bucket) => {
    for (const hand of bucket.hands) {
      const value = moneyChartHandValue(hand.net_won, hand, unit);
      total += value;
      if (hand.went_to_showdown) sd += value;
      else nsd += value;
    }
    return { label: bucket.label, total, sd, nsd };
  });
}

function moneyChartHandValue(value: number, hand: HandHistoryRecentHand, unit: MoneyChartUnit) {
  if (unit === "currency") return value;
  return hand.big_blind && hand.big_blind > 0 ? value / hand.big_blind : value;
}

function parseHandDate(value: string | null | undefined) {
  if (!value) return null;
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? null : parsed;
}

function bucketKey(date: Date | null, period: MoneyChartPeriod) {
  if (!date) return "Unknown";
  if (period === "year") return String(date.getFullYear());
  if (period === "month") return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}`;
  if (period === "week") {
    const start = new Date(date);
    start.setDate(date.getDate() - date.getDay());
    return `${start.getFullYear()}-${String(start.getMonth() + 1).padStart(2, "0")}-${String(start.getDate()).padStart(2, "0")}`;
  }
  return date.toISOString();
}

function niceMoneyDomain(values: number[]) {
  const finite = values.filter((value) => Number.isFinite(value));
  const minValue = Math.min(0, ...finite);
  const maxValue = Math.max(0, ...finite);
  const padding = Math.max(5, (maxValue - minValue) * 0.15);
  const min = Math.floor((minValue - padding) / 10) * 10;
  const max = Math.ceil((maxValue + padding) / 10) * 10;
  const span = Math.max(1, max - min);
  const step = niceMoneyStep(span / 4);
  const ticks: number[] = [];
  for (let tick = Math.ceil(min / step) * step; tick <= max + step / 2; tick += step) ticks.push(tick);
  return { min, max, span, ticks };
}

function niceMoneyStep(rawStep: number) {
  const power = Math.pow(10, Math.floor(Math.log10(Math.max(1, rawStep))));
  const normalized = rawStep / power;
  const multiplier = normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10;
  return multiplier * power;
}

function StatsTable({ activeTab, summary, stats }: { activeTab: string; summary: LiveSummary; stats: HandHistoryStats | null }) {
  const overall: StatTableRow = {
    label: "All",
    totalHands: stats?.total_hands ?? 0,
    totalWon: stats?.total_won ?? 0,
    bb100: stats?.bb_per_100 ?? null,
    evBb100: stats?.bb_per_100 ?? null,
    vpip: stats?.vpip_percent ?? null,
    pfr: stats?.pfr_percent ?? null,
    aggression: stats?.aggression_factor ?? null,
    af: stats?.aggression_factor ?? null,
    wtsd: stats?.went_to_showdown_percent ?? null,
  };

  if (activeTab === "Hole Cards") {
    const rows = stats?.by_hole_cards ?? [];
    return (
      <TableShell>
        <thead><tr><th>Down Cards</th>{STAT_COLUMNS.map((column) => <th key={column}>{column}</th>)}</tr></thead>
        <tbody>
          {rows.length === 0 ? <NoDataRow span={STAT_COLUMNS.length + 1} /> : rows.slice(0, 32).map((row, index) => (
            <StatRow key={row.key} row={statRowFromGroup(row)} firstCell={<CardRun text={row.key} />} selected={index === 0} />
          ))}
        </tbody>
        <StatFooter row={overall} firstLabel="" />
      </TableShell>
    );
  }

  if (activeTab === "Position") return <GroupedStatsTable firstHeader="Position" rows={stats?.by_position ?? []} total={overall} />;

  if (activeTab === "Sessions") {
    const recent = stats?.recent_hands ?? [];
    const first = recent.length > 0 ? recent[recent.length - 1] : null;
    return (
      <TableShell>
        <thead><tr><th>Session Start</th><th>Session Length</th><th>Games Played</th>{STAT_COLUMNS.map((column) => <th key={column}>{column}</th>)}</tr></thead>
        <tbody>
          <tr className="selected">
            <td>{formatTimestamp(first?.timestamp)}</td>
            <td>--</td>
            <td>{summary.handHistory?.current_table ?? "PokerStars"}</td>
            <td>{stats?.total_hands ?? 0}</td>
            <td><MoneyText value={stats?.total_won ?? 0} /></td>
            <td>{formatNumber(stats?.bb_per_100)}</td>
            <td>{formatNumber(stats?.bb_per_100)}</td>
            <td>{formatNumber(stats?.vpip_percent)}</td>
            <td>{formatNumber(stats?.pfr_percent)}</td>
            <td>--</td><td>--</td>
            <td>{formatNumber(stats?.went_to_showdown_percent)}</td>
            <td>--</td>
            <td>{formatNumber(stats?.aggression_factor)}</td>
            <td>{formatNumber(stats?.aggression_factor)}</td>
            <td>--</td><td>--</td>
          </tr>
        </tbody>
      </TableShell>
    );
  }

  if (activeTab === "Poker Site") {
    return (
      <TableShell>
        <thead><tr><th>Poker Site</th>{STAT_COLUMNS.map((column) => <th key={column}>{column}</th>)}</tr></thead>
        <tbody><StatRow row={overall} firstCell="PokerStars" selected /></tbody>
      </TableShell>
    );
  }

  if (activeTab === "Showdown Hands") {
    const rows = (stats?.recent_hands ?? []).filter((hand) => hand.went_to_showdown);
    return (
      <TableShell>
        <thead><tr><th>Date</th><th>Table</th><th>Cards</th><th>Board</th><th>Net Won</th><th>Position</th></tr></thead>
        <tbody>
          {rows.length === 0 ? <NoDataRow span={6} /> : rows.map((hand) => (
            <tr key={hand.hand_id}>
              <td>{formatTimestamp(hand.timestamp)}</td>
              <td>{hand.table_name || "--"}</td>
              <td><CardRun text={hand.cards} /></td>
              <td><CardRun text={hand.board} /></td>
              <td><MoneyText value={hand.net_won} /></td>
              <td>{hand.position || "--"}</td>
            </tr>
          ))}
        </tbody>
      </TableShell>
    );
  }

  if (activeTab === "Stakes" || activeTab === "Time") return <NoDataTable title={activeTab} />;

  return (
    <TableShell>
      <thead><tr><th></th>{STAT_COLUMNS.map((column) => <th key={column}>{column}</th>)}</tr></thead>
      <tbody><StatRow row={overall} firstCell="" selected /></tbody>
    </TableShell>
  );
}

type StatTableRow = {
  label: string;
  totalHands: number | string;
  totalWon: number | null;
  bb100: number | null;
  evBb100?: number | null;
  vpip?: number | null;
  pfr?: number | null;
  threeBet?: number | null;
  callThreeBet?: number | null;
  wtsd?: number | null;
  wwsf?: number | null;
  aggression?: number | null;
  af?: number | null;
  cbBet?: number | null;
  steal?: number | null;
};

const STAT_COLUMNS = ["Total Hands", "Total Won", "BB/100", "EV BB/100", "VPIP", "PFR", "3Bet", "Call 3Bet", "WTSD", "WWSF", "Agg", "AF", "CBet", "Steal"];

function statRowFromGroup(row: HandHistoryStatRow): StatTableRow {
  return {
    label: row.key,
    totalHands: row.total_hands,
    totalWon: row.total_won,
    bb100: row.bb_per_100,
    evBb100: row.bb_per_100,
    vpip: row.vpip_percent,
    pfr: row.pfr_percent,
    aggression: row.aggression_factor,
    af: row.aggression_factor,
  };
}

function GroupedStatsTable({ firstHeader, rows, total }: { firstHeader: string; rows: HandHistoryStatRow[]; total: StatTableRow }) {
  return (
    <TableShell>
      <thead><tr><th>{firstHeader}</th>{STAT_COLUMNS.map((column) => <th key={column}>{column}</th>)}</tr></thead>
      <tbody>
        {rows.length === 0 ? <NoDataRow span={STAT_COLUMNS.length + 1} /> : rows.slice(0, 32).map((row, index) => (
          <StatRow key={row.key} row={statRowFromGroup(row)} firstCell={row.key} selected={index === 0} />
        ))}
      </tbody>
      <StatFooter row={total} firstLabel="" />
    </TableShell>
  );
}

function TableShell({ children }: { children: ReactNode }) {
  return <div className="dh-table-wrap"><table className="dh-stats-table">{children}</table></div>;
}

function StatRow({ row, firstCell, selected = false }: { row: StatTableRow; firstCell: ReactNode; selected?: boolean }) {
  return (
    <tr className={selected ? "selected" : ""}>
      <td>{firstCell}</td>
      <td>{row.totalHands}</td>
      <td><MoneyText value={row.totalWon} /></td>
      <td>{formatNumber(row.bb100)}</td>
      <td>{formatNumber(row.evBb100)}</td>
      <td>{formatNumber(row.vpip)}</td>
      <td>{formatNumber(row.pfr)}</td>
      <td>{formatNumber(row.threeBet)}</td>
      <td>{formatNumber(row.callThreeBet)}</td>
      <td>{formatNumber(row.wtsd)}</td>
      <td>{formatNumber(row.wwsf)}</td>
      <td>{formatNumber(row.aggression)}</td>
      <td>{formatNumber(row.af)}</td>
      <td>{formatNumber(row.cbBet)}</td>
      <td>{formatNumber(row.steal)}</td>
    </tr>
  );
}

function StatFooter({ row, firstLabel }: { row: StatTableRow; firstLabel: string }) {
  return <tfoot><StatRow row={row} firstCell={firstLabel} /></tfoot>;
}

function NoDataRow({ span }: { span: number }) {
  return <tr><td className="dh-no-data" colSpan={span}>No data</td></tr>;
}

function NoDataTable({ title }: { title: string }) {
  return (
    <TableShell>
      <thead><tr><th>{title}</th><th>Total Hands</th><th>Total Won</th><th>BB/100</th></tr></thead>
      <tbody><NoDataRow span={4} /></tbody>
    </TableShell>
  );
}

export function CardRun({ text, empty = "--" }: { text: string | null | undefined; empty?: string }) {
  const tokens = parseCardTokens(text);
  if (tokens.length === 0) return <span className="dh-muted-text">{empty}</span>;
  return (
    <span className="dh-card-run">
      {tokens.map((token, index) => <CardBadge key={`${token.rank}-${token.suit ?? "class"}-${index}`} token={token} />)}
    </span>
  );
}

export function MoneyText({ value }: { value: number | null | undefined }) {
  const negative = typeof value === "number" && value < 0;
  const positive = typeof value === "number" && value > 0;
  return <span className={negative ? "dh-money-negative" : positive ? "dh-money-positive" : ""}>{formatMoney(value)}</span>;
}

type CardToken = { rank: string; suit: string | null; suffix?: string };

function parseCardTokens(text: string | null | undefined): CardToken[] {
  if (!text || text === "--" || text === "-") return [];
  return text.replace(/\[|\]/g, " ").split(/\s+/).filter(Boolean).map((raw) => {
    const match = raw.match(/^([AKQJT2-9]{1,2})([cdhs])?([os])?$/i);
    if (!match) return { rank: raw, suit: null };
    const rank = match[1].toUpperCase().replace("10", "T");
    const suit = match[2]?.toLowerCase() ?? null;
    return { rank, suit, suffix: match[3] };
  });
}

function CardBadge({ token }: { token: CardToken }) {
  const suit = token.suit && token.suit in CARD_SUITS ? CARD_SUITS[token.suit as keyof typeof CARD_SUITS] : null;
  return (
    <span className={`dh-card-badge ${suit?.className ?? "dh-card-class"}`}>
      <strong>{token.rank}</strong>
      {suit ? <span>{suit.symbol}</span> : token.suffix ? <span>{token.suffix}</span> : null}
    </span>
  );
}

function PreflopBadge({ labels }: { labels: string[] | undefined }) {
  const displayed = labels?.length ? labels : ["Unopened"];
  const tooltip = displayed.map((label) => pokerTermTooltip(label) ?? label).join("\n");
  return (
    <span className={`dh-preflop-badges${displayed.length > 1 ? " dh-preflop-badges-stacked" : ""}`} title={tooltip}>
      {displayed.map((label) => <span key={label} className={`dh-action-pill ${preflopBadgeClass(label)}`} title={pokerTermTooltip(label) ?? tooltip}>{label}</span>)}
    </span>
  );
}

function preflopBadgeClass(label: string) {
  if (label === "Unopened") return "dh-action-pill-unopened";
  if (label.includes("Raiser")) return "dh-action-pill-raiser";
  if (label === "1 Limper") return "dh-action-pill-limper";
  if (label === "3Bet") return "dh-action-pill-threebet";
  if (label.includes("Caller")) return "dh-action-pill-callers";
  return "";
}

function ActionRun({ line }: { line: string }) {
  const compact = compactLine(line);
  if (!compact || compact === "--") return <span className="dh-action-empty"></span>;
  return (
    <span className="dh-action-run">
      {compact.split("-").map((action: string, index: number) => <span className={actionClass(action)} key={`${action}-${index}`} title={pokerTermTooltip(action)}>{action}</span>)}
    </span>
  );
}

function actionClass(action: string) {
  if (action.startsWith("F")) return "dh-action-fold";
  if (action.startsWith("X")) return "dh-action-check";
  if (action.startsWith("C")) return "dh-action-call";
  if (action.startsWith("R") || action.startsWith("B") || action.startsWith("AI")) return "dh-action-bet";
  return "dh-action-call";
}

function compactLine(line: string) {
  if (!line || line === "--") return "--";
  return line
    .replace(/folds/g, "F")
    .replace(/checks/g, "X")
    .replace(/calls/g, "C")
    .replace(/bets/g, "B")
    .replace(/raises/g, "R")
    .replace(/all-in/g, "AI");
}

export function formatTimestamp(value: string | null | undefined) {
  if (!value) return "--";
  const match = value.match(/(\d{4})[/-](\d{1,2})[/-](\d{1,2})\s+(\d{1,2}):(\d{2})(?::(\d{2}))?/);
  const parsed = match
    ? new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]), Number(match[4]), Number(match[5]), Number(match[6] ?? 0))
    : new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  const weekday = RO_WEEKDAYS[parsed.getDay()];
  const day = parsed.getDate();
  const month = RO_MONTHS[parsed.getMonth()];
  const year = parsed.getFullYear();
  const hour = String(parsed.getHours()).padStart(2, "0");
  const minute = String(parsed.getMinutes()).padStart(2, "0");
  return `${weekday} ${day} ${month} ${year} ${hour}:${minute}`;
}

interface HandContextMenuState {
  x: number;
  y: number;
  handId: string;
}

function HandRowContextMenu({ menu, onClose }: { menu: HandContextMenuState | null; onClose: () => void }) {
  useEffect(() => {
    if (!menu) return;
    const close = () => onClose();
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    window.addEventListener("click", close);
    window.addEventListener("blur", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("blur", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu, onClose]);

  if (!menu) return null;
  return (
    <div className="dh-row-menu" style={{ left: menu.x, top: menu.y }} role="menu">
      <button type="button" role="menuitem" onClick={() => openViewHandWindow(menu.handId)}>View Hand</button>
      <button type="button" role="menuitem" onClick={() => openReplayHandWindow(menu.handId)}>Replay Hand</button>
    </div>
  );
}

function HandHistoryTable({ hands }: { hands: HandHistoryRecentHand[] }) {
  const [menu, setMenu] = useState<HandContextMenuState | null>(null);
  const openMenu = (x: number, y: number, handId: string) => setMenu({ x, y, handId });

  return (
    <div className="dh-table-wrap dh-history-wrap">
      <table className="dh-stats-table dh-history-table">
        <thead>
          <tr>
            <th>Date</th><th>Table</th><th>Cards</th><th>Preflop</th><th>Preflop Actions</th>
            <th>Flop</th><th>Turn</th><th>River</th><th>Board</th><th>Net Won</th><th>Position</th><th>Pot</th>
          </tr>
        </thead>
        <tbody>
          {hands.length === 0 ? <NoDataRow span={12} /> : hands.map((hand) => (
            <tr
              key={hand.hand_id}
              onContextMenu={(event) => {
                event.preventDefault();
                openMenu(event.clientX, event.clientY, hand.hand_id);
              }}
            >
              <td>{formatTimestamp(hand.timestamp)}</td>
              <td>{hand.table_name || "--"}</td>
              <td><CardRun text={hand.cards} /></td>
              <td><PreflopBadge labels={hand.preflop_situation} /></td>
              <td><ActionRun line={hand.preflop_line} /></td>
              <td><ActionRun line={hand.flop_line} /></td>
              <td><ActionRun line={hand.turn_line} /></td>
              <td><ActionRun line={hand.river_line} /></td>
              <td><CardRun text={hand.board} /></td>
              <td><MoneyText value={hand.net_won} /></td>
              <td>{hand.position || "--"}</td>
              <td>{formatChips(hand.pot)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <HandRowContextMenu menu={menu} onClose={() => setMenu(null)} />
    </div>
  );
}

function SettingsWorkspace() {
  return <div className="dh-workspace"><SettingsPanel /></div>;
}
