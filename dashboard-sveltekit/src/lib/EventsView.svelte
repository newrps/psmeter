<script>
  import { listEvents, downloadEventsCsv } from "./api.js";

  let { domain, fromMs, toMs } = $props();

  let kind = $state("all");
  let limit = $state(50);
  let offset = $state(0);
  let data = $state(null);
  let loading = $state(false);
  let error = $state("");
  let csvBusy = $state(false);

  async function load() {
    loading = true;
    error = "";
    try {
      data = await listEvents(domain, fromMs, toMs, kind, limit, offset);
    } catch (e) {
      error = e.message;
      data = null;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    domain; fromMs; toMs; kind; limit; offset;
    offset = 0; // 필터 변경시 첫 페이지
    load();
  });

  function changeKind(k) {
    kind = k;
    offset = 0;
  }

  function next() {
    if (data?.has_more) offset = offset + limit;
  }
  function prev() {
    offset = Math.max(0, offset - limit);
  }

  function fmtTs(ms) {
    const d = new Date(ms);
    const pad = (n) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  }

  async function csv() {
    csvBusy = true;
    try {
      await downloadEventsCsv(domain, fromMs, toMs, kind);
    } catch (e) {
      error = e.message;
    } finally {
      csvBusy = false;
    }
  }
</script>

<div class="toolbar">
  <div class="kinds">
    {#each ["all","pageview","custom"] as k}
      <button class="rb" class:on={kind === k} onclick={() => changeKind(k)}>{k}</button>
    {/each}
  </div>
  <div class="right">
    <button class="btn secondary mini" disabled={csvBusy} onclick={csv}>
      {csvBusy ? "다운로드중..." : "CSV 다운로드"}
    </button>
  </div>
</div>

{#if error}
  <div class="err">{error}</div>
{:else if loading && !data}
  <div class="info">불러오는 중...</div>
{:else if data}
  <div class="meta">
    총 {data.total.toLocaleString()}개 · {offset + 1}~{offset + data.events.length} 표시
  </div>

  {#if data.events.length === 0}
    <div class="info">해당 조건의 이벤트가 없습니다</div>
  {:else}
    <div class="tablewrap">
      <table>
        <thead>
          <tr>
            <th class="ts">시각</th>
            <th>kind</th>
            <th>path</th>
            <th>referrer</th>
            <th>device</th>
            <th>browser</th>
          </tr>
        </thead>
        <tbody>
          {#each data.events as e}
            <tr>
              <td class="ts">{fmtTs(e.ts_ms)}</td>
              <td><span class="badge">{e.kind}</span></td>
              <td class="path" title={e.path}>{e.path}</td>
              <td class="ref" title={e.referrer ?? "(direct)"}>{e.referrer ?? "(direct)"}</td>
              <td>{e.device ?? "-"}</td>
              <td>{e.browser ?? "-"}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <div class="pager">
      <button class="btn secondary mini" disabled={offset === 0} onclick={prev}>← 이전</button>
      <span class="page">offset {offset}</span>
      <button class="btn secondary mini" disabled={!data.has_more} onclick={next}>다음 →</button>
    </div>
  {/if}
{/if}

<style>
  .toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
    gap: 8px;
    flex-wrap: wrap;
  }
  .kinds { display: flex; gap: 4px; }
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
  .meta { color: var(--muted); font-size: 12px; margin-bottom: 8px; }
  .tablewrap {
    overflow-x: auto;
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  th, td {
    text-align: left;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }
  th { background: var(--bg); font-weight: 600; color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; }
  tr:last-child td { border-bottom: 0; }
  .ts { font-variant-numeric: tabular-nums; color: var(--muted); }
  .path, .ref {
    max-width: 280px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .badge {
    display: inline-block;
    background: var(--bg);
    border: 1px solid var(--border);
    padding: 1px 8px;
    border-radius: 6px;
    font-size: 11px;
    color: var(--accent);
  }
  .pager {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 12px;
    justify-content: center;
  }
  .page { color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; }
  .err { color: var(--danger); padding: 12px; border: 1px solid var(--danger); border-radius: 6px; }
  .info { color: var(--muted); padding: 20px; text-align: center; }
</style>
