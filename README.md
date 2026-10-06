# SoftPoker Tracker

SoftPoker Tracker is a local desktop app for reviewing PokerStars hand-history
files after they have been written by the client.

This compliant build intentionally does not include:

- screen capture or OCR
- live card reading
- table overlays
- real-time advice
- mouse/keyboard automation
- automatic advice or GTO strategy modules

The app keeps only offline tracking features: Cash, Zoom, Tournament history,
MDA dashboards, JSON backup/import, and hand detail/replay windows based on
saved hand-history text.

## Development

```bash
npm install
npm install --prefix frontend
npm run build --prefix frontend
cargo check
```

Build the Windows app with:

```bash
npm run tauri -- build
```
