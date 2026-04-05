// --- API helper ---
async function api(path, options) {
    const resp = await fetch('/api/' + path, options);
    if (!resp.ok) {
        const text = await resp.text();
        throw new Error(text || resp.statusText);
    }
    return resp.json();
}

// --- DOM ---
const $status = document.getElementById('status');
const $spinner = document.getElementById('spinner');
const $spinnerText = document.getElementById('spinner-text');

// Ollama config
const $ollamaUrl = document.getElementById('ollama-url');
const $modelSelect = document.getElementById('model-select');
const $configStatus = document.getElementById('config-status');
const btnCheckOllama = document.getElementById('btn-check-ollama');

// Anonymize mode
const $modeAnon = document.getElementById('mode-anon');
const $input = document.getElementById('input');
const $output = document.getElementById('output');
const $fileName = document.getElementById('file-name');
const $anonInfo = document.getElementById('anon-info');
const $saveInfo = document.getElementById('save-info');
const $mappingSection = document.getElementById('mapping-section');
const $mappingTable = document.querySelector('#mapping-table tbody');
const $mappingCount = document.getElementById('mapping-count');

const btnOpenFile = document.getElementById('btn-open-file');
const fileInput = document.getElementById('file-input');
const btnAnonymize = document.getElementById('btn-anonymize');
const btnSaveAnon = document.getElementById('btn-save-anon');

// Deanonymize mode
const $modeDeanon = document.getElementById('mode-deanon');
const $deanonInput = document.getElementById('deanon-input');
const $deanonOutput = document.getElementById('deanon-output');
const $deanonFileName = document.getElementById('deanon-file-name');
const $mapFileName = document.getElementById('map-file-name');

const btnOpenAnonFile = document.getElementById('btn-open-anon-file');
const deanonFileInput = document.getElementById('deanon-file-input');
const btnOpenMap = document.getElementById('btn-open-map');
const mapFileInput = document.getElementById('map-file-input');
const btnDeanonymize = document.getElementById('btn-deanonymize');
const $deanonInfo = document.getElementById('deanon-info');
const btnSaveDeanon = document.getElementById('btn-save-deanon');

// Mode buttons
const btnModeAnon = document.getElementById('btn-mode-anon');
const btnModeDeanon = document.getElementById('btn-mode-deanon');

// --- State ---
let currentSourceName = null;
let currentAnonText = null;
let currentMapJson = null;
let deanonText = null;
let deanonMapJson = null;
let deanonFile = null; // Original File object for native format deanonymization
let deanonIsNative = false; // true for DOCX/XLSX

// --- Log panel ---
const $logOutput = document.getElementById('log-output');
const btnClearLogs = document.getElementById('btn-clear-logs');
let logPolling = null;

function appendLog(text) {
    // Add timestamp if not already present (backend logs have [HH:MM:SS])
    if (!text.startsWith('[')) {
        const now = new Date();
        const ts = [now.getHours(), now.getMinutes(), now.getSeconds()]
            .map(n => String(n).padStart(2, '0')).join(':');
        text = '[' + ts + '] ' + text;
    }
    const line = document.createElement('span');
    if (text.includes('BŁĄD')) {
        line.className = 'log-error';
    } else if (text.includes('Zakończono') || text.includes('OK') || text.includes('Pobrano')) {
        line.className = 'log-success';
    }
    line.textContent = text + '\n';
    $logOutput.appendChild(line);
    // Limit log entries to prevent memory leak
    while ($logOutput.childNodes.length > 500) {
        $logOutput.removeChild($logOutput.firstChild);
    }
    $logOutput.scrollTop = $logOutput.scrollHeight;
}

function startLogPolling() {
    if (logPolling) return;
    logPolling = setInterval(async () => {
        try {
            const logs = await api('logs');
            for (const line of logs) {
                appendLog(line);
            }
        } catch (_) {}
    }, 500);
}

