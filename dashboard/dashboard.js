// Nexus Dashboard JavaScript

// State
let ws = null;
let reconnectAttempts = 0;
const MAX_RECONNECT_ATTEMPTS = 5;
const BASE_RECONNECT_DELAY = 3000; // Start at 3 seconds
let currentReconnectDelay = BASE_RECONNECT_DELAY;
// Maps backend UUID → display name for request history
const backendNameMap = new Map();

// Initialize dashboard
document.addEventListener('DOMContentLoaded', () => {
    console.log('Dashboard loaded');
    loadInitialData();
    fetchSystemSummary(); // Fetch initial stats
    fetchModels(); // Fetch initial models
    fetchRequestHistory(); // Fetch initial request history
    connectWebSocket();
    
    // Refresh system summary every 5 seconds
    setInterval(fetchSystemSummary, 5000);
    
    // Refresh models every 30 seconds (as per contract)
    setInterval(fetchModels, 30000);
});

// Load initial data from script tag
function loadInitialData() {
    const dataElement = document.getElementById('initial-data');
    if (dataElement) {
        try {
            const data = JSON.parse(dataElement.textContent);
            console.log('Initial data:', data);
            if (data.stats) {
                updateSystemSummary(data.stats);
                if (data.stats.backends && Array.isArray(data.stats.backends)) {
                    renderBackendCardsFromStats(data.stats.backends);
                }
            }
            if (data.backends && Array.isArray(data.backends)) {
                renderBackendCards(data.backends);
            }
            if (data.models && data.models.data) {
                renderModelMatrix(data.models.data);
            }
        } catch (e) {
            console.error('Failed to parse initial data:', e);
        }
    }
}

// Fetch system summary from /v1/stats
async function fetchSystemSummary() {
    try {
        const response = await fetch('/v1/stats');
        if (response.ok) {
            const stats = await response.json();
            updateSystemSummary(stats);
            // Stats include backend data — render cards from it
            if (stats.backends && Array.isArray(stats.backends)) {
                renderBackendCardsFromStats(stats.backends);
            }
        } else {
            console.error('Failed to fetch stats:', response.status);
        }
    } catch (error) {
        console.error('Error fetching stats:', error);
    }
}

// Update system summary display
function updateSystemSummary(stats) {
    // Update uptime
    const uptimeElement = document.getElementById('uptime');
    if (uptimeElement && stats.uptime_seconds !== undefined) {
        uptimeElement.textContent = formatUptime(stats.uptime_seconds);
    }
    
    // Update total requests
    const requestsElement = document.getElementById('total-requests');
    if (requestsElement && stats.requests) {
        requestsElement.textContent = stats.requests.total.toLocaleString();
    }
    
    // Update active backends
    const backendsElement = document.getElementById('active-backends');
    if (backendsElement && stats.backends) {
        const activeCount = stats.backends.filter(b => b.pending !== undefined).length;
        backendsElement.textContent = activeCount;
    }
    
    // Update available models count
    const modelsElement = document.getElementById('available-models');
    if (modelsElement && stats.models) {
        modelsElement.textContent = stats.models.length;
    }
}

// WebSocket connection
function connectWebSocket() {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${protocol}//${window.location.host}/ws`;
    
    console.log('Connecting to WebSocket:', wsUrl);
    
    try {
        ws = new WebSocket(wsUrl);
        
        ws.onopen = handleWebSocketOpen;
        ws.onmessage = handleWebSocketMessage;
        ws.onerror = handleWebSocketError;
        ws.onclose = handleWebSocketClose;
    } catch (error) {
        console.error('WebSocket connection error:', error);
        updateConnectionStatus('disconnected');
    }
}

function handleWebSocketOpen(event) {
    console.log('WebSocket connected');
    reconnectAttempts = 0;
    currentReconnectDelay = BASE_RECONNECT_DELAY; // Reset delay
    stopPolling(); // Stop polling if it was active
    updateConnectionStatus('connected');
}

function handleWebSocketMessage(event) {
    try {
        const update = JSON.parse(event.data);
        console.log('WebSocket update:', update);
        
        switch (update.update_type) {
            case 'BackendStatus':
                handleBackendStatusUpdate(update.data);
                break;
            case 'RequestComplete':
                handleRequestCompleteUpdate(update.data);
                break;
            case 'ModelChange':
                handleModelChangeUpdate(update.data);
                break;
            default:
                console.warn('Unknown update type:', update.update_type);
        }
    } catch (error) {
        console.error('Failed to parse WebSocket message:', error);
    }
}

function handleWebSocketError(error) {
    console.error('WebSocket error:', error);
    updateConnectionStatus('error');
}

