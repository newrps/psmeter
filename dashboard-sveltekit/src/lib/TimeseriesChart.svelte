<script>
  let { points = [], bucket = "hour" } = $props();

  const W = 800;
  const H = 220;
  const PAD = { top: 16, right: 16, bottom: 28, left: 44 };

  let hover = $state(null); // index 또는 null

  let plotW = $derived(W - PAD.left - PAD.right);
  let plotH = $derived(H - PAD.top - PAD.bottom);

  let maxY = $derived.by(() => {
    if (!points.length) return 1;
    const m = Math.max(
      ...points.map((p) => Math.max(p.pageviews, p.visitors)),
      1
    );
    return niceCeil(m);
  });

  function niceCeil(n) {
    if (n <= 5) return 5;
    const pow = Math.pow(10, Math.floor(Math.log10(n)));
    const r = n / pow;
    let nice;
    if (r <= 1) nice = 1;
    else if (r <= 2) nice = 2;
    else if (r <= 5) nice = 5;
    else nice = 10;
    return nice * pow;
  }

  function xOf(i) {
    if (points.length <= 1) return PAD.left + plotW / 2;
    return PAD.left + (plotW * i) / (points.length - 1);
  }
  function yOf(v) {
    return PAD.top + plotH - (plotH * v) / maxY;
  }

  let pvPath = $derived(
    points.map((p, i) => `${i === 0 ? "M" : "L"}${xOf(i)},${yOf(p.pageviews)}`).join(" ")
  );
  let pvArea = $derived.by(() => {
    if (!points.length) return "";
    const top = points
      .map((p, i) => `${i === 0 ? "M" : "L"}${xOf(i)},${yOf(p.pageviews)}`)
      .join(" ");
    return `${top} L${xOf(points.length - 1)},${yOf(0)} L${xOf(0)},${yOf(0)} Z`;
  });
  let vsPath = $derived(
    points.map((p, i) => `${i === 0 ? "M" : "L"}${xOf(i)},${yOf(p.visitors)}`).join(" ")
  );

  // Y축 눈금 (0, max/2, max)
  let yTicks = $derived([0, Math.round(maxY / 2), maxY]);

  // X축 라벨: 간격 자동
  function xLabel(p) {
    const d = new Date(p.ts_ms);
    if (bucket === "hour") {
      return `${String(d.getHours()).padStart(2, "0")}:00`;
    }
    return `${d.getMonth() + 1}/${d.getDate()}`;
  }
  let xTickIdxs = $derived.by(() => {
    if (!points.length) return [];
    const desired = bucket === "hour" ? 6 : 7;
    const step = Math.max(1, Math.floor(points.length / desired));
    const idxs = [];
    for (let i = 0; i < points.length; i += step) idxs.push(i);
    if (idxs[idxs.length - 1] !== points.length - 1) idxs.push(points.length - 1);
    return idxs;
  });

  function tooltipText(p) {
    const d = new Date(p.ts_ms);
    const dateStr =
      bucket === "hour"
        ? `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, "0")}:00`
        : `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()}`;
    return { dateStr, pv: p.pageviews, vs: p.visitors };
  }

  function onMove(e) {
    if (!points.length) return;
    const svg = e.currentTarget;
    const rect = svg.getBoundingClientRect();
    const x = ((e.clientX - rect.left) / rect.width) * W;
    if (x < PAD.left - 8 || x > W - PAD.right + 8) {
      hover = null;
      return;
    }
    const ratio = (x - PAD.left) / plotW;
    const i = Math.round(ratio * (points.length - 1));
    hover = Math.max(0, Math.min(points.length - 1, i));
  }
  function onLeave() { hover = null; }

  let hp = $derived(hover != null ? points[hover] : null);
  let htxt = $derived(hp ? tooltipText(hp) : null);
</script>

