const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// DOM elements
const browserUrl = document.getElementById('browser-url');
const btnGo      = document.getElementById('btn-go');
const btnScout   = document.getElementById('btn-scout');
const scoutSteps = document.getElementById('scout-steps');
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

// ---- Scout toggle ----

let scouting = false;

btnScout.addEventListener('click', async () => {
    scouting = !scouting;
    const steps = parseInt(scoutSteps.value) || 3;
    try {
        await invoke('toggle_scout', { active: scouting, steps });
        btnScout.textContent = scouting ? 'Stop Scout' : 'Scout';
        btnScout.classList.toggle('active', scouting);
        addMessage(scouting ? 'Scouting started — ' + steps + ' steps per direction, looking for Lv.40+...' : 'Scouting stopped', 'msg-scout');
    } catch (err) {
        addMessage('Scout error: ' + err, 'msg-error');
    }
});

// When a high-level Pokemon is found, stop scouting and alert
listen('scout-found', (event) => {
    scouting = false;
    btnScout.textContent = 'Scout';
    btnScout.classList.remove('active');
    addMessage(event.payload, 'msg-found');
});

// ---- Intercepted WebSocket frames from browser windows ----

listen('ws-intercepted', (event) => {
    // Color scout reports differently
    if (event.payload.startsWith('[SCOUT]')) {
        addMessage(event.payload, 'msg-scout');
    } else {
        addMessage(event.payload, 'msg-intercepted');
    }
});
