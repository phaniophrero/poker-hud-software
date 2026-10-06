// "View Hand" - a separate window (see `handWindows.ts`) opened from the
// hand-history table's right-click menu, showing one hand as a plain
// street-by-street text log: seats/stacks, dealt cards, then each street's
// actions as one combined line with the pot at the start of that street -
// the same layout DriveHud's own "View Hand" window uses.
//
// Deliberately does not show an equity/win% column next to each street the
// way DriveHud does: that number is only honest when every live opponent's
// hole cards are known, which is true at showdown and essentially never
// otherwise - see ARCHITECTURE.md §0 on this app's rule against a number
// that looks solved but isn't. `results` (the "X won $Y" line) is shown
// instead, since that's a fact taken straight from the hand text.

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CardRun, formatChips, formatTimestamp } from "./App";
import type { HandDetail, HandReplayStep } from "./types";

interface StreetGroup {
  street: string;
  potAtStart: number | null;
  board: string;
  lines: string[];
}

function groupByStreet(replay: HandReplayStep[]): StreetGroup[] {
  const groups: StreetGroup[] = [];
  let lastPot = 0;
  for (const step of replay) {
    let group = groups[groups.length - 1];
    if (!group || group.street !== step.street) {
      group = { street: step.street, potAtStart: step.street === "PREFLOP" ? null : lastPot, board: step.board, lines: [] };
      groups.push(group);
    }
    group.lines.push(step.description);
    group.board = step.board;
    lastPot = step.pot;
  }
  return groups;
}

export default function ViewHandWindow({ handId }: { handId: string }) {
  const [detail, setDetail] = useState<HandDetail | null>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "missing">("loading");

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

  if (status === "loading") {
    return (
      <div className="dh-shell dh-hand-window">
        <div className="dh-hand-window-empty">Se încarcă mâna #{handId}...</div>
      </div>
    );
  }

  if (status === "missing" || !detail) {
    return (
      <div className="dh-shell dh-hand-window">
        <div className="dh-hand-window-empty">
          Mâna #{handId} nu a fost găsită. Poate a ieșit din cache-ul de mâini recente - reimportă folderul de Hand
          History din Settings.
        </div>
      </div>
    );
  }

  const groups = groupByStreet(detail.replay);

  return (
    <div className="dh-shell dh-hand-window">
      <div className="dh-hand-window-header">
        <strong>
          {detail.game_type ?? "Hold'em No Limit"} {detail.stakes ? `(${detail.stakes})` : ""}
        </strong>
        <span>{formatTimestamp(detail.timestamp)}</span>
        <span className="dh-muted-text">
          {detail.table_name} · Mâna #{detail.hand_id}
        </span>
      </div>

      <div className="dh-hand-window-seats">
        {detail.players.map((player) => (
          <div key={player.seat} className={player.is_hero ? "dh-hand-seat dh-hand-seat-hero" : "dh-hand-seat"}>
            <span className="dh-hand-seat-position">{player.position ?? `Seat ${player.seat}`}</span>
            <span className="dh-hand-seat-name">{player.is_hero ? "Hero" : player.position ?? player.name}</span>
            <span className="dh-hand-seat-stack">
              {formatChips(player.starting_stack)}
              {detail.big_blind ? ` (${((player.starting_stack ?? 0) / detail.big_blind).toFixed(1)} BB)` : ""}
            </span>
            {player.is_button && <span className="dh-hand-seat-button">BTN</span>}
          </div>
        ))}
      </div>

      {detail.hero_cards && detail.hero_cards !== "-" && (
        <div className="dh-hand-window-dealt">
          <span>Dealt to Hero</span>
          <CardRun text={detail.hero_cards} />
        </div>
      )}

      <div className="dh-hand-window-streets">
        {groups.map((group, index) => (
          <div key={`${group.street}-${index}`} className="dh-hand-street">
            <div className="dh-hand-street-header">
              <span>{group.street}</span>
              {group.potAtStart !== null && <span className="dh-hand-street-pot">{formatChips(group.potAtStart)}</span>}
              {group.board !== "-" && <CardRun text={group.board} />}
            </div>
            <p className="dh-hand-street-line">{group.lines.join(", ")}</p>
          </div>
        ))}
      </div>

      {detail.results.length > 0 && (
        <div className="dh-hand-window-results">
          {detail.results.map((line) => (
            <p key={line}>{line}</p>
          ))}
        </div>
      )}
    </div>
  );
}
