use std::sync::atomic::{AtomicU32, Ordering};
use tauri::{AppHandle, Emitter, WebviewUrl, WebviewWindowBuilder};

static BROWSER_COUNTER: AtomicU32 = AtomicU32::new(0);

// This script gets injected into every browser window BEFORE the page loads.
// It monkey-patches the WebSocket constructor so we can intercept all frames.
const WS_INTERCEPTOR: &str = r#"
(function() {
    const OrigWS = window.WebSocket;

    // --- Raw Protobuf Decoder ---
    // Decodes protobuf wire format without a schema.
    // Shows field numbers, types, and values.

    function readVarint(bytes, offset) {
        let result = 0, shift = 0;
        while (offset < bytes.length) {
            const b = bytes[offset++];
            result |= (b & 0x7f) << shift;
            if ((b & 0x80) === 0) return { value: result >>> 0, offset: offset };
            shift += 7;
            if (shift > 35) return null; // too large for safe int
        }
        return null;
    }

    function readFixed32(bytes, offset) {
        if (offset + 4 > bytes.length) return null;
        const buf = new ArrayBuffer(4);
        const view = new DataView(buf);
        for (let i = 0; i < 4; i++) view.setUint8(i, bytes[offset + i]);
        return { value: view.getFloat32(0, true), offset: offset + 4 };
    }

    function readFixed64(bytes, offset) {
        if (offset + 8 > bytes.length) return null;
        const buf = new ArrayBuffer(8);
        const view = new DataView(buf);
        for (let i = 0; i < 8; i++) view.setUint8(i, bytes[offset + i]);
        return { value: view.getFloat64(0, true), offset: offset + 8 };
    }

    function decodeProtobuf(bytes, offset, end) {
        const fields = [];
        while (offset < end) {
            const tag = readVarint(bytes, offset);
            if (!tag) break;
            offset = tag.offset;
            const fieldNum = tag.value >>> 3;
            const wireType = tag.value & 0x7;
            if (fieldNum === 0 || fieldNum > 536870911) return null; // invalid

            if (wireType === 0) { // varint
                const v = readVarint(bytes, offset);
                if (!v) return null;
                offset = v.offset;
                fields.push(fieldNum + ':' + v.value);
            } else if (wireType === 1) { // 64-bit
                const v = readFixed64(bytes, offset);
                if (!v) return null;
                offset = v.offset;
                const f = v.value;
                if (Number.isFinite(f) && Math.abs(f) > 0.0001 && Math.abs(f) < 1e15) {
                    fields.push(fieldNum + ':' + parseFloat(f.toFixed(4)) + 'd');
                } else {
                    fields.push(fieldNum + ':0x' + Array.from(bytes.slice(offset-8, offset)).map(b => b.toString(16).padStart(2,'0')).join(''));
                }
            } else if (wireType === 2) { // length-delimited (string, bytes, or nested msg)
                const lenV = readVarint(bytes, offset);
                if (!lenV || lenV.offset + lenV.value > end) return null;
                offset = lenV.offset;
                const chunk = bytes.slice(offset, offset + lenV.value);
                offset += lenV.value;

                // Try as UTF-8 string FIRST — short printable text is almost
                // certainly a string, not a nested message. Without this,
                // names like "Ekans" get misread as protobuf because 0x45 ('E')
                // happens to be a valid tag (field 8, wire type 5).
                let isString = false;
                try {
                    const text = new TextDecoder('utf-8', { fatal: true }).decode(chunk);
                    let printable = true;
                    for (let i = 0; i < text.length; i++) {
                        const c = text.charCodeAt(i);
                        if (c < 32 && c !== 9 && c !== 10 && c !== 13) { printable = false; break; }
                    }
                    if (printable && text.length > 0) {
                        fields.push(fieldNum + ':"' + text + '"');
                        isString = true;
                    }
                } catch(e) {}
                if (isString) continue;

                // Then try nested protobuf
                const nested = decodeProtobuf(chunk, 0, chunk.length);
                if (nested && nested.length > 0) {
                    fields.push(fieldNum + ':{' + nested.join(', ') + '}');
                    continue;
                }

                // Raw bytes
                const hex = Array.from(chunk.slice(0, 32)).map(b => b.toString(16).padStart(2,'0')).join(' ');
                fields.push(fieldNum + ':[' + chunk.length + 'B ' + hex + (chunk.length > 32 ? '...' : '') + ']');
            } else if (wireType === 5) { // 32-bit (float)
                const v = readFixed32(bytes, offset);
                if (!v) return null;
                offset = v.offset;
                const f = v.value;
                if (Number.isFinite(f) && Math.abs(f) > 0.0001 && Math.abs(f) < 1e10) {
                    fields.push(fieldNum + ':' + parseFloat(f.toFixed(4)) + 'f');
                } else {
                    fields.push(fieldNum + ':0x' + Array.from(bytes.slice(offset-4, offset)).map(b => b.toString(16).padStart(2,'0')).join(''));
                }
            } else {
                return null; // unknown wire type, not protobuf
            }
        }
        return fields;
    }

    function decodeBuffer(buf) {
        const bytes = new Uint8Array(buf);
        const len = bytes.length;
        if (len === 0) return '[empty binary]';

        // Try UTF-8 text first
        try {
            const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
            let controlCount = 0;
            for (let i = 0; i < Math.min(text.length, 200); i++) {
                const c = text.charCodeAt(i);
                if (c < 32 && c !== 9 && c !== 10 && c !== 13) controlCount++;
            }
            if (controlCount < text.length * 0.2) {
                if (text.length > 500) return '(text ' + len + 'B) ' + text.substring(0, 500) + '...';
                return '(text ' + len + 'B) ' + text;
            }
        } catch(e) {}

        // Try protobuf decode
        try {
            const fields = decodeProtobuf(bytes, 0, bytes.length);
            if (fields && fields.length > 0) {
                const decoded = '{' + fields.join(', ') + '}';
                if (decoded.length > 800) return '(proto ' + len + 'B) ' + decoded.substring(0, 800) + '...';
                return '(proto ' + len + 'B) ' + decoded;
            }
        } catch(e) {}

        // Fall back to hex dump
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
