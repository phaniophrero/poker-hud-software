// "Replay Hand" - a separate window (see `handWindows.ts`) opened from the
// hand-history table's right-click menu. Steps through the one hand that
// was right-clicked, action by action, on a small visual table - prev/next/
// play/pause plus P/F/T/R street jumps, mirroring Drivetracker's own replayer
// window for a single hand (Drivetracker's replayer can *also* scrub across an
// entire session's worth of hands; that whole-session browsing is a
// separate, bigger feature and isn't built here - this opens directly on
// the hand that was right-clicked).

import { useEffect, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CardRun, formatChips, formatTimestamp } from "./App";
import type { HandDetail, HandDetailPlayer, HandReplaySeat } from "./types";

type ReplaySeatLike = Pick<
  HandReplaySeat,
  "seat" | "name" | "position" | "stack" | "bet_this_street" | "folded" | "is_hero" | "is_button" | "is_actor" | "cards"
>;

function startingSeats(players: HandDetailPlayer[], heroCards: string): ReplaySeatLike[] {
  return players.map((p) => ({
    seat: p.seat,
    name: p.name,
    position: p.position,
    stack: p.starting_stack ?? 0,
    bet_this_street: 0,
    folded: false,
    is_hero: p.is_hero,
    is_button: p.is_button,
    is_actor: false,
    cards: p.is_hero ? heroCards : "-",
  }));
}

// Rotates the seat-sorted list so hero is first, keeping the rest in their
// real seating order - the same "hero fixed, everyone else rotates around
// them" convention `PokerTable.tsx` uses for the table viewer.
function orderFromHero<T extends { is_hero: boolean }>(seats: T[]): T[] {
  const heroIndex = seats.findIndex((s) => s.is_hero);
  if (heroIndex <= 0) return seats;
  return [...seats.slice(heroIndex), ...seats.slice(0, heroIndex)];
}

function seatStyle(index: number, total: number): CSSProperties {
  // Hero (index 0) sits at the bottom (90deg); the rest spread clockwise.
  const angle = (index / total) * 2 * Math.PI + Math.PI / 2;
  const rx = 40;
  const ry = 36;
  const left = 50 + rx * Math.cos(angle);
  const top = 50 + ry * Math.sin(angle);
  return { left: `${left}%`, top: `${top}%` };
}

const STREETS = ["PREFLOP", "FLOP", "TURN", "RIVER"];
const PLAYBACK_MS = 1100;