function stopLogPolling() {
    if (logPolling) {
        clearInterval(logPolling);
        logPolling = null;
    }
    // Final flush
    setTimeout(async () => {
        try {
            const logs = await api('logs');
            for (const line of logs) appendLog(line);
        } catch (_) {}
    }, 300);
}

const btnCopyLogs = document.getElementById('btn-copy-logs');

btnCopyLogs.addEventListener('click', () => {
    navigator.clipboard.writeText($logOutput.textContent).then(() => {
        btnCopyLogs.textContent = 'Skopiowano!';
        setTimeout(() => { btnCopyLogs.textContent = 'Kopiuj'; }, 1500);
    });
});

btnClearLogs.addEventListener('click', () => {
    $logOutput.textContent = '';
});

// --- Heartbeat & shutdown ---
function sendHeartbeat() {
    fetch('/api/heartbeat', { method: 'POST' }).catch(() => {});
}
setInterval(sendHeartbeat, 5000);

// Browsers throttle setInterval in background tabs (down to ~1/min in Chrome).
// visibilitychange fires immediately when user returns — sends heartbeat before watchdog kills server.
document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') sendHeartbeat();
});

// No shutdown on beforeunload — allows page refresh without killing server.
// Server shuts down via heartbeat timeout (120s without heartbeat).

// --- Init ---
async function init() {
    try {
        const config = await api('get-config');
        $ollamaUrl.value = config.url;
    } catch (_) {}
    setStatus('Wpisz URL i kliknij Sprawdź', '');
}

// --- Ollama config ---
function clearSelectOptions(select) {
    while (select.firstChild) select.removeChild(select.firstChild);
}

function addSelectOption(select, value, text) {
    const opt = document.createElement('option');
    opt.value = value;
    opt.textContent = text;
    select.appendChild(opt);
}

async function checkOllamaAndLoadModels() {
    const url = $ollamaUrl.value.trim().replace(/\/+$/, '');
    if (!url) return;

    btnCheckOllama.classList.add('loading');
    btnCheckOllama.textContent = '...';
    $configStatus.textContent = 'Łączę...';
    $modelSelect.disabled = true;
    clearSelectOptions($modelSelect);
    addSelectOption($modelSelect, '', '— ładuję... —');

    try {
        await api('set-config', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ url, model: '' })
        });

        const models = await api('list-models');

        clearSelectOptions($modelSelect);
        if (models.length === 0) {
            addSelectOption($modelSelect, '', 'brak modeli');
            $configStatus.textContent = 'Połączono, ale brak modeli';
            $configStatus.className = 'config-status error';
        } else {
            addSelectOption($modelSelect, '', '— wybierz model —');
            for (const m of models) {
                addSelectOption($modelSelect, m, m);
            }
            $modelSelect.disabled = false;

            $configStatus.textContent = models.length + ' modeli — wybierz model';
            $configStatus.className = 'config-status ok';
        }
        setStatus('Ollama OK', 'ok');
    } catch (e) {
        clearSelectOptions($modelSelect);
        addSelectOption($modelSelect, '', '— niedostępna —');
        $configStatus.textContent = String(e.message || e);
        $configStatus.className = 'config-status error';
        setStatus('Brak Ollama', 'error');
    } finally {
        btnCheckOllama.classList.remove('loading');
        btnCheckOllama.textContent = 'Sprawdź';
    }
}

async function applySelectedModel() {
    const url = $ollamaUrl.value.trim().replace(/\/+$/, '');
    const model = $modelSelect.value;
    if (url && model) {
        await api('set-config', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ url, model })
        });
    }
}

btnCheckOllama.addEventListener('click', checkOllamaAndLoadModels);

$modelSelect.addEventListener('change', async () => {
    await applySelectedModel();
    $configStatus.textContent = 'Model: ' + $modelSelect.value;
    $configStatus.className = 'config-status ok';
});

// --- Helpers ---
function setStatus(text, cls) {
    $status.textContent = text;
    $status.className = 'status ' + (cls || '');
}

function showSpinner(text) {
    $spinnerText.textContent = text || 'Przetwarzam...';
    $spinner.classList.remove('hidden');
}

