import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { HandHistoryStatus } from "./types";
import "./styles.css";

const HAND_HISTORY_POLL_INTERVAL_MS = 1200;

export default function SettingsPanel() {
  const [handHistoryStatus, setHandHistoryStatus] = useState<HandHistoryStatus | null>(null);
  const [handHistoryFolder, setHandHistoryFolder] = useState("");
  const [message, setMessage] = useState<string | null>(null);

  async function refreshHandHistoryStatus() {
    const status = await invoke<HandHistoryStatus>("get_hand_history_status");
    setHandHistoryStatus(status);
    if (!handHistoryFolder && status.folder) setHandHistoryFolder(status.folder);
  }

  useEffect(() => {
    refreshHandHistoryStatus().catch(() => undefined);
    const interval = setInterval(() => refreshHandHistoryStatus().catch(() => undefined), HAND_HISTORY_POLL_INTERVAL_MS);
    return () => clearInterval(interval);
  }, []);

  async function saveHandHistoryFolder() {
    if (!handHistoryFolder.trim()) return;
    setMessage(null);
    try {
      await invoke("set_hand_history_folder", { folder: handHistoryFolder.trim() });
      await refreshHandHistoryStatus();
      setMessage("Folderul de HandHistory a fost salvat.");
    } catch (error) {
      setMessage(`Nu am putut salva folderul: ${String(error)}`);
    }
  }

  async function rescanHandHistory() {
    setMessage(null);
    try {
      await invoke("rescan_hand_history_folder");
      await refreshHandHistoryStatus();
      setMessage("Rescan terminat.");
    } catch (error) {
      setMessage(`Rescan eșuat: ${String(error)}`);
    }
  }

  function formatHHTime(ms: number | null): string {
    if (ms === null) return "-";
    const date = new Date(ms);
    return `${date.toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    })}.${String(date.getMilliseconds()).padStart(3, "0")}`;
  }

  return (
    <div className="settings-panel">
      <h1>SoftPoker Tracker</h1>
      <p className="settings-subtitle">
        Varianta aceasta foloseste doar fisierele HandHistory salvate de PokerStars pentru import si analiza offline.
      </p>

      <section>
        <h2>PokerStars data source</h2>
        <div className="hh-panel">
          <div className="settings-row">
            <span>Hand History</span>
            <span className={handHistoryStatus?.watcher_active ? "status-ok" : "status-dim"}>
              {handHistoryStatus?.watcher_active ? "watcher active" : "watcher inactive"}
            </span>
          </div>
          <label className="settings-row hh-folder-row">
            <span>Folder</span>
            <input
              type="text"
              value={handHistoryFolder}
              placeholder="PokerStars HandHistory folder"
              onChange={(event) => setHandHistoryFolder(event.target.value)}
            />
          </label>
          <div className="settings-row">
            <span>Controls</span>
            <span className="hh-actions">
              <button className="settings-button" onClick={saveHandHistoryFolder}>Set Folder</button>
              <button className="settings-button" onClick={rescanHandHistory}>Rescan</button>
            </span>
          </div>
          {message && <p className="settings-note">{message}</p>}
          {handHistoryStatus?.detected_candidates.length ? (
            <p className="settings-note">Detectat posibil: {handHistoryStatus.detected_candidates.join(" | ")}</p>
          ) : (
            <p className="settings-note settings-warning">
              Nu am găsit automat folder HandHistory. Pune manual calea din PokerStars Settings - Playing History.
            </p>
          )}
          <div className="hh-grid">
            <span>Folder valid</span><strong>{handHistoryStatus?.folder_valid ? "YES" : "UNKNOWN"}</strong>
            <span>Current file</span><strong>{handHistoryStatus?.current_file ?? "-"}</strong>
            <span>Current table</span><strong>{handHistoryStatus?.current_table ?? "-"}</strong>
            <span>Hand</span><strong>{handHistoryStatus?.current_hand ?? "-"}</strong>
            <span>Hero</span><strong>{handHistoryStatus?.hero ?? "-"}</strong>
            <span>Cards</span><strong>{handHistoryStatus?.hero_cards ?? "-"}</strong>
            <span>Board</span><strong>{handHistoryStatus?.board ?? "-"}</strong>
            <span>Street</span><strong>{handHistoryStatus?.street ?? "-"}</strong>
            <span>Last HH update</span><strong>{formatHHTime(handHistoryStatus?.last_update ?? null)}</strong>
          </div>
          <h3>HH debug log</h3>
          <div className="hh-log">
            {handHistoryStatus?.events.length ? handHistoryStatus.events.map((entry) => (
              <div key={`${entry.timestamp_ms}-${entry.message}`}>
                <span>{formatHHTime(entry.timestamp_ms)}</span>
                <strong>{entry.message}</strong>
              </div>
            )) : <p className="settings-note">Aștept evenimente din hand history...</p>}
          </div>
        </div>
      </section>
    </div>
  );
}
