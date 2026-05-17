<script>
  import { getStats, getLive, deleteSite } from "./api.js";
  import EventsView from "./EventsView.svelte";

  let { domain, onDeleted } = $props();

  let stats = $state(null);
  let live = $state(0);
  let range = $state("24h"); // 24h | 7d | 30d
  let view = $state("stats"); // stats | events
  let error = $state("");

  function rangeMs() {
    const map = { "24h": 24 * 3600e3, "7d": 7 * 24 * 3600e3, "30d": 30 * 24 * 3600e3 };
    return map[range] || map["24h"];
  }

  let nowMs = $state(Date.now());
  let fromMs = $derived(nowMs - rangeMs());
  let toMs = $derived(nowMs);

  async function load() {
    error = "";
    try {
      nowMs = Date.now();
      stats = await getStats(domain, nowMs - rangeMs(), nowMs);
    } catch (e) {
      error = e.message;
    }
  }

  async function loadLive() {
    try {
      const r = await getLive(domain);
      live = r.active;
    } catch (_) {}
  }

  $effect(() => {
    domain; range;
    load();
    loadLive();
    const tStats = setInterval(load, 10000);
    const tLive = setInterval(loadLive, 3000);
    return () => { clearInterval(tStats); clearInterval(tLive); };
  });

  async function del() {
    if (!confirm(`"${domain}" 삭제? 이벤트도 함께 삭제됩니다.`)) return;
    await deleteSite(domain);
    onDeleted?.();
  }

  const snippet = (origin) =>
`<script src="${origin}/tracker/psmeter.js"
        data-server="${origin}"
        data-site="${domain}"><\/script>`;

  function snippetForBrowser() {
    if (typeof window === "undefined") return snippet("https://your-psmeter.com");
    return snippet(window.location.origin);
  }
</script>

<h2 class="dom">{domain}</h2>

<div class="bar">
  <div class="live-card">
    <span class="dot"></span>
    <span class="live-n">{live}</span>
    <span class="live-lbl">지금 접속중</span>
  </div>
  <div class="range">
    {#each ["24h","7d","30d"] as r}
      <button class="rb" class:on={range === r} onclick={() => (range = r)}>{r}</button>
    {/each}
  </div>
</div>

<div class="tabs">
  <button class="tab" class:on={view === "stats"} onclick={() => (view = "stats")}>통계</button>
  <button class="tab" class:on={view === "events"} onclick={() => (view = "events")}>원본 이벤트</button>
</div>

{#if view === "events"}
  <EventsView {domain} {fromMs} {toMs} />
{:else if error}
  <div class="err">{error}</div>
{:else if stats}
  <div class="kpis">
    <div class="kpi">
      <div class="val">{stats.pageviews.toLocaleString()}</div>
      <div class="lbl">페이지뷰</div>
    </div>
    <div class="kpi">
      <div class="val">{stats.visitors.toLocaleString()}</div>
      <div class="lbl">방문자 (Unique)</div>
    </div>
  </div>

  <div class="grid">
    <div class="panel">
      <h3>상위 페이지</h3>
      <ul class="rank">
        {#each stats.top_pages as [p, n]}
          <li><span class="t">{p}</span><span class="n">{n.toLocaleString()}</span></li>
        {:else}
          <li class="empty">데이터 없음</li>
        {/each}
      </ul>
    </div>

    <div class="panel">
      <h3>유입 (Referrer)</h3>
      <ul class="rank">
        {#each stats.top_referrers as [r, n]}
          <li><span class="t">{r}</span><span class="n">{n.toLocaleString()}</span></li>
        {:else}
          <li class="empty">데이터 없음</li>
        {/each}
      </ul>
    </div>

    <div class="panel">
      <h3>디바이스</h3>
      <ul class="rank">
        {#each stats.devices as [d, n]}
          <li><span class="t">{d}</span><span class="n">{n.toLocaleString()}</span></li>
        {:else}
          <li class="empty">데이터 없음</li>
        {/each}
      </ul>
    </div>

    <div class="panel">
      <h3>국가</h3>
      <ul class="rank">
        {#each stats.countries as [c, n]}
          <li><span class="t">{c}</span><span class="n">{n.toLocaleString()}</span></li>
        {:else}
          <li class="empty">데이터 없음</li>
        {/each}
      </ul>
    </div>
  </div>

  <div class="panel">
    <h3>설치 코드</h3>
    <p class="hint">이 도메인의 <code>&lt;head&gt;</code>에 박으면 자동 추적 시작</p>
    <pre class="snippet">{snippetForBrowser()}</pre>
  </div>

  <div class="actions">
    <button class="btn danger" onclick={del}>사이트 삭제</button>
  </div>
{/if}

<style>
  .dom { font-size: 24px; margin: 0 0 16px; }
  .bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
  }
  .live-card {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 14px;
  }
  .dot {
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--ok);
    box-shadow: 0 0 6px var(--ok);
    animation: pulse 1.5s infinite;
  }
  @keyframes pulse { 50% { opacity: 0.4; } }
  .live-n { font-size: 22px; font-weight: 700; color: var(--accent); font-variant-numeric: tabular-nums; }
  .live-lbl { color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.05em; }
  .range { display: flex; gap: 4px; }
  .rb {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--muted);
    padding: 6px 12px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 12px;
  }
  .rb.on { background: var(--accent); color: var(--bg); border-color: var(--accent); }
  .kpis { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; margin-bottom: 20px; }
  .kpi { background: var(--bg); border: 1px solid var(--border); border-radius: 10px; padding: 18px; text-align: center; }
  .kpi .val { font-size: 32px; font-weight: 800; color: var(--accent); font-variant-numeric: tabular-nums; }
  .kpi .lbl { color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.05em; margin-top: 4px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; margin-bottom: 20px; }
  .panel { background: var(--panel); border: 1px solid var(--border); border-radius: 10px; padding: 16px; }
  .panel h3 { margin: 0 0 10px; font-size: 14px; color: var(--text); }
  .rank { list-style: none; margin: 0; padding: 0; }
  .rank li { display: flex; padding: 6px 0; font-size: 13px; border-bottom: 1px solid var(--border); }
  .rank li:last-child { border-bottom: 0; }
  .rank .t { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text); }
  .rank .n { color: var(--accent); font-variant-numeric: tabular-nums; font-weight: 600; }
  .rank .empty { color: var(--muted); justify-content: center; padding: 12px; border-bottom: 0; }
  .snippet {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    font-family: Consolas, Monaco, monospace;
    font-size: 12px;
    color: var(--text);
    overflow-x: auto;
    white-space: pre-wrap;
    margin: 0;
  }
  .tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 16px;
    border-bottom: 1px solid var(--border);
  }
  .tab {
    background: transparent;
    border: 0;
    color: var(--muted);
    padding: 8px 12px;
    cursor: pointer;
    font-size: 13px;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
  }
  .tab.on { color: var(--accent); border-bottom-color: var(--accent); }
  .actions { margin-top: 20px; }
  .err { color: var(--danger); padding: 12px; border: 1px solid var(--danger); border-radius: 6px; }
  .hint { color: var(--muted); font-size: 12px; margin: 0 0 8px; }
  code { background: var(--bg); padding: 1px 6px; border-radius: 4px; font-size: 11px; }
</style>
