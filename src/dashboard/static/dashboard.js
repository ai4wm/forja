const state = {
  activeTab: "chat",
  pendingBubble: null,
  streamBuffer: "",
};

const sections = Object.fromEntries(
  Array.from(document.querySelectorAll(".tab-panel")).map((panel) => [
    panel.id.replace("tab-", ""),
    panel,
  ]),
);

function setTab(tab) {
  state.activeTab = tab;
  document.querySelectorAll(".tab-button").forEach((button) => {
    button.classList.toggle("active", button.dataset.tab === tab);
  });
  Object.entries(sections).forEach(([name, panel]) => {
    panel.classList.toggle("active", name === tab);
  });
}

document.querySelectorAll(".tab-button").forEach((button) => {
  button.addEventListener("click", () => setTab(button.dataset.tab));
});

async function fetchJson(url, options) {
  const response = await fetch(url, options);
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }
  return response.json();
}

function safeJson(value) {
  return JSON.stringify(value, null, 2);
}

function el(id) {
  return document.getElementById(id);
}

function prependEvent(targetId, html, maxItems = 24) {
  const target = el(targetId);
  const wrapper = document.createElement("article");
  wrapper.className = "event-item";
  wrapper.innerHTML = html;
  target.prepend(wrapper);
  while (target.children.length > maxItems) {
    target.removeChild(target.lastChild);
  }
}

function appendChatBubble(role, text, meta, pending = false) {
  const bubble = document.createElement("article");
  bubble.className = `chat-bubble ${role}${pending ? " pending" : ""}`;
  bubble.innerHTML = `
    <div class="bubble-meta">${meta}</div>
    <div>${text}</div>
  `;
  el("chat-thread").appendChild(bubble);
  el("chat-thread").scrollTop = el("chat-thread").scrollHeight;
  return bubble;
}

async function loadAudit() {
  const filter = el("audit-filter").value.trim();
  const limit = el("audit-limit").value;
  const query = new URLSearchParams({ limit });
  if (filter) {
    query.set("event_type", filter);
  }
  const rows = await fetchJson(`/api/audit?${query.toString()}`);
  el("audit-body").innerHTML = rows.map((row) => `
    <tr>
      <td>${row.timestamp}</td>
      <td>${row.event_type}</td>
      <td>${row.agent_id}</td>
      <td>${row.token_count}</td>
      <td><pre>${safeJson(row.payload)}</pre></td>
    </tr>
  `).join("");
}

async function loadConversation() {
  const rows = await fetchJson("/api/conversation?limit=18");
  el("conversation-feed").innerHTML = rows.map((row) => `
    <article class="event-item">
      <div class="event-meta">${row.timestamp} · ${row.event_type}</div>
      <strong>${row.headline}</strong>
      <div>${row.detail}</div>
    </article>
  `).join("");
}

async function loadDebates() {
  const debates = await fetchJson("/api/debates");
  el("debate-list").innerHTML = debates.map((debate, index) => `
    <article class="event-item debate-card ${index === 0 ? "active" : ""}" data-id="${debate.id}">
      <div class="event-meta">${debate.started_at}</div>
      <strong>${debate.message_count} messages</strong>
      <div>${debate.preview || "(empty)"}</div>
    </article>
  `).join("");

  document.querySelectorAll("#debate-list .debate-card").forEach((item) => {
    item.addEventListener("click", async () => {
      document.querySelectorAll("#debate-list .debate-card").forEach((node) => node.classList.remove("active"));
      item.classList.add("active");
      await loadDebateTranscript(item.dataset.id);
    });
  });

  if (debates[0]) {
    await loadDebateTranscript(debates[0].id);
  } else {
    el("transcript").innerHTML = "";
  }
}

async function loadDebateTranscript(id) {
  const transcript = await fetchJson(`/api/debate/${id}`);
  el("transcript").innerHTML = transcript.map((message) => `
    <article class="chat-bubble assistant">
      <div class="bubble-meta">${message.timestamp} · ${message.phase} · ${message.role}</div>
      <div>${message.content}</div>
    </article>
  `).join("");
}

async function loadBudget() {
  const rows = await fetchJson("/api/budget");
  el("budget-grid").innerHTML = rows.map((row) => `
    <article class="budget-card">
      <div class="budget-row">
        <strong>${row.agent_id}</strong>
        <span>${row.used_tokens} / ${row.monthly_limit}</span>
      </div>
      <div class="bar"><div class="bar-fill" style="width:${Math.min(row.percent, 100)}%"></div></div>
      <div class="budget-row">
        <span class="muted">${row.month_key}</span>
        <span>${row.percent}%</span>
      </div>
    </article>
  `).join("");
}

async function approveTask(id) {
  await fetch(`/api/approve/${id}`, { method: "POST" });
  await loadTasks();
}

async function loadTasks() {
  const [tasks, skills, unresolved] = await Promise.all([
    fetchJson("/api/tasks"),
    fetchJson("/api/skills"),
    fetchJson("/api/unresolved"),
  ]);

  el("task-list").innerHTML = tasks.map((task) => `
    <article class="event-item">
      <div class="event-meta">#${task.id} · ${task.source} · ${task.status}</div>
      <strong>${task.description}</strong>
      <div class="muted">created ${task.created_at}</div>
      ${task.requires_approval ? `<button class="ghost-button approve-button" data-task-id="${task.id}">Approve</button>` : ""}
    </article>
  `).join("");

  document.querySelectorAll("[data-task-id]").forEach((button) => {
    button.addEventListener("click", () => approveTask(button.dataset.taskId));
  });

  el("skills-list").innerHTML = skills.map((skill) => `
    <article class="event-item">
      <div class="event-meta">${skill.last_used || "-"}</div>
      <strong>${skill.tool_name}</strong>
      <div>success=${skill.success_count}</div>
      <div class="muted">auto=${skill.auto_approved}</div>
    </article>
  `).join("");

  el("unresolved-list").innerHTML = unresolved.map((item) => `
    <article class="event-item">
      <div class="event-meta">#${item.id} · ${item.status}</div>
      <strong>${item.task}</strong>
      <div>${item.error || ""}</div>
    </article>
  `).join("");
}

