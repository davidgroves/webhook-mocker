(() => {
  const $ = (sel, root = document) => root.querySelector(sel);

  document.querySelectorAll(".tab").forEach((tab) => {
    tab.addEventListener("click", () => {
      const panel = tab.closest(".debug-panel");
      if (!panel) return;
      panel.querySelectorAll(".tab").forEach((t) => t.classList.remove("active"));
      panel.querySelectorAll(".tab-pane").forEach((p) => p.classList.remove("active"));
      tab.classList.add("active");
      const pane = panel.querySelector(`#tab-${tab.dataset.tab}`);
      if (pane) pane.classList.add("active");
    });
  });

  document.querySelectorAll("[data-copy]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const el = document.querySelector(btn.dataset.copy);
      if (!el) return;
      const text = el.innerText || el.textContent || "";
      try {
        await navigator.clipboard.writeText(text);
        btn.textContent = "Copied";
        setTimeout(() => (btn.textContent = btn.dataset.label || "Copy"), 1200);
      } catch (_) {}
    });
  });

  const newDialog = $("#new-dialog");
  const newWebhook = $("#new-webhook");
  if (newWebhook && newDialog) {
    newWebhook.addEventListener("click", () => newDialog.showModal());
    newDialog.addEventListener("close", async () => {
      if (newDialog.returnValue !== "create") return;
      const kind = $("#new-kind").value;
      const name = $("#new-name").value.trim();
      if (!name) return;
      const res = await fetch("/api/channels", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ kind, name }),
      });
      if (res.ok) {
        const ch = await res.json();
        location.href = `/ui/channel/${ch.id}`;
      }
    });
  }

  const clearBtn = $("#clear-messages");
  clearBtn?.addEventListener("click", async () => {
    const id = clearBtn.dataset.channel;
    if (!id || !confirm("Clear all messages in this channel?")) return;
    await fetch(`/api/channels/${id}/messages`, { method: "DELETE" });
    location.reload();
  });

  const delBtn = $("#delete-channel");
  delBtn?.addEventListener("click", async () => {
    const id = delBtn.dataset.channel;
    if (!id || !confirm("Delete this channel?")) return;
    await fetch(`/api/channels/${id}`, { method: "DELETE" });
    location.href = "/";
  });

  const faultsDialog = $("#faults-dialog");
  const faultsBtn = $("#toggle-faults");
  faultsBtn?.addEventListener("click", async () => {
    const id = faultsBtn.dataset.channel;
    const res = await fetch(`/api/channels/${id}`);
    if (!res.ok) return;
    const ch = await res.json();
    const f = ch.faults || {};
    $("#fault-status").value = f.status ?? "";
    $("#fault-delay").value = f.delay_ms ?? 0;
    $("#fault-delay-random").value = f.delay_random_ms ?? 0;
    $("#fault-429").checked = !!f.force_429;
    $("#fault-retry-after").value = f.retry_after_secs ?? 1;
    $("#fault-fail-pct").value = f.fail_percent ?? 0;
    faultsDialog.showModal();
  });

  faultsDialog?.addEventListener("close", async () => {
    if (faultsDialog.returnValue !== "save") return;
    const id = faultsBtn?.dataset.channel;
    if (!id) return;
    const statusRaw = $("#fault-status").value;
    const body = {
      status: statusRaw === "" ? null : Number(statusRaw),
      delay_ms: Number($("#fault-delay").value) || 0,
      delay_random_ms: Number($("#fault-delay-random").value) || 0,
      force_429: $("#fault-429").checked,
      retry_after_secs: Number($("#fault-retry-after").value) || 1,
      fail_percent: Number($("#fault-fail-pct").value) || 0,
    };
    await fetch(`/api/channels/${id}/faults`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  });

  // Live updates via SSE
  try {
    const es = new EventSource("/api/events");
    const reloadSoon = (() => {
      let t;
      return () => {
        clearTimeout(t);
        t = setTimeout(() => location.reload(), 250);
      };
    })();
    ["message", "channel_created", "channel_updated", "channel_deleted", "messages_cleared"].forEach(
      (name) => es.addEventListener(name, reloadSoon)
    );
  } catch (_) {}
})();
