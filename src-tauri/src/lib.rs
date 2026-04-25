use futures_util::{SinkExt, StreamExt};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

type WsSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    Message,
>;

struct WsState {
    sink: Mutex<Option<WsSink>>,
}

#[tauri::command]
async fn ws_connect(
    url: String,
    app: AppHandle,
    state: tauri::State<'_, WsState>,
) -> Result<(), String> {
    let (ws_stream, _response) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|e| format!("Connection failed: {e}"))?;

    let (write, read) = ws_stream.split();

    {
        let mut sink = state.sink.lock().await;
        *sink = Some(write);
    }

    // Spawn a read loop that pushes incoming messages to the frontend
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut read = read;
        while let Some(result) = read.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    let _ = app_handle.emit("ws-message", text.to_string());
                }
                Ok(Message::Binary(bin)) => {
                    let _ = app_handle.emit(
                        "ws-message",
                        format!("[binary: {} bytes]", bin.len()),
                    );
                }
                Ok(Message::Close(frame)) => {
                    let reason = frame
                        .map(|f| f.reason.to_string())
                        .unwrap_or_else(|| "remote closed".into());
                    let _ = app_handle.emit("ws-closed", reason);
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    let _ = app_handle.emit("ws-error", e.to_string());
                    break;
                }
            }
        }
    });

    Ok(())
}

#[tauri::command]
async fn ws_send(
    message: String,
    state: tauri::State<'_, WsState>,
) -> Result<(), String> {
    let mut sink = state.sink.lock().await;
    match sink.as_mut() {
        Some(writer) => writer
            .send(Message::Text(message.into()))
            .await
            .map_err(|e| format!("Send failed: {e}")),
        None => Err("Not connected".into()),
    }
}

#[tauri::command]
async fn ws_disconnect(state: tauri::State<'_, WsState>) -> Result<(), String> {
    let mut sink = state.sink.lock().await;
    if let Some(mut writer) = sink.take() {
        writer
            .close()
            .await
            .map_err(|e| format!("Close failed: {e}"))?;
    }
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .manage(WsState {
            sink: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![ws_connect, ws_send, ws_disconnect])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