async function loadTools() {
  const rows = await fetchJson("/api/tools");
  el("tools-list").innerHTML = rows.map((row) => `
    <article class="event-item">
      <div class="event-meta">${row.timestamp}</div>
      <strong>${row.tool_name}</strong>
      <pre>${safeJson(row.payload)}</pre>
    </article>
  `).join("");
}

async function loadMemory() {
  const query = el("memory-query").value.trim();
  const searchParams = query ? `?q=${encodeURIComponent(query)}` : "";
  const [stateRow, entries, summaries] = await Promise.all([
    fetchJson("/api/memory"),
    fetchJson(`/api/memory/entries${searchParams}`),
    fetchJson(`/api/memory/summaries${searchParams}`),
  ]);

  el("memory-state").innerHTML = `
    <article class="stat-card">
      <div class="event-meta">entries</div>
      <strong>${stateRow.memory_entries}</strong>
    </article>
    <article class="stat-card">
      <div class="event-meta">summaries</div>
      <strong>${stateRow.memory_summaries}</strong>
    </article>
  `;

  el("memory-entries").innerHTML = entries.map((entry) => `
    <article class="event-item">
      <div class="event-meta">${entry.source} · ${entry.role}</div>
      <strong>${entry.id}</strong>
      <div>${entry.content}</div>
    </article>
  `).join("");

  el("memory-summaries").innerHTML = summaries.map((summary) => `
    <article class="event-item">
      <div class="event-meta">${summary.source}</div>
      <pre>${summary.summary}</pre>
    </article>
  `).join("");
}

async function loadChannelStatus() {
  const status = await fetchJson("/api/channel-status");
  el("telegram-status").textContent = status.telegram;
}

async function submitChat(event) {
  event.preventDefault();
  const input = el("chat-input");
  const text = input.value.trim();
  if (!text) {
    return;
  }

  appendChatBubble("user", text, "You");
  input.value = "";
  state.streamBuffer = "";
  state.pendingBubble = appendChatBubble("assistant", "", "Forja · streaming", true);
  el("stream-status").textContent = "Streaming";

  const result = await fetchJson("/api/chat", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ text }),
  });

  if (!result.ok && state.pendingBubble) {
    state.pendingBubble.classList.remove("pending");
    state.pendingBubble.innerHTML = `<div class="bubble-meta">Forja · error</div><div>${result.reason || "chat request failed"}</div>`;
    state.pendingBubble = null;
    el("stream-status").textContent = "Error";
  }
}

function startChatStream() {
  const stream = new EventSource("/api/chat/stream");
  stream.onmessage = (event) => {
    const payload = JSON.parse(event.data);
    if (payload.kind === "assistant_chunk") {
      state.streamBuffer += payload.text;
      if (!state.pendingBubble) {
        state.pendingBubble = appendChatBubble("assistant", "", "Forja · streaming", true);
      }
      state.pendingBubble.innerHTML = `<div class="bubble-meta">Forja · streaming</div><div>${state.streamBuffer}</div>`;
      el("stream-status").textContent = "Streaming";
      return;
    }

    if (payload.kind === "assistant_message") {
      const text = payload.text || state.streamBuffer;
      if (state.pendingBubble) {
        state.pendingBubble.classList.remove("pending");
        state.pendingBubble.innerHTML = `<div class="bubble-meta">Forja</div><div>${text}</div>`;
      } else {
        appendChatBubble("assistant", text, "Forja");
      }
      state.pendingBubble = null;
      state.streamBuffer = "";
      el("stream-status").textContent = "Idle";
      loadConversation().catch(console.error);
      return;
    }

    if (payload.kind === "user_message") {
      return;
    }

    if (payload.kind === "error") {
      el("stream-status").textContent = "Unavailable";
      prependEvent("event-stream", `<div class="event-meta">chat stream</div><strong>${payload.text}</strong>`);
    }
  };

  stream.onerror = () => {
    el("stream-status").textContent = "Disconnected";
  };
}

function startAuditEventStream() {
  const events = new EventSource("/api/events");
  events.onmessage = (event) => {
    prependEvent("event-stream", `<pre>${event.data}</pre>`);
  };
}

async function refreshAll() {
  await Promise.all([
    loadAudit(),
    loadConversation(),
    loadDebates(),
    loadBudget(),
    loadTasks(),
    loadTools(),
    loadMemory(),
    loadChannelStatus(),
  ]);
}

el("audit-filter").addEventListener("change", loadAudit);
el("audit-limit").addEventListener("change", loadAudit);
el("memory-search").addEventListener("click", loadMemory);
el("memory-query").addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    loadMemory().catch(console.error);
  }
});
el("chat-form").addEventListener("submit", (event) => {
  submitChat(event).catch(console.error);
});

setTab("chat");
startChatStream();
startAuditEventStream();
refreshAll().catch(console.error);
setInterval(() => {
  loadConversation().catch(console.error);
  loadTools().catch(console.error);
  loadChannelStatus().catch(console.error);
}, 5000);
