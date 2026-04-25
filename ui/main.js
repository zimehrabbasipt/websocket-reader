const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// DOM elements
const browserUrl    = document.getElementById('browser-url');
const btnGo         = document.getElementById('btn-go');
const browserFrame  = document.getElementById('browser-frame');
const wsUrl         = document.getElementById('ws-url');
const btnConnect    = document.getElementById('btn-connect');
const btnDisconnect = document.getElementById('btn-disconnect');
const wsMessage     = document.getElementById('ws-message');
const btnSend       = document.getElementById('btn-send');
const wsStatus      = document.getElementById('ws-status');
const messageLog    = document.getElementById('message-log');

// ---- Browser Panel ----

btnGo.addEventListener('click', () => {
    let url = browserUrl.value.trim();
    if (url && !url.startsWith('http')) {
        url = 'https://' + url;
        browserUrl.value = url;
    }
    browserFrame.src = url;
});

browserUrl.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') btnGo.click();
});

// ---- WebSocket Panel ----

function addMessage(text, className) {
    const div = document.createElement('div');
    div.className = className;
    div.textContent = text;
    messageLog.appendChild(div);
    messageLog.scrollTop = messageLog.scrollHeight;
}

function setConnected(connected) {
    btnConnect.disabled = connected;
    btnDisconnect.disabled = !connected;
    wsMessage.disabled = !connected;
    btnSend.disabled = !connected;
    wsStatus.textContent = connected ? 'Connected' : 'Disconnected';
    wsStatus.className = 'ws-status ' + (connected ? 'connected' : 'disconnected');
}

btnConnect.addEventListener('click', async () => {
    const url = wsUrl.value.trim();
    if (!url) return;
    try {
        addMessage('Connecting to ' + url + '...', 'msg-system');
        await invoke('ws_connect', { url });
        setConnected(true);
        addMessage('Connected to ' + url, 'msg-system');
    } catch (err) {
        addMessage('Connection failed: ' + err, 'msg-error');
    }
});

btnDisconnect.addEventListener('click', async () => {
    try {
        await invoke('ws_disconnect');
        setConnected(false);
        addMessage('Disconnected', 'msg-system');
    } catch (err) {
        addMessage('Disconnect error: ' + err, 'msg-error');
    }
});

btnSend.addEventListener('click', async () => {
    const msg = wsMessage.value.trim();
    if (!msg) return;
    try {
        await invoke('ws_send', { message: msg });
        addMessage(msg, 'msg-sent');
        wsMessage.value = '';
    } catch (err) {
        addMessage('Send error: ' + err, 'msg-error');
    }
});

wsMessage.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') btnSend.click();
});

// ---- Listen for WS messages from Rust backend ----

listen('ws-message', (event) => {
    addMessage(event.payload, 'msg-recv');
});

listen('ws-closed', (event) => {
    setConnected(false);
    addMessage('Connection closed: ' + event.payload, 'msg-system');
});

listen('ws-error', (event) => {
    setConnected(false);
    addMessage('Error: ' + event.payload, 'msg-error');
});
