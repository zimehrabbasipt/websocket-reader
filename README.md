# WebSocket Reader

A desktop app built with Tauri (Rust + Web UI) that acts as a browser with a built-in WebSocket inspector. Open any website in a browser window and see all WebSocket traffic intercepted and displayed in real time.

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
4. Intercepted frames are sent back to Rust via Tauri's IPC (`window.__TAURI_INTERNALS__.invoke`)
5. Rust emits them as events to the main window, where they appear in the inspector log

## Prerequisites

- **Rust** (1.77.2+): https://rustup.rs
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
5. Click **Clear** to reset the log
6. Open multiple sites — all WebSocket traffic from all browser windows is captured

## Project Structure

```
websocket-reader/
├── package.json                  # npm scripts + Tauri CLI
├── ui/
│   ├── index.html                # Main window layout (browser bar + inspector)
│   ├── styles.css                # Dark theme (Catppuccin Mocha)
│   └── main.js                   # Frontend: open browser, listen for intercepted frames
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
    │   └── lib.rs                # Core: open_browser, report_ws_frame, WS_INTERCEPTOR
    └── bin/
        └── server.rs             # Echo server for testing
```

## Echo Server (for testing)

An echo WebSocket server is included for local testing:

```bash
# Terminal 1: start the echo server
cd src-tauri && cargo run --bin server

# Terminal 2: run the app
npm run dev
```

The echo server listens on `ws://127.0.0.1:9001` and sends back whatever you send it.

## Key Files

- **`src-tauri/src/lib.rs`** — Contains the `WS_INTERCEPTOR` script (injected into browser windows) and two Tauri commands: `open_browser` and `report_ws_frame`
- **`ui/main.js`** — Listens for `ws-intercepted` events and displays them in the log
- **`src-tauri/capabilities/default.json`** — Grants IPC access to remote pages (`"remote": {"urls": ["https://*", "http://*"]}`) so the injected script can call back to Rust

## Production Build

```bash
npm run build
```

This produces a `.app` bundle in `src-tauri/target/release/bundle/macos/`.