function handleWebSocketClose(event) {
    console.log('WebSocket closed:', event.code, event.reason);
    updateConnectionStatus('disconnected');
    
    // Attempt to reconnect with exponential backoff
    if (reconnectAttempts < MAX_RECONNECT_ATTEMPTS) {
        reconnectAttempts++;
        // Exponential backoff: 3s, 6s, 12s, 24s, 48s (capped at 60s)
        currentReconnectDelay = Math.min(BASE_RECONNECT_DELAY * Math.pow(2, reconnectAttempts - 1), 60000);
        console.log(`Reconnecting in ${currentReconnectDelay}ms (attempt ${reconnectAttempts}/${MAX_RECONNECT_ATTEMPTS})`);
        setTimeout(connectWebSocket, currentReconnectDelay);
    } else {
        console.error('Max reconnection attempts reached');
        // Fall back to polling
        startPolling();
    }
}

// Connection status indicator
function updateConnectionStatus(status) {
    const statusDot = document.querySelector('.status-dot');
    const statusText = document.querySelector('.status-text');
    
    if (!statusDot || !statusText) return;
    
    statusDot.className = 'status-dot';
    
    switch (status) {
        case 'connected':
            statusDot.classList.add('connected');
            statusText.textContent = 'Connected';
            break;
        case 'disconnected':
            statusDot.classList.add('disconnected');
            statusText.textContent = 'Disconnected';
            break;
        case 'polling':
            statusDot.classList.add('polling');
            statusText.textContent = 'Polling Mode';
            break;
        case 'error':
            statusDot.classList.add('disconnected');
            statusText.textContent = 'Error';
            break;
        default:
            statusText.textContent = 'Connecting...';
    }
}

// Update handlers
function handleBackendStatusUpdate(data) {
    console.log('Backend status update:', data);
    if (Array.isArray(data)) {
        renderBackendCards(data);
    }
    fetchSystemSummary();
    fetchModels();
}

function handleRequestCompleteUpdate(data) {
    console.log('Request complete update:', data);
    // Add to request history
    addRequestToHistory(data);
}

function handleModelChangeUpdate(data) {
    console.log('Model change update:', data);
    // Refresh the model matrix when models change
    fetchModels();
}

// Polling fallback
let pollingInterval = null;

function startPolling() {
    if (pollingInterval) return;
    
    console.log('Starting polling fallback (5s interval)');
    updateConnectionStatus('polling');
    
    pollingInterval = setInterval(async () => {
        try {
            const response = await fetch('/v1/stats');
            if (response.ok) {
                const data = await response.json();
                updateSystemSummary(data);
            }
        } catch (error) {
            console.error('Polling error:', error);
        }
    }, 5000);
}

function stopPolling() {
    if (pollingInterval) {
        clearInterval(pollingInterval);
        pollingInterval = null;
        console.log('Stopped polling');
    }
}

