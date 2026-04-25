use std::sync::atomic::{AtomicU32, Ordering};
use tauri::{AppHandle, Emitter, WebviewUrl, WebviewWindowBuilder};

static BROWSER_COUNTER: AtomicU32 = AtomicU32::new(0);

// This script gets injected into every browser window BEFORE the page loads.
// It monkey-patches the WebSocket constructor so we can intercept all frames.
const WS_INTERCEPTOR: &str = r#"
(function() {
    const OrigWS = window.WebSocket;

    function InterceptedWebSocket(url, protocols) {
        const ws = (protocols !== undefined)
            ? new OrigWS(url, protocols)
            : new OrigWS(url);

        const report = (frameType, data) => {
            try {
                if (window.__TAURI_INTERNALS__) {
                    window.__TAURI_INTERNALS__.invoke('report_ws_frame', {
                        frameType: frameType,
                        wsUrl: url,
                        data: data
                    });
                }
            } catch(e) {}
        };

        ws.addEventListener('open', () => report('OPEN', 'connected'));

        ws.addEventListener('message', (e) => {
            let d = typeof e.data === 'string' ? e.data : '[binary]';
            if (d.length > 500) d = d.substring(0, 500) + '...';
            report('RECV', d);
        });

        ws.addEventListener('close', (e) => report('CLOSE', e.reason || 'closed'));

        ws.addEventListener('error', () => report('ERROR', 'connection error'));

        const origSend = ws.send.bind(ws);
        ws.send = function(data) {
            let d = typeof data === 'string' ? data : '[binary]';
            if (d.length > 500) d = d.substring(0, 500) + '...';
            report('SEND', d);
            return origSend(data);
        };

        return ws;
    }

    InterceptedWebSocket.prototype = OrigWS.prototype;
    InterceptedWebSocket.CONNECTING = 0;
    InterceptedWebSocket.OPEN = 1;
    InterceptedWebSocket.CLOSING = 2;
    InterceptedWebSocket.CLOSED = 3;

    window.WebSocket = InterceptedWebSocket;
})();
"#;

#[tauri::command]
async fn open_browser(url: String, app: AppHandle) -> Result<(), String> {
    let id = BROWSER_COUNTER.fetch_add(1, Ordering::Relaxed);
    let label = format!("browser-{id}");

    let external_url = url.parse::<tauri::Url>().map_err(|e| format!("Invalid URL: {e}"))?;

    WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(external_url))
        .title(url)
        .inner_size(1024.0, 768.0)
        .initialization_script(WS_INTERCEPTOR)
        .build()
        .map_err(|e| format!("Failed to open browser: {e}"))?;

    Ok(())
}

#[tauri::command]
async fn report_ws_frame(
    frame_type: String,
    ws_url: String,
    data: String,
    app: AppHandle,
) -> Result<(), String> {
    let payload = format!("[{}] {} | {}", frame_type, ws_url, data);
    app.emit("ws-intercepted", payload)
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![open_browser, report_ws_frame])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
