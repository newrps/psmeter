<script>
  import { onMount } from "svelte";
  import { isFirstRun, getToken, clearToken, listSites, getDisk } from "$lib/api.js";
  import Setup from "$lib/Setup.svelte";
  import Login from "$lib/Login.svelte";
  import SiteList from "$lib/SiteList.svelte";
  import StatsView from "$lib/StatsView.svelte";

  let phase = $state("checking"); // "checking" | "setup" | "login" | "ready"
  let selected = $state("");
  let refreshKey = $state(0);
  let disk = $state(null);

  async function loadDisk() {
    try { disk = await getDisk(); } catch (_) {}
  }

  onMount(async () => {
    const first = await isFirstRun();
    if (first) {
      phase = "setup";
      return;
    }
    if (getToken()) {
      try {
        await listSites();
        phase = "ready";
        loadDisk();
        setInterval(loadDisk, 60_000);
        return;
      } catch (_) {
        clearToken();
      }
    }
    phase = "login";
  });

  function fmtBytes(n) {
    if (n == null) return "-";
    if (n > 1e9) return `${(n/1e9).toFixed(1)} GB`;
    if (n > 1e6) return `${(n/1e6).toFixed(0)} MB`;
    if (n > 1e3) return `${(n/1e3).toFixed(0)} KB`;
    return `${n} B`;
  }
  function diskPct() {
    if (!disk || !disk.disk_total_bytes) return 0;
    return (1 - disk.disk_free_bytes / disk.disk_total_bytes) * 100;
  }

  function logout() {
    clearToken();
    phase = "login";
    selected = "";
  }

  function handleSiteDeleted() {
    selected = "";
    refreshKey++;
  }
</script>

{#if phase === "checking"}
  <div class="loading">로딩중...</div>
{:else if phase === "setup"}
  <Setup onDone={() => (phase = "ready")} />
{:else if phase === "login"}
  <div class="login-wrap">
    <div class="login-card">
      <h1>Psmeter 📊</h1>
      <p class="sub">분석 대시보드</p>
      <Login onSuccess={() => (phase = "ready")} />
    </div>
  </div>
{:else}
  <div class="layout">
    <aside>
      <h1 class="brand">Psmeter 📊</h1>
      <p class="sub">분석 대시보드</p>
      <div class="top-actions">
        <button class="btn secondary mini" on:click={logout}>로그아웃</button>
      </div>
      <SiteList bind:selected refreshKey={refreshKey} />

      {#if disk}
        <div class="disk" class:warn={diskPct() >= 80}>
          <div class="disk-row">
            <span class="dk">DB</span><span class="dv">{fmtBytes(disk.db_bytes)}</span>
          </div>
          <div class="disk-row">
            <span class="dk">디스크</span>
            <span class="dv">{fmtBytes(disk.disk_free_bytes)} 남음</span>
          </div>
          <div class="bar-wrap">
            <div class="bar" style="width: {diskPct().toFixed(0)}%"></div>
          </div>
          <div class="disk-pct">{diskPct().toFixed(0)}% 사용</div>
        </div>
      {/if}
    </aside>

    <main>
      {#if !selected}
        <div class="empty">
          <div class="big">📊</div>
          <div>왼쪽에서 사이트를 선택하거나 추가하세요</div>
        </div>
      {:else}
        {#key selected}
          <StatsView
            domain={selected}
            onDeleted={handleSiteDeleted}
          />
        {/key}
      {/if}
    </main>
  </div>
{/if}

<style>
  .loading { padding: 40px; text-align: center; color: var(--muted); }
  .login-wrap {
    min-height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    background: linear-gradient(135deg, #0f172a 0%, #1e293b 100%);
  }
  .login-card {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 16px;
    padding: 32px 40px;
    max-width: 400px;
    width: 100%;
  }
  .login-card h1 {
    margin: 0;
    background: linear-gradient(135deg, var(--accent) 0%, #818cf8 100%);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
    font-size: 28px;
  }
  .login-card .sub { color: var(--muted); margin: 4px 0 24px; font-size: 13px; }

  .layout {
    display: grid;
    grid-template-columns: 280px 1fr;
    min-height: 100vh;
  }
  aside {
    background: var(--panel-2);
    border-right: 1px solid var(--border);
    padding: 20px;
    overflow-y: auto;
    max-height: 100vh;
    position: sticky;
    top: 0;
  }
  main { padding: 32px 40px; max-width: 800px; }
  .brand {
    font-size: 20px;
    margin: 0;
    background: linear-gradient(135deg, var(--accent) 0%, #818cf8 100%);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
  }
  .sub { color: var(--muted); margin: 0 0 12px; font-size: 12px; }
  .top-actions { margin-bottom: 16px; }
  .mini { font-size: 11px; padding: 4px 8px; }
  .empty {
    text-align: center;
    padding: 60px 20px;
    color: var(--muted);
    font-size: 14px;
  }
  .empty .big { font-size: 36px; margin-bottom: 12px; opacity: 0.4; }

  .disk {
    margin-top: 20px;
    padding: 10px 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    font-size: 11px;
  }
  .disk.warn { border-color: var(--danger); }
  .disk-row { display: flex; justify-content: space-between; padding: 2px 0; }
  .dk { color: var(--muted); }
  .dv { color: var(--text); font-variant-numeric: tabular-nums; }
  .bar-wrap { height: 4px; background: var(--border); border-radius: 2px; overflow: hidden; margin: 6px 0 3px; }
  .bar { height: 100%; background: var(--accent); }
  .disk.warn .bar { background: var(--danger); }
  .disk-pct { color: var(--muted); text-align: right; font-size: 10px; }

  @media (max-width: 720px) {
    .layout { grid-template-columns: 1fr; }
    aside { max-height: none; position: static; border-right: 0; border-bottom: 1px solid var(--border); }
  }
</style>