function hideSpinner() {
    $spinner.classList.add('hidden');
}

function activateStep(stepId) {
    const step = document.getElementById(stepId);
    if (step) {
        step.classList.remove('disabled');
        step.classList.add('active');
    }
}

function downloadFile(filename, content, type) {
    const blob = new Blob([content], { type: type || 'text/plain;charset=utf-8' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = filename;
    a.click();
    URL.revokeObjectURL(a.href);
}

// --- Mode switch ---
btnModeAnon.addEventListener('click', () => {
    btnModeAnon.classList.add('active');
    btnModeDeanon.classList.remove('active');
    $modeAnon.classList.remove('hidden');
    $modeDeanon.classList.add('hidden');
});

btnModeDeanon.addEventListener('click', () => {
    btnModeDeanon.classList.add('active');
    btnModeAnon.classList.remove('active');
    $modeDeanon.classList.remove('hidden');
    $modeAnon.classList.add('hidden');
});

// =============================================
// ANONYMIZE FLOW
// =============================================

// Step 1: Open file (via hidden <input type="file">)
btnOpenFile.addEventListener('click', () => fileInput.click());

fileInput.addEventListener('change', async () => {
    const file = fileInput.files[0];
    if (!file) return;

    try {
        showSpinner('Wczytuję plik...');
        currentSourceName = file.name;

        const ext = file.name.split('.').pop().toLowerCase();
        let text;

        if (['xlsx', 'xls', 'docx'].includes(ext)) {
            // Binary files — send to backend
            const formData = new FormData();
            formData.append('file', file);
            text = await api('load-file', { method: 'POST', body: formData });
        } else {
            // Text files — read locally
            text = await file.text();
        }

        $input.value = text;
        $fileName.textContent = file.name + ' (' + text.length + ' znaków)';
        appendLog('Wczytano plik: ' + file.name + ' (' + text.length + ' znaków)');

        activateStep('step-anon-2');
        btnAnonymize.disabled = false;
        setStatus('Plik wczytany', 'ok');
    } catch (e) {
        setStatus('Błąd: ' + (e.message || e), 'error');
    } finally {
        hideSpinner();
        fileInput.value = '';
    }
});

// Step 2: Anonymize
btnAnonymize.addEventListener('click', async () => {
    const text = $input.value.trim();
    if (!text) return;

    try {
        const modelName = $modelSelect.value || 'Model';
        showSpinner(modelName + ' analizuje dokument...');
        btnAnonymize.disabled = true;
        startLogPolling();

        const categories = Array.from(document.querySelectorAll('.category-panel input:checked'))
            .map(cb => cb.value);

        const result = await api('anonymize', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ text, source_file: currentSourceName || 'unknown', categories })
        });

        currentAnonText = result.text;
        $output.value = result.text;
        $anonInfo.textContent = result.entities_found + ' encji | ' + result.model_used;

        if (result.entities_found === 0) {
            setStatus('0 encji — model nie znalazł danych wrażliwych', 'error');
        } else {
            // export-map returns a JSON string (already serialized map)
            currentMapJson = await api('export-map');

            await refreshMapping();
            activateStep('mapping-section');

            activateStep('step-anon-3');
            btnSaveAnon.disabled = false;

            setStatus('Anonimizacja OK — ' + result.entities_found + ' encji', 'ok');
        }
    } catch (e) {
        setStatus('Błąd: ' + (e.message || e), 'error');
    } finally {
        stopLogPolling();
        hideSpinner();
        btnAnonymize.disabled = false;
    }
});

async function refreshMapping() {
    try {
        const mapping = await api('get-mapping');
        if (mapping.length === 0) return;

        $mappingCount.textContent = mapping.length + ' encji';
        $mappingTable.textContent = '';

        for (const item of mapping) {
            const row = document.createElement('tr');

            const tdToken = document.createElement('td');
            tdToken.textContent = item.token;
            row.appendChild(tdToken);

            const tdType = document.createElement('td');
            tdType.textContent = item.entity_type;
            row.appendChild(tdType);

            const tdOriginal = document.createElement('td');
            tdOriginal.textContent = item.original;
            row.appendChild(tdOriginal);

            $mappingTable.appendChild(row);
        }
    } catch (e) {
        console.error('Mapping error:', e);
    }
}

