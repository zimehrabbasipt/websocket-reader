const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// DOM elements
const browserUrl = document.getElementById('browser-url');
const btnGo      = document.getElementById('btn-go');
const btnClear   = document.getElementById('btn-clear');
const messageLog = document.getElementById('message-log');

// ---- Browser ----

btnGo.addEventListener('click', async () => {
    let url = browserUrl.value.trim();
    if (!url) return;
    if (!url.startsWith('http')) {
        url = 'https://' + url;
        browserUrl.value = url;
    }
    try {
        await invoke('open_browser', { url });
    } catch (err) {
        addMessage('Browser error: ' + err, 'msg-error');
    }
});

browserUrl.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') btnGo.click();
});

// ---- Message Log ----

function addMessage(text, className) {
    const div = document.createElement('div');
    div.className = className;
    div.textContent = text;
    messageLog.appendChild(div);
    messageLog.scrollTop = messageLog.scrollHeight;
}

btnClear.addEventListener('click', () => {
    messageLog.innerHTML = '';
});

// ---- Intercepted WebSocket frames from browser windows ----

listen('ws-intercepted', (event) => {
    addMessage(event.payload, 'msg-intercepted');
});
