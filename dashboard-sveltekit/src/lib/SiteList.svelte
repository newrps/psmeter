<script>
  import { listSites, createSite } from "./api.js";

  let { selected = $bindable(""), refreshKey = 0 } = $props();

  let sites = $state([]);
  let newDomain = $state("");
  let addMsg = $state("");

  async function load() {
    try {
      sites = await listSites();
    } catch (e) {
      addMsg = e.message;
    }
  }

  $effect(() => {
    refreshKey;
    load();
    const id = setInterval(load, 5000);
    return () => clearInterval(id);
  });

  async function add() {
    if (!newDomain.trim()) return;
    addMsg = "";
    try {
      const d = newDomain.trim().toLowerCase();
      await createSite({ domain: d });
      newDomain = "";
      await load();
      selected = d;
    } catch (e) {
      addMsg = e.message;
    }
  }
</script>

<ul class="list">
  {#each sites as s (s.domain)}
    <li
      class="item"
      class:selected={s.domain === selected}
      on:click={() => (selected = s.domain)}
      role="button"
      tabindex="0"
      on:keydown={(e) => e.key === "Enter" && (selected = s.domain)}
    >
      <span class="dom">{s.domain}</span>
    </li>
  {/each}
  {#if sites.length === 0}
    <li class="empty">등록된 사이트 없음. 페이지에 트래커 박으면 자동 등록됨</li>
  {/if}
</ul>

<div class="add">
  <label for="newdom">새 사이트 추가</label>
  <input
    id="newdom"
    class="form-input"
    placeholder="예: example.com"
    bind:value={newDomain}
    on:keydown={(e) => e.key === "Enter" && add()}
  />
  <button class="btn" style="width:100%; margin-top:6px" on:click={add}>추가</button>
  {#if addMsg}
    <div class="hint" style="color:var(--danger)">{addMsg}</div>
  {/if}
</div>

<style>
  .list { list-style: none; margin: 0; padding: 0; }
  .item {
    padding: 10px 12px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
    margin-bottom: 4px;
    border: 1px solid transparent;
  }
  .item:hover { background: var(--bg); }
  .item.selected { background: var(--bg); border-color: var(--accent); }
  .dom { font-weight: 500; }
  .empty { padding: 12px; text-align: center; color: var(--muted); font-size: 12px; }
  .add { margin-top: 16px; padding-top: 16px; border-top: 1px solid var(--border); }
</style>