// Format helpers
function formatDuration(ms) {
    if (ms == null || ms === undefined || isNaN(ms)) return 'N/A';
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(2)}s`;
}

function formatTimestamp(timestamp) {
    if (!timestamp) return 'N/A';
    const date = new Date(timestamp * 1000);
    return date.toLocaleTimeString();
}

function formatUptime(seconds) {
    if (seconds == null || isNaN(seconds)) return 'N/A';
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    
    if (days > 0) return `${days}d ${hours}h`;
    if (hours > 0) return `${hours}h ${minutes}m`;
    return `${minutes}m`;
}

function formatContextLength(contextLength) {
    if (!contextLength || contextLength === 0) return '—';
    if (contextLength >= 1000) {
        return `${(contextLength / 1000).toFixed(0)}k`;
    }
    return contextLength.toString();
}

// Truncate long model names with ellipsis
function truncateModelName(name, maxLength = 50) {
    if (!name) return '—';
    if (name.length <= maxLength) return name;
    return name.substring(0, maxLength - 3) + '...';
}

// Backend card rendering

// Render backend cards from WebSocket BackendView data (full details)
function renderBackendCards(backends) {
    const container = document.getElementById('backend-cards');
    const noBackends = document.getElementById('no-backends');
    if (!container) return;

    if (!backends || backends.length === 0) {
        container.innerHTML = '';
        if (noBackends) noBackends.style.display = 'block';
        return;
    }

    if (noBackends) noBackends.style.display = 'none';
    container.innerHTML = '';

    backends.forEach(backend => {
        backendNameMap.set(backend.id, backend.name || backend.id);
        const card = document.createElement('div');
        card.className = `backend-card ${statusClass}`;

        const statusDot = statusClass === 'healthy' ? '🟢'
            : statusClass === 'unhealthy' ? '🔴' : '🟡';

        card.innerHTML = `
            <div class="backend-header">
                <span class="backend-name">${statusDot} ${escapeHtml(backend.name || backend.id)}</span>
                <span class="badge">${escapeHtml(backend.backend_type || 'Unknown')}</span>
            </div>
            <div class="backend-url">${escapeHtml(backend.url || '')}</div>
            <div class="backend-metrics">
                <div><strong>${backend.total_requests || 0}</strong> requests</div>
                <div><strong>${formatDuration(backend.avg_latency_ms)}</strong> avg</div>
                <div><strong>${backend.pending_requests || 0}</strong> pending</div>
                <div><strong>${(backend.models || []).length}</strong> models</div>
            </div>
        `;
        container.appendChild(card);
    });
}

// Render backend cards from /v1/stats data (minimal: id, requests, latency, pending)
function renderBackendCardsFromStats(backends) {
    // Only render from stats if we haven't received full WebSocket data yet
    const container = document.getElementById('backend-cards');
    if (!container || container.children.length > 0) return;

    const noBackends = document.getElementById('no-backends');
    if (!backends || backends.length === 0) {
        if (noBackends) noBackends.style.display = 'block';
        return;
    }

    if (noBackends) noBackends.style.display = 'none';
    container.innerHTML = '';

    backends.forEach(backend => {
        const card = document.createElement('div');
        card.className = 'backend-card healthy';

        card.innerHTML = `
            <div class="backend-header">
                <span class="backend-name">🟢 ${escapeHtml(backend.id)}</span>
            </div>
            <div class="backend-metrics">
                <div><strong>${backend.requests || 0}</strong> requests</div>
                <div><strong>${formatDuration(backend.average_latency_ms)}</strong> avg</div>
                <div><strong>${backend.pending || 0}</strong> pending</div>
            </div>
        `;
        container.appendChild(card);
    });
}

function escapeHtml(str) {
    if (!str) return '';
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

// Fetch models from /v1/models endpoint
async function fetchModels() {
    try {
        const response = await fetch('/v1/models');
        if (response.ok) {
            const data = await response.json();
            renderModelMatrix(data);
        } else {
            console.error('Failed to fetch models:', response.status);
        }
    } catch (error) {
        console.error('Error fetching models:', error);
    }
}

// Render model availability matrix
function renderModelMatrix(data) {
    const tbody = document.getElementById('model-matrix-tbody');
    const thead = document.querySelector('#model-matrix thead tr');
    const noModelsElement = document.getElementById('no-models');
    
    if (!data || !data.data || data.data.length === 0) {
        tbody.innerHTML = '';
        noModelsElement.style.display = 'block';
        return;
    }
    
    noModelsElement.style.display = 'none';
    
    // Build a map of models to backends
    const modelMap = new Map();
    const backends = new Set();
    
    data.data.forEach(model => {
        if (!modelMap.has(model.id)) {
            modelMap.set(model.id, {
                id: model.id,
                name: model.id,
                context_length: model.context_length || 0,
                supports_vision: model.supports_vision || false,
                supports_tools: model.supports_tools || false,
                supports_json_mode: model.supports_json_mode || false,
                backends: new Set()
            });
        }
        
        // Track which backend has this model (extracted from owned_by or object field)
        const backendId = model.owned_by || model.object || 'unknown';
        modelMap.get(model.id).backends.add(backendId);
        backends.add(backendId);
    });
    
    // Update table headers with backend columns
    const backendArray = Array.from(backends);
    
    // Remove old backend header columns (keep first 3: Model, Context, Capabilities)
    const headerCells = Array.from(thead.querySelectorAll('th'));
    for (let i = headerCells.length - 1; i >= 3; i--) {
        headerCells[i].remove();
    }
    
    // Add new backend header columns
    backendArray.forEach(backendId => {
        const th = document.createElement('th');
        th.className = 'backend-availability';
        th.textContent = backendId;
        thead.appendChild(th);
    });
    
    // Render model rows
    tbody.innerHTML = '';
    modelMap.forEach(model => {
        const row = document.createElement('tr');
        
        // Model name
        const nameCell = document.createElement('td');
        nameCell.className = 'model-name-cell';
        const truncatedName = truncateModelName(model.name, 50);
        nameCell.textContent = truncatedName;
        if (truncatedName !== model.name) {
            nameCell.title = model.name; // Show full name on hover
        }
        row.appendChild(nameCell);
        
        // Context length
        const contextCell = document.createElement('td');
        contextCell.className = 'context-length';
        contextCell.textContent = formatContextLength(model.context_length);
        row.appendChild(contextCell);
        
        // Capabilities
        const capabilitiesCell = document.createElement('td');
        capabilitiesCell.appendChild(renderModelCapabilities(model));
        row.appendChild(capabilitiesCell);
        
        // Backend availability indicators
        backendArray.forEach(backendId => {
            const availabilityCell = document.createElement('td');
            availabilityCell.className = 'backend-availability';
            
            const indicator = document.createElement('span');
            indicator.className = 'availability-indicator';
            
            if (model.backends.has(backendId)) {
                indicator.classList.add('available');
                indicator.textContent = '✓';
                indicator.title = `Available on ${backendId}`;
            } else {
                indicator.classList.add('unavailable');
                indicator.textContent = '—';
                indicator.title = `Not available on ${backendId}`;
            }
            
            availabilityCell.appendChild(indicator);
            row.appendChild(availabilityCell);
        });
        
        tbody.appendChild(row);
    });
}

// Render model capabilities badges
function renderModelCapabilities(model) {
    const container = document.createElement('div');
    container.className = 'capability-badges';
    
    if (model.supports_vision) {
        const badge = document.createElement('span');
        badge.className = 'capability-badge vision';
        badge.textContent = '👁 Vision';
        badge.title = 'Supports vision/image input';
        container.appendChild(badge);
    }
    
    if (model.supports_tools) {
        const badge = document.createElement('span');
        badge.className = 'capability-badge tools';
        badge.textContent = '🔧 Tools';
        badge.title = 'Supports function calling/tools';
        container.appendChild(badge);
    }
    
    if (model.supports_json_mode) {
        const badge = document.createElement('span');
        badge.className = 'capability-badge json';
        badge.textContent = '{ } JSON';
        badge.title = 'Supports JSON mode';
        container.appendChild(badge);
    }
    
    if (!model.supports_vision && !model.supports_tools && !model.supports_json_mode) {
        const noBadge = document.createElement('span');
        noBadge.className = 'text-secondary';
        noBadge.textContent = 'Basic';
        container.appendChild(noBadge);
    }
    
    return container;
}

// Request History Functions
async function fetchRequestHistory() {
    try {
        const response = await fetch('/v1/history');
        if (response.ok) {
            const data = await response.json();
            renderRequestHistory(data);
        } else {
            console.error('Failed to fetch request history:', response.status);
        }
    } catch (error) {
        console.error('Error fetching request history:', error);
    }
}

function addRequestToHistory(entry) {
    // Add new entry to the top of the history
    const tbody = document.getElementById('history-tbody');
    const noHistoryElement = document.getElementById('no-history');
    
    if (noHistoryElement) {
        noHistoryElement.style.display = 'none';
    }
    
    const row = renderRequestRow(entry);
    tbody.insertBefore(row, tbody.firstChild);
    
    // Keep only the last 100 entries
    while (tbody.children.length > 100) {
        tbody.removeChild(tbody.lastChild);
    }
}

function renderRequestHistory(entries) {
    const tbody = document.getElementById('history-tbody');
    const noHistoryElement = document.getElementById('no-history');
    
    if (!entries || entries.length === 0) {
        tbody.innerHTML = '';
        noHistoryElement.style.display = 'block';
        return;
    }
    
    noHistoryElement.style.display = 'none';
    tbody.innerHTML = '';
    
    // Render entries in reverse chronological order
    entries.slice().reverse().forEach(entry => {
        tbody.appendChild(renderRequestRow(entry));
    });
}

function renderRequestRow(entry) {
    const row = document.createElement('tr');
    
    // Timestamp
    const timeCell = document.createElement('td');
    timeCell.textContent = formatTimestamp(entry.timestamp);
    row.appendChild(timeCell);
    
    // Model
    const modelCell = document.createElement('td');
    modelCell.textContent = entry.model || '—';
    row.appendChild(modelCell);
    
    // Backend
    const backendCell = document.createElement('td');
    backendCell.textContent = backendNameMap.get(entry.backend_id) || entry.backend_id || '—';
    row.appendChild(backendCell);
    
    // Duration
    const durationCell = document.createElement('td');
    durationCell.textContent = formatDuration(entry.duration_ms);
    row.appendChild(durationCell);
    
    // Status
    const statusCell = document.createElement('td');
    const statusText = entry.status === 'Success' ? 'Success' : 'Error';
    statusCell.className = entry.status === 'Success' ? 'status-success' : 'status-error';
    statusCell.textContent = statusText;
    
    // If error, make it clickable to show details
    if (entry.status === 'Error' && entry.error_message) {
        statusCell.style.cursor = 'pointer';
        statusCell.title = 'Click to see error details';
        statusCell.addEventListener('click', () => {
            alert(`Error: ${entry.error_message}`);
        });
    }
    
    row.appendChild(statusCell);
    
    return row;
}