<div class="chart-wrap">
  <div class="legend">
    <span class="leg pv"><span class="sw"></span>페이지뷰</span>
    <span class="leg vs"><span class="sw"></span>방문자</span>
  </div>

  <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" onmousemove={onMove} onmouseleave={onLeave}>
    <!-- Y grid -->
    {#each yTicks as t}
      <line x1={PAD.left} x2={W - PAD.right} y1={yOf(t)} y2={yOf(t)} class="grid"/>
      <text x={PAD.left - 8} y={yOf(t) + 4} class="ylbl">{t}</text>
    {/each}

    <!-- X labels -->
    {#each xTickIdxs as i}
      <text x={xOf(i)} y={H - 8} class="xlbl">{xLabel(points[i])}</text>
    {/each}

    {#if points.length}
      <!-- 페이지뷰 area + line -->
      <path d={pvArea} class="area pv"/>
      <path d={pvPath} class="line pv"/>
      <!-- 방문자 line -->
      <path d={vsPath} class="line vs"/>

      {#if hover != null}
        <line x1={xOf(hover)} x2={xOf(hover)} y1={PAD.top} y2={H - PAD.bottom} class="cursor"/>
        <circle cx={xOf(hover)} cy={yOf(points[hover].pageviews)} r="4" class="dot pv"/>
        <circle cx={xOf(hover)} cy={yOf(points[hover].visitors)} r="4" class="dot vs"/>
      {/if}
    {:else}
      <text x={W / 2} y={H / 2} class="empty">데이터 없음</text>
    {/if}
  </svg>

  {#if htxt}
    <div class="tip" style="left:{(xOf(hover) / W) * 100}%">
      <div class="tip-date">{htxt.dateStr}</div>
      <div class="tip-row"><span class="sw pv-bg"></span>페이지뷰 <b>{htxt.pv}</b></div>
      <div class="tip-row"><span class="sw vs-bg"></span>방문자 <b>{htxt.vs}</b></div>
    </div>
  {/if}
</div>

<style>
  .chart-wrap {
    position: relative;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 16px;
    margin-bottom: 20px;
  }
  .legend {
    display: flex;
    gap: 16px;
    margin-bottom: 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .leg { display: inline-flex; align-items: center; gap: 6px; }
  .sw { display: inline-block; width: 10px; height: 10px; border-radius: 2px; }
  .leg.pv .sw, .pv-bg { background: var(--accent); }
  .leg.vs .sw, .vs-bg { background: var(--ok, #59c275); }
  svg { width: 100%; height: 220px; display: block; cursor: crosshair; }
  .grid { stroke: var(--border); stroke-width: 1; }
  .ylbl { fill: var(--muted); font-size: 10px; text-anchor: end; font-variant-numeric: tabular-nums; }
  .xlbl { fill: var(--muted); font-size: 10px; text-anchor: middle; }
  .area.pv { fill: var(--accent); fill-opacity: 0.12; }
  .line { fill: none; stroke-width: 2; }
  .line.pv { stroke: var(--accent); }
  .line.vs { stroke: var(--ok, #59c275); }
  .cursor { stroke: var(--muted); stroke-width: 1; stroke-dasharray: 3 3; opacity: 0.6; }
  .dot { stroke: var(--panel); stroke-width: 2; }
  .dot.pv { fill: var(--accent); }
  .dot.vs { fill: var(--ok, #59c275); }
  .empty { fill: var(--muted); font-size: 13px; text-anchor: middle; }
  .tip {
    position: absolute;
    top: 32px;
    transform: translateX(-50%);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px 10px;
    font-size: 11px;
    pointer-events: none;
    white-space: nowrap;
    box-shadow: 0 2px 8px rgba(0,0,0,0.2);
  }
  .tip-date { color: var(--muted); font-size: 10px; margin-bottom: 4px; }
  .tip-row { display: flex; align-items: center; gap: 6px; color: var(--text); }
  .tip-row b { font-variant-numeric: tabular-nums; }
</style>
