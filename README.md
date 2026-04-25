# WebSocket Reader

A desktop app built with Tauri (Rust + Web UI) that acts as a browser with a built-in WebSocket inspector. Open any website in a browser window and see all WebSocket traffic intercepted, decoded, and displayed in real time. Includes a **Scout mode** for Pokemon Vortex that auto-walks and alerts when a high-level Pokemon appears.

## Features

- **Browser windows** — Open any URL in a native webview (WKWebView on macOS)
- **WebSocket interception** — Monkey-patches `window.WebSocket` in every browser window to capture all traffic
- **Binary frame decoding** — Automatically decodes binary WebSocket frames:
  - UTF-8 text detection
  - Raw protobuf wire format decoding (varints, fixed32/64, length-delimited fields, nested messages)
  - Hex dump fallback for unknown formats
- **Scout mode** — Auto-walks in Pokemon Vortex, parses encounter protobuf messages, and stops when a Pokemon at level 40+ is found
- **Multi-window** — Open multiple browser windows; all WebSocket traffic is captured in a single inspector log

## How It Works

```
┌─────────────────────────────┐
│  Browser Window (external)  │
│                             │
│  Page JS: new WebSocket()   │
│       | (intercepted!)      │
│  Injected script wraps      │
│  every send/recv and calls  │──── IPC ────┐
│  Rust via Tauri invoke()    │             │
└─────────────────────────────┘             v
                                 ┌──────────────────┐
                                 │  Rust Backend     │
                                 │  report_ws_frame()│
                                 │       | emit()    │
                                 └────────┬─────────┘
                                          v
                                 ┌──────────────────┐
                                 │  Main Window      │
                                 │  [BROWSER WS]     │
                                 │  shows all frames │
                                 └──────────────────┘
```

1. You type a URL and click **Open** — a new native browser window opens (WKWebView on macOS)
2. Before the page loads, we inject a script that **monkey-patches `window.WebSocket`**
3. Every WebSocket the page creates is wrapped — we intercept `open`, `send`, `message`, `close`, and `error` events
4. Binary frames are decoded automatically (tries UTF-8 text, then raw protobuf, then hex dump)
5. Intercepted frames are sent back to Rust via Tauri's IPC (`window.__TAURI_INTERNALS__.invoke`)
6. Rust emits them as events to the main window, where they appear in the inspector log

## Scout Mode

Scout mode is designed for Pokemon Vortex. When activated:

1. The injected script **auto-walks** by simulating left/right arrow key presses every 250ms
2. Every incoming binary WebSocket frame is parsed for **Pokemon encounter messages** (protobuf message type 19)
3. The Pokemon's **name** (field 7) and **level** (field 3) are extracted
4. Every sighting is reported to the inspector log
5. When a Pokemon at **level 40 or higher** is found, scouting stops automatically and an alert is shown

```
┌─────────────────────────────────────────────┐
│  Browser Window (Pokemon Vortex)            │
│                                             │
│  Auto-walk: ← → ← → ← →                   │
│                                             │
│  WS message received (binary)               │
│       |                                     │
│  extractPokemon(bytes)                      │
│       |                                     │
│  {1:19, 2:{3:level, 7:"Name"}}             │
│       |                                     │
│  level >= 40? ──yes──> STOP + alert         │
│       |                                     │
│       no ──> keep walking                   │
└─────────────────────────────────────────────┘
```

## Prerequisites

- **Rust** (1.85+): https://rustup.rs (edition 2024 requires 1.85+)
- **Node.js** (18+): https://nodejs.org
- **macOS**: Xcode Command Line Tools (`xcode-select --install`)

## Setup

```bash
# Clone the repo
git clone https://github.com/zimehrabbasipt/websocket-reader.git
cd websocket-reader

# Install npm dependencies (Tauri CLI)
npm install

# Build and run in development mode
npm run dev
```

The first build downloads and compiles all Rust dependencies (~400 crates). Subsequent builds are fast (~3-5 seconds).

## Usage

1. Run `npm run dev` — the app window opens
2. Type a URL in the browser bar (e.g. `https://tradingview.com`) and click **Open**
3. A new browser window opens and loads the site
4. Any WebSocket traffic from that site appears in the **WebSocket Inspector** log in purple
5. Click **Scout** to start auto-walking and Pokemon detection (turns red with pulse animation while active)
6. Click **Scout** again (or wait for a level 40+ Pokemon) to stop
7. Click **Clear** to reset the log
8. Open multiple sites — all WebSocket traffic from all browser windows is captured

## Project Structure

```
websocket-reader/
├── package.json                  # npm scripts + Tauri CLI
├── ui/
│   ├── index.html                # Main window layout (browser bar + inspector)
│   ├── styles.css                # Dark theme (Catppuccin Mocha)
│   └── main.js                   # Frontend: open browser, scout toggle, event listeners
└── src-tauri/
    ├── Cargo.toml                # Rust dependencies
    ├── build.rs                  # Tauri build script
    ├── tauri.conf.json           # Window config, CSP, withGlobalTauri
    ├── capabilities/
    │   └── default.json          # IPC permissions (including remote page access)
    ├── icons/
    │   └── icon.png              # App icon
    ├── src/
    │   ├── main.rs               # Entry point
    │   └── lib.rs                # Core: WS interceptor, protobuf decoder, scout system
    └── bin/
        └── server.rs             # Echo WebSocket server for testing
```

## Key Files

- **`src-tauri/src/lib.rs`** — The core of the application. Contains:
  - `WS_INTERCEPTOR` — JavaScript injected into every browser window that monkey-patches `window.WebSocket`
  - Raw protobuf wire format decoder (`readVarint`, `readFixed32`, `readFixed64`, `decodeProtobuf`)
  - Pokemon encounter parser (`extractFields`, `extractPokemon`) — parses message type 19 for name and level
  - Scout system (`__startScout`, `__stopScout`, `checkScout`) — auto-walk + level detection
  - Binary frame decoder (`decodeBuffer`, `decodeData`) — UTF-8 -> protobuf -> hex fallback
  - Tauri commands: `open_browser`, `report_ws_frame`, `scout_found`, `toggle_scout`
- **`ui/main.js`** — Listens for `ws-intercepted` and `scout-found` events, manages scout toggle state
- **`ui/styles.css`** — Dark theme with color-coded messages (purple for WS frames, yellow for scout, green for found)
- **`src-tauri/capabilities/default.json`** — Grants IPC access to remote pages (`"remote": {"urls": ["https://*", "http://*"]}`) so the injected script can call back to Rust

## Echo Server (for testing)

An echo WebSocket server is included for local testing:

```bash
# Terminal 1: start the echo server
cd src-tauri && cargo run --bin server

# Terminal 2: run the app
npm run dev
```

The echo server listens on `ws://127.0.0.1:9001` and sends back whatever you send it.

## Production Build

```bash
npm run build
```

This produces a `.app` bundle in `src-tauri/target/release/bundle/macos/`.
