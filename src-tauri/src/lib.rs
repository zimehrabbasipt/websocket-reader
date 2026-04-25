use std::sync::atomic::{AtomicU32, Ordering};
use tauri::{AppHandle, Emitter, WebviewUrl, WebviewWindowBuilder};

static BROWSER_COUNTER: AtomicU32 = AtomicU32::new(0);

// This script gets injected into every browser window BEFORE the page loads.
// It monkey-patches the WebSocket constructor so we can intercept all frames.
const WS_INTERCEPTOR: &str = r#"
(function() {
    const OrigWS = window.WebSocket;

    // Convert an ArrayBuffer to a decoded string.
    // Tries UTF-8 first. If > 20% of chars are control chars, falls back to hex.
    function decodeBuffer(buf) {
        const bytes = new Uint8Array(buf);
        const len = bytes.length;
        if (len === 0) return '[empty binary]';

        // Try UTF-8 decode
        try {
            const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
            // Check if it looks like readable text (not garbled binary)
            let controlCount = 0;
            for (let i = 0; i < Math.min(text.length, 200); i++) {
                const c = text.charCodeAt(i);
                if (c < 32 && c !== 9 && c !== 10 && c !== 13) controlCount++;
            }
            if (controlCount < text.length * 0.2) {
                // Looks like text
                if (text.length > 500) return '(text ' + len + 'B) ' + text.substring(0, 500) + '...';
                return '(text ' + len + 'B) ' + text;
            }
        } catch(e) {}

        // Fall back to hex dump (show first 128 bytes)
        const limit = Math.min(len, 128);
        let hex = '';
        for (let i = 0; i < limit; i++) {
            hex += bytes[i].toString(16).padStart(2, '0') + ' ';
        }
        if (len > limit) hex += '...';
        return '(bin ' + len + 'B) ' + hex.trim();
    }

    // Decode any data type (string, ArrayBuffer, Blob, TypedArray)
    // Returns a promise that resolves to a display string.
    async function decodeData(data) {
        if (typeof data === 'string') {
            if (data.length > 500) return data.substring(0, 500) + '...';
            return data;
        }
        if (data instanceof ArrayBuffer) {
            return decodeBuffer(data);
        }
        if (data instanceof Blob) {
            const buf = await data.arrayBuffer();
            return decodeBuffer(buf);
        }
        if (ArrayBuffer.isView(data)) {
            return decodeBuffer(data.buffer);
        }
        return '[unknown type]';
    }

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
            decodeData(e.data).then(d => report('RECV', d));
        });

        ws.addEventListener('close', (e) => report('CLOSE', e.reason || 'closed'));

        ws.addEventListener('error', () => report('ERROR', 'connection error'));

        const origSend = ws.send.bind(ws);
        ws.send = function(data) {
            decodeData(data).then(d => report('SEND', d));
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