// Step 3: Save (download)
btnSaveAnon.addEventListener('click', async () => {
    if (!currentAnonText || !currentMapJson) return;

    const baseName = currentSourceName
        ? currentSourceName.replace(/\.[^.]+$/, '')
        : 'dokument';

    const ext = currentSourceName ? currentSourceName.split('.').pop().toLowerCase() : '';

    // Native format export for DOCX/XLSX
    const isNative = ['docx', 'xlsx', 'xls'].includes(ext);
    if (isNative) {
        try {
            const resp = await fetch('/api/export-anon-native');
            if (resp.ok) {
                const blob = await resp.blob();
                const a = document.createElement('a');
                a.href = URL.createObjectURL(blob);
                a.download = baseName + '.anon.' + ext;
                a.click();
                URL.revokeObjectURL(a.href);
            } else {
                downloadFile(baseName + '.anon.md', currentAnonText);
            }
        } catch (_) {
            downloadFile(baseName + '.anon.md', currentAnonText);
        }
    } else {
        downloadFile(baseName + '.anon.md', currentAnonText);
    }

    // Always download the map
    const mapStr = typeof currentMapJson === 'string' ? currentMapJson : JSON.stringify(currentMapJson, null, 2);
    downloadFile(baseName + '.map.json', mapStr, 'application/json');

    const outExt = isNative ? '.anon.' + ext : '.anon.md';
    $saveInfo.textContent = 'Pobrano: ' + baseName + outExt + ' + .map.json';
    appendLog('Pobrano: ' + baseName + outExt + ' + .map.json');
    setStatus('Pliki pobrane', 'ok');
});

// =============================================
// DEANONYMIZE FLOW
// =============================================

// Step 1: Open anonymized file
btnOpenAnonFile.addEventListener('click', () => deanonFileInput.click());

deanonFileInput.addEventListener('change', async () => {
    const file = deanonFileInput.files[0];
    if (!file) return;

    const ext = file.name.split('.').pop().toLowerCase();
    deanonIsNative = ['docx', 'xlsx', 'xls'].includes(ext);
    deanonFile = file;

    if (deanonIsNative) {
        // DOCX — upload to backend to extract text for preview
        const formData = new FormData();
        formData.append('file', file);
        try {
            deanonText = await api('load-file', { method: 'POST', body: formData });
            $deanonInput.value = deanonText;
        } catch (e) {
            $deanonInput.value = '[DOCX — podgląd niedostępny, ale de-anonimizacja binarna zadziała]';
            deanonText = null;
            appendLog('⚠️ Nie udało się odczytać podglądu: ' + (e.message || e));
        }
    } else {
        deanonText = await file.text();
        $deanonInput.value = deanonText;
    }

    $deanonFileName.textContent = file.name;
    appendLog('Wczytano plik do de-anonimizacji: ' + file.name);
    // Clear stale state from previous session
    btnSaveDeanon._docxBlob = null;
    btnSaveDeanon._docxName = null;
    activateStep('step-deanon-2');
    btnOpenMap.disabled = false;
    deanonFileInput.value = '';
});

// Step 2: Open map file + auto-deanonymize
btnOpenMap.addEventListener('click', () => mapFileInput.click());

mapFileInput.addEventListener('change', async () => {
    const file = mapFileInput.files[0];
    if (!file) return;

    deanonMapJson = await file.text();
    $mapFileName.textContent = file.name;
    appendLog('Wczytano mapę: ' + file.name);

    activateStep('step-deanon-3');
    btnDeanonymize.disabled = false;
    setStatus('Mapa wczytana — kliknij De-anonimizuj', 'ok');
    mapFileInput.value = '';
});