export default function ReplayHandWindow({ handId }: { handId: string }) {
  const [detail, setDetail] = useState<HandDetail | null>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "missing">("loading");
  const [stepIndex, setStepIndex] = useState(-1);
  const [playing, setPlaying] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let attempts = 0;

    async function load() {
      attempts += 1;
      try {
        const result = await invoke<HandDetail | null>("get_hand_detail", { handId });
        if (cancelled) return;
        if (result) {
          setDetail(result);
          setStatus("ready");
        } else if (attempts < 5) {
          window.setTimeout(load, 500);
        } else {
          setStatus("missing");
        }
      } catch {
        if (!cancelled) setStatus("missing");
      }
    }

    load();
    return () => {
      cancelled = true;
    };
  }, [handId]);

  useEffect(() => {
    if (!playing || !detail) return;
    if (stepIndex >= detail.replay.length - 1) {
      setPlaying(false);
      return;
    }
    const timer = window.setTimeout(() => {
      setStepIndex((current) => Math.min(current + 1, detail.replay.length - 1));
    }, PLAYBACK_MS);
    return () => window.clearTimeout(timer);
  }, [playing, stepIndex, detail]);

  if (status === "loading") {
    return (
      <div className="dh-shell dh-replay-window">
        <div className="dh-hand-window-empty">Se încarcă mâna #{handId}...</div>
      </div>
    );
  }

  if (status === "missing" || !detail) {
    return (
      <div className="dh-shell dh-replay-window">
        <div className="dh-hand-window-empty">
          Mâna #{handId} nu a fost găsită. Poate a ieșit din cache-ul de mâini recente - reimportă folderul de Hand
          History din Settings.
        </div>
      </div>
    );
  }

  const steps = detail.replay;
  const current = stepIndex >= 0 ? steps[stepIndex] : null;
  const seats = orderFromHero(current ? current.seats : startingSeats(detail.players, detail.hero_cards));
  const board = current ? current.board : "-";
  const pot = current ? current.pot : 0;
  const street = current ? current.street : "PREFLOP";
  const atStart = stepIndex <= -1;
  const atEnd = stepIndex >= steps.length - 1;

  function jumpToStreet(target: string) {
    const index = steps.findIndex((step) => step.street === target);
    if (index !== -1) {
      setPlaying(false);
      setStepIndex(index);
    }
  }

  return (
    <div className="dh-shell dh-replay-window">
      <div className="dh-replay-header">
        <strong>{detail.table_name || "Masă"}</strong>
        <span className="dh-muted-text">
          Mâna #{detail.hand_id} · {formatTimestamp(detail.timestamp)}
        </span>
      </div>

      <div className="dh-replay-felt">
        {seats.map((seat, index) => (
          <div
            key={seat.seat}
            className={[
              "dh-replay-seat",
              seat.is_actor ? "dh-replay-seat-active" : "",
              seat.folded ? "dh-replay-seat-folded" : "",
            ].join(" ").trim()}
            style={seatStyle(index, seats.length)}
          >
            <div className="dh-replay-seat-title">
              {seat.is_hero && <span className="dh-hero-tag">Hero</span>}
              <span>{seat.position ?? `Seat ${seat.seat}`}</span>
              {seat.is_button && <span className="dh-replay-button-dot" title="Button" />}
            </div>
            {seat.cards !== "-" && <CardRun text={seat.cards} />}
            <div className="dh-replay-seat-stack">{formatChips(seat.stack)}</div>
            {seat.bet_this_street > 0 && <div className="dh-replay-seat-bet">bet {formatChips(seat.bet_this_street)}</div>}
            {seat.folded && <div className="dh-replay-seat-fold-label">Fold</div>}
          </div>
        ))}
        <div className="dh-replay-center">
          {board !== "-" && <CardRun text={board} />}
          <div className="dh-replay-pot">Pot {formatChips(pot)}</div>
        </div>
      </div>

      <div className="dh-replay-log">{current ? current.description : "Începutul mâinii - nicio acțiune încă"}</div>

      <div className="dh-replay-controls">
        <div className="dh-replay-street-tabs">
          {STREETS.map((label) => (
            <button
              key={label}
              type="button"
              className={street === label ? "active" : ""}
              disabled={!steps.some((step) => step.street === label)}
              onClick={() => jumpToStreet(label)}
            >
              {label[0]}
            </button>
          ))}
        </div>
        <div className="dh-replay-buttons">
          <button type="button" disabled={atStart} title="Start" onClick={() => { setPlaying(false); setStepIndex(-1); }}>
            |&lt;
          </button>
          <button
            type="button"
            disabled={atStart}
            title="Pas anterior"
            onClick={() => { setPlaying(false); setStepIndex((i) => Math.max(-1, i - 1)); }}
          >
            &lt;
          </button>
          <button type="button" title={playing ? "Pauză" : "Redă"} onClick={() => setPlaying((p) => !p)} disabled={atEnd && !playing}>
            {playing ? "❚❚" : "▶"}
          </button>
          <button
            type="button"
            disabled={atEnd}
            title="Pas următor"
            onClick={() => { setPlaying(false); setStepIndex((i) => Math.min(steps.length - 1, i + 1)); }}
          >
            &gt;
          </button>
          <button type="button" disabled={atEnd} title="Sfârșit" onClick={() => { setPlaying(false); setStepIndex(steps.length - 1); }}>
            &gt;|
          </button>
        </div>
        <div className="dh-replay-progress">
          {stepIndex + 2} / {steps.length + 1}
        </div>
      </div>

      {atEnd && detail.results.length > 0 && (
        <div className="dh-hand-window-results">
          {detail.results.map((line) => (
            <p key={line}>{line}</p>
          ))}
        </div>
      )}
    </div>
  );
}
