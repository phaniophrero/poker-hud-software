import { useId, useState } from "react";
import type { HandHistoryStats } from "./types";

const GAUGES = [
  { key: "went_to_showdown_percent", label: "WTSD", color: "#f59a52", x: 84, y: 179, r: 42,
    description: "Procentul mâinilor în care ai ajuns la arătarea cărților, dintre mâinile în care ai văzut flopul." },
  { key: "three_bet_percent", label: "3Bet%", color: "#d573dd", x: 656, y: 179, r: 42, reverse: true,
    description: "Procentul ocaziilor în care ai făcut prima replusare înainte de flop, după o plusare a unui adversar." },
  { key: "won_at_showdown_percent", label: "W$SD", color: "#b07cff", x: 151, y: 155, r: 60,
    description: "Procentul mâinilor ajunse la arătarea cărților în care ai câștigat cel puțin o parte din pot." },
  { key: "won_when_saw_flop_percent", label: "WWSF%", color: "#f36672", x: 589, y: 155, r: 60, reverse: true,
    description: "Procentul mâinilor în care ai câștigat cel puțin o parte din pot, dintre mâinile în care ai văzut flopul." },
  { key: "pfr_percent", label: "PFR", color: "#48d779", x: 240, y: 130, r: 81,
    description: "Procentul mâinilor în care ai plusat înainte de flop. Egalarea unei mize, inclusiv cu toate fisele, nu este o plusare." },
  { key: "aggression_percent", label: "AGG%", color: "#51d7ef", x: 500, y: 130, r: 81, reverse: true,
    description: "Frecvența acțiunilor agresive: pariuri și plusări împărțite la totalul pariurilor, plusărilor, egalărilor și pasurilor, înmulțit cu 100." },
  { key: "vpip_percent", label: "VPIP", color: "#5d89ff", x: 370, y: 110, r: 101,
    description: "Procentul mâinilor în care ai pus voluntar bani în pot înainte de flop. Blindurile obligatorii nu se includ." },
] as const;

function point(radius: number, angle: number) {
  const radians = angle * Math.PI / 180;
  return { x: radius * Math.sin(radians), y: -radius * Math.cos(radians) };
}

function arc(radius: number, start: number, end: number) {
  const a = point(radius, start);
  const b = point(radius, end);
  return `M ${a.x} ${a.y} A ${radius} ${radius} 0 ${Math.abs(end - start) > 180 ? 1 : 0} ${end > start ? 1 : 0} ${b.x} ${b.y}`;
}

export default function GaugeCluster({ stats }: { stats: HandHistoryStats | null }) {
  const [active, setActive] = useState<number | null>(null);
  const tooltipId = useId();
  return (
    <div className="dh-gauge-cluster" onMouseLeave={() => setActive(null)}>
      <svg viewBox="0 0 740 225" role="group" aria-label="Statisticile de joc" className="dh-gauge-instruments">
        {GAUGES.map((gauge, index) => {
          const raw = stats?.[gauge.key];
          const value = typeof raw === "number" && Number.isFinite(raw) ? raw : null;
          const numeric = Math.max(0, Math.min(100, value ?? 0));
          const reverse = "reverse" in gauge && gauge.reverse;
          const start = reverse ? 120 : -120;
          const end = start + (reverse ? -1 : 1) * numeric * 2.4;
          const large = gauge.key === "vpip_percent";
          const textX = gauge.r < 65 ? (reverse ? 10 : -10) : 0;
          const text = value === null ? "--" : value.toFixed(1);
          return (
            <g key={gauge.key} transform={`translate(${gauge.x} ${gauge.y})`}
              className="dh-gauge-instrument" tabIndex={0} role="img"
              aria-label={`${gauge.label}: ${text}${value === null ? "" : "%"}. ${gauge.description}`}
              aria-describedby={active === index ? tooltipId : undefined}
              onMouseEnter={() => setActive(index)} onFocus={() => setActive(index)}
              onBlur={() => setActive(null)} onKeyDown={(event) => { if (event.key === "Escape") setActive(null); }}>
              <path d={`${arc(gauge.r + 5, -120, 120)} Z`} fill="#101425" />
              <path d={arc(gauge.r, -120, 120)} fill="none" stroke="#242a3d" strokeWidth="2" />
              {value !== null && numeric > 0 && <path d={arc(gauge.r, start, end)} fill="none" stroke={gauge.color} strokeWidth="2.5" className="dh-gauge-glow" />}
              {Array.from({ length: 41 }, (_, tick) => {
                const angle = -120 + tick * 6;
                const a = point(gauge.r - 5, angle);
                const b = point(gauge.r - (tick % 5 === 0 ? 15 : 10), angle);
                const colored = value !== null && (reverse ? 100 - tick * 2.5 : tick * 2.5) <= numeric;
                return <line key={tick} x1={a.x} y1={a.y} x2={b.x} y2={b.y}
                  stroke={colored ? gauge.color : "#2b3145"} strokeWidth={tick % 5 === 0 ? 2 : 1.5} />;
              })}
              {large && <>
                <circle r="49" fill="none" stroke="#2b3145" strokeWidth="3" />
                {[0, 25, 50, 75, 100].map((tick) => {
                  const p = point(71, -120 + tick * 2.4);
                  return <text key={tick} x={p.x} y={p.y} textAnchor="middle" dominantBaseline="middle" className="dh-gauge-scale">{tick}</text>;
                })}
              </>}
              <text x={textX} textAnchor="middle" y={large ? 5 : 3} className={`dh-gauge-value ${large ? "dh-gauge-value-main" : gauge.r < 65 ? "dh-gauge-value-small" : ""}`}>{text}</text>
              <text x={textX} textAnchor="middle" y={large ? 23 : gauge.r < 65 ? 16 : 22} className={gauge.r < 65 ? "dh-gauge-label-small" : "dh-gauge-label"}>{gauge.label}</text>
            </g>
          );
        })}
      </svg>
      {active !== null && <div id={tooltipId} role="tooltip" className="dh-gauge-tooltip">
        <strong>{GAUGES[active].label}</strong> {GAUGES[active].description}
        <span>0,0 = nicio apariție în situațiile eligibile. -- = încă nu există situații eligibile.</span>
      </div>}
    </div>
  );
}
