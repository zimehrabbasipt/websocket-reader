use std::sync::atomic::{AtomicU32, Ordering};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

static BROWSER_COUNTER: AtomicU32 = AtomicU32::new(0);

// This script gets injected into every browser window BEFORE the page loads.
// It monkey-patches the WebSocket constructor so we can intercept all frames,
// and provides a scout system that auto-walks and detects high-level Pokemon.
const WS_INTERCEPTOR: &str = r#"
(function() {
    const OrigWS = window.WebSocket;

    // --- Raw Protobuf Decoder ---

    function readVarint(bytes, offset) {
        let result = 0, shift = 0;
        while (offset < bytes.length) {
            const b = bytes[offset++];
            result |= (b & 0x7f) << shift;
            if ((b & 0x80) === 0) return { value: result >>> 0, offset: offset };
            shift += 7;
            if (shift > 35) return null;
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
            if (fieldNum === 0 || fieldNum > 536870911) return null;

            if (wireType === 0) {
                const v = readVarint(bytes, offset);
                if (!v) return null;
                offset = v.offset;
                fields.push(fieldNum + ':' + v.value);
            } else if (wireType === 1) {
                const v = readFixed64(bytes, offset);
                if (!v) return null;
                offset = v.offset;
                const f = v.value;
                if (Number.isFinite(f) && Math.abs(f) > 0.0001 && Math.abs(f) < 1e15) {
                    fields.push(fieldNum + ':' + parseFloat(f.toFixed(4)) + 'd');
                } else {
                    fields.push(fieldNum + ':0x' + Array.from(bytes.slice(offset-8, offset)).map(b => b.toString(16).padStart(2,'0')).join(''));
                }
            } else if (wireType === 2) {
                const lenV = readVarint(bytes, offset);
                if (!lenV || lenV.offset + lenV.value > end) return null;
                offset = lenV.offset;
                const chunk = bytes.slice(offset, offset + lenV.value);
                offset += lenV.value;

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

                const nested = decodeProtobuf(chunk, 0, chunk.length);
                if (nested && nested.length > 0) {
                    fields.push(fieldNum + ':{' + nested.join(', ') + '}');
                    continue;
                }

                const hex = Array.from(chunk.slice(0, 32)).map(b => b.toString(16).padStart(2,'0')).join(' ');
                fields.push(fieldNum + ':[' + chunk.length + 'B ' + hex + (chunk.length > 32 ? '...' : '') + ']');
            } else if (wireType === 5) {
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
                return null;
            }
        }
        return fields;
    }

    // --- Protobuf field extractor (returns map of fieldNum -> value) ---
    // Only extracts top-level fields. Nested length-delimited fields
    // are returned as Uint8Array so we can recurse manually.

    function extractFields(bytes, offset, end) {
        const result = {};
        while (offset < end) {
            const tag = readVarint(bytes, offset);
            if (!tag) break;
            offset = tag.offset;
            const fieldNum = tag.value >>> 3;
            const wireType = tag.value & 0x7;
            if (fieldNum === 0) break;

            if (wireType === 0) {
                const v = readVarint(bytes, offset);
                if (!v) break;
                offset = v.offset;
                result[fieldNum] = v.value;
            } else if (wireType === 1) {
                if (offset + 8 > end) break;
                offset += 8;
            } else if (wireType === 2) {
                const lenV = readVarint(bytes, offset);
                if (!lenV) break;
                offset = lenV.offset;
                result[fieldNum] = bytes.slice(offset, offset + lenV.value);
                offset += lenV.value;
            } else if (wireType === 5) {
                if (offset + 4 > end) break;
                offset += 4;
            } else {
                break;
            }
        }
        return result;
    }

    // Extract Pokemon name and level from a raw WS binary frame.
    // Returns { name, level } or null.
    // Structure: {1:19 (encounter type), 2:{..., 3:level, 7:"Name", ...}}
    function extractPokemon(bytes) {
        try {
            const outer = extractFields(bytes, 0, bytes.length);
            if (outer[1] !== 19) return null; // not a pokemon encounter
            if (!(outer[2] instanceof Uint8Array)) return null;

            const pokemonData = extractFields(outer[2], 0, outer[2].length);
            const level = pokemonData[3];
            if (typeof level !== 'number') return null;

            // Get name from field 7 (string)
            let name = 'Unknown';
            if (pokemonData[7] instanceof Uint8Array) {
                try {
                    name = new TextDecoder('utf-8', { fatal: true }).decode(pokemonData[7]);
                } catch(e) {}
            }

            return { name, level };
        } catch(e) {
            return null;
        }
    }

    // --- Display decoder ---

    function decodeBuffer(buf) {
        const bytes = new Uint8Array(buf);
        const len = bytes.length;
        if (len === 0) return '[empty binary]';

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

        try {
            const fields = decodeProtobuf(bytes, 0, bytes.length);
            if (fields && fields.length > 0) {
                const decoded = '{' + fields.join(', ') + '}';
                if (decoded.length > 800) return '(proto ' + len + 'B) ' + decoded.substring(0, 800) + '...';
                return '(proto ' + len + 'B) ' + decoded;
            }
        } catch(e) {}

        const limit = Math.min(len, 128);
        let hex = '';
        for (let i = 0; i < limit; i++) {
            hex += bytes[i].toString(16).padStart(2, '0') + ' ';
        }
        if (len > limit) hex += '...';
        return '(bin ' + len + 'B) ' + hex.trim();
    }

    async function decodeData(data) {
        if (typeof data === 'string') {
            if (data.length > 500) return data.substring(0, 500) + '...';
            return data;
        }
        if (data instanceof ArrayBuffer) return decodeBuffer(data);
        if (data instanceof Blob) {
            const buf = await data.arrayBuffer();
            return decodeBuffer(buf);
        }
        if (ArrayBuffer.isView(data)) return decodeBuffer(data.buffer);
        return '[unknown type]';
    }

    // --- Scout system: auto-walk + Pokemon level detection ---

    window.__scoutActive = false;
    let walkInterval = null;
    let walkLeft = true;
    let stepCount = 0;
    let stepsPerDirection = 3;

    function pressKey(key, code, keyCode) {
        const down = new KeyboardEvent('keydown', {
            key, code, keyCode, which: keyCode, bubbles: true, cancelable: true
        });
        const up = new KeyboardEvent('keyup', {
            key, code, keyCode, which: keyCode, bubbles: true, cancelable: true
        });
        document.dispatchEvent(down);
        setTimeout(() => document.dispatchEvent(up), 80);
    }

    window.__startScout = function(steps) {
        if (window.__scoutActive) return;
        window.__scoutActive = true;
        stepsPerDirection = steps || 3;
        stepCount = 0;
        walkLeft = true;
        walkInterval = setInterval(() => {
            if (!window.__scoutActive) return;
            if (walkLeft) {
                pressKey('ArrowLeft', 'ArrowLeft', 37);
            } else {
                pressKey('ArrowRight', 'ArrowRight', 39);
            }
            stepCount++;
            if (stepCount >= stepsPerDirection) {
                stepCount = 0;
                walkLeft = !walkLeft;
            }
        }, 250);
    };

    window.__stopScout = function() {
        window.__scoutActive = false;
        if (walkInterval) {
            clearInterval(walkInterval);
            walkInterval = null;
        }
    };

    // Called from the WS message handler when we get binary data
    async function checkScout(rawData) {
        if (!window.__scoutActive) return;
        let bytes;
        if (rawData instanceof ArrayBuffer) {
            bytes = new Uint8Array(rawData);
        } else if (rawData instanceof Blob) {
            bytes = new Uint8Array(await rawData.arrayBuffer());
        } else if (ArrayBuffer.isView(rawData)) {
            bytes = new Uint8Array(rawData.buffer);
        } else {
            return;
        }

        const pokemon = extractPokemon(bytes);
        if (!pokemon) return;

        // Report every pokemon sighting
        try {
            if (window.__TAURI_INTERNALS__) {
                window.__TAURI_INTERNALS__.invoke('report_ws_frame', {
                    frameType: 'SCOUT',
                    wsUrl: 'scout',
                    data: pokemon.name + ' Lv.' + pokemon.level
                });
            }
        } catch(e) {}

        if (pokemon.level >= 40) {
            window.__stopScout();
            try {
                if (window.__TAURI_INTERNALS__) {
                    window.__TAURI_INTERNALS__.invoke('scout_found', {
                        name: pokemon.name,
                        level: pokemon.level
                    });
                }
            } catch(e) {}
        }
    }

    // --- WebSocket interceptor ---

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
            // Also check for Pokemon if scouting
            if (typeof e.data !== 'string') {
                checkScout(e.data);
            }
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

#[tauri::command]
async fn scout_found(name: String, level: u32, app: AppHandle) -> Result<(), String> {
    let payload = format!("FOUND: {} Lv.{} — Stopped scouting!", name, level);
    app.emit("scout-found", payload)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn toggle_scout(active: bool, steps: u32, app: AppHandle) -> Result<(), String> {
    // Run __startScout(steps) or __stopScout() in ALL browser windows
    let js = if active {
        format!("if(window.__startScout) window.__startScout({});", steps)
    } else {
        "if(window.__stopScout) window.__stopScout();".to_string()
    };

    for (label, window) in app.webview_windows() {
        if label.starts_with("browser-") {
            let _ = window.eval(&js);
        }
    }

    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            open_browser,
            report_ws_frame,
            scout_found,
            toggle_scout
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
