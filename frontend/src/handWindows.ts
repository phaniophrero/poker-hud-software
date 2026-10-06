// Opens "View Hand" / "Replay Hand" as their own separate OS windows, the
// same way DriveHud does from its hand-history table's right-click menu
// (a real ask: "Hand History si Hand Replay sunt doua ferestre separate,
// nu sunt in aceeasi fereastra"). Both windows load the same `index.html`
// bundle every window in this app already shares with a
// `?window=...&hand=...` query string, and
// `App.tsx` reads that to decide which component to render instead of the
// normal dashboard.
//
// One Tauri window label per hand id (not one shared window whose content
// swaps) so a user can have several different hands' View/Replay windows
// open side by side at once, exactly like DriveHud.

import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

// Tauri window labels only allow [A-Za-z0-9-_/:.]. Hand ids are normally
// plain digits, but this keeps a stray character from making the window
// fail to create instead of silently doing nothing.
function sanitizeForLabel(handId: string): string {
  const cleaned = handId.replace(/[^A-Za-z0-9_-]/g, "_");
  return cleaned.length > 0 ? cleaned : "unknown";
}

async function openHandWindow(kind: "view-hand" | "replay-hand", handId: string, title: string) {
  const label = `${kind === "view-hand" ? "view_hand" : "replay_hand"}_${sanitizeForLabel(handId)}`;
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    await existing.setFocus();
    return;
  }
  const url = `index.html?window=${kind}&hand=${encodeURIComponent(handId)}`;
  const win = new WebviewWindow(label, {
    url,
    title,
    width: kind === "replay-hand" ? 1000 : 720,
    height: kind === "replay-hand" ? 760 : 820,
    minWidth: 560,
    minHeight: 480,
    resizable: true,
    decorations: true,
  });
  win.once("tauri://error", (event) => {
    console.error(`Nu am putut deschide fereastra ${kind} pentru mana ${handId}`, event);
  });
}

export function openViewHandWindow(handId: string) {
  void openHandWindow("view-hand", handId, `SoftPoker Tracker - Mana #${handId}`);
}

export function openReplayHandWindow(handId: string) {
  void openHandWindow("replay-hand", handId, `SoftPoker Tracker - Replay #${handId}`);
}