// Step 3: Deanonymize
btnDeanonymize.addEventListener('click', async () => {
    if (!deanonMapJson) return;

    try {
        showSpinner('De-anonimizuję...');
        btnDeanonymize.disabled = true;

        if (deanonIsNative && deanonFile) {
            // Native format deanonymization — upload file + map, get file back
            const formData = new FormData();
            formData.append('file', deanonFile);
            formData.append('map_json', deanonMapJson);

            const resp = await fetch('/api/deanonymize-docx', {
                method: 'POST',
                body: formData
            });

            if (!resp.ok) {
                throw new Error(await resp.text());
            }

            // Read stats from header
            const statsHeader = resp.headers.get('x-deanon-stats');
            let stats = null;
            try { stats = statsHeader ? JSON.parse(statsHeader) : null; } catch (_) {}

            const blob = await resp.blob();
            const fileExt = deanonFile.name.split('.').pop().toLowerCase();

            // Show stats in logs and info
            if (stats) {
                const info = stats.found + '/' + stats.total + ' tokenów zamieniono';
                $deanonInfo.textContent = fileExt.toUpperCase() + ' odtworzony — ' + info;
                appendLog('De-anonimizacja: ' + info);
                if (stats.missing.length > 0) {
                    appendLog('Brakujące tokeny: ' + stats.missing.join(', '));
                }
            } else {
                $deanonInfo.textContent = fileExt.toUpperCase() + ' odtworzony';
            }

            deanonText = '[De-anonimizacja zakończona, kliknij Pobierz]';
            $deanonOutput.value = deanonText;

            // Store blob for save step
            btnSaveDeanon._docxBlob = blob;
            const anonExt = '.anon.' + fileExt;
            btnSaveDeanon._docxName = deanonFile.name.includes(anonExt)
                ? deanonFile.name.replace(anonExt, '.restored.' + fileExt)
                : deanonFile.name.replace('.' + fileExt, '.restored.' + fileExt);
        } else {
            // Text deanonymization
            if (!deanonText) return;

            const result = await api('deanonymize', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ anon_text: deanonText, map_json: deanonMapJson })
            });

            $deanonOutput.value = result.text;
            const stats = result.stats;
            const info = stats.found + '/' + stats.total + ' tokenów zamieniono';
            $deanonInfo.textContent = info;
            appendLog('De-anonimizacja: ' + info);
            if (stats.missing.length > 0) {
                appendLog('Brakujące tokeny: ' + stats.missing.join(', '));
            }
            btnSaveDeanon._docxBlob = null;
        }

        activateStep('step-deanon-4');
        btnSaveDeanon.disabled = false;

        setStatus('De-anonimizacja OK', 'ok');
    } catch (e) {
        setStatus('Błąd: ' + (e.message || e), 'error');
    } finally {
        hideSpinner();
        btnDeanonymize.disabled = false;
    }
});

// Step 3: Save deanonymized (download)
btnSaveDeanon.addEventListener('click', () => {
    if (btnSaveDeanon._docxBlob) {
        // Download restored DOCX
        const a = document.createElement('a');
        a.href = URL.createObjectURL(btnSaveDeanon._docxBlob);
        const fname = btnSaveDeanon._docxName || 'dokument.restored.docx';
        a.download = fname;
        a.click();
        URL.revokeObjectURL(a.href);
        appendLog('Pobrano: ' + fname);
    } else {
        const text = $deanonOutput.value;
        if (!text) return;
        downloadFile('dokument.restored.md', text);
        appendLog('Pobrano: dokument.restored.md');
    }
    setStatus('Plik pobrany', 'ok');
});

// --- Theme ---
const btnTheme = document.getElementById('btn-theme');

if (window.matchMedia('(prefers-color-scheme: light)').matches) {
    document.body.classList.add('light');
    btnTheme.textContent = '☀️';
}

btnTheme.addEventListener('click', () => {
    document.body.classList.toggle('light');
    btnTheme.textContent = document.body.classList.contains('light') ? '☀️' : '🌙';
});

// --- Start ---
init();
