(function () {
  "use strict";
  var s = document.currentScript;
  if (!s) return;

  // server: data-server | script.src 의 origin
  var SRV = (s.dataset.server || (function () {
    try { var u = new URL(s.src); return u.origin; } catch (_) { return ""; }
  })()).replace(/\/+$/, "");
  if (!SRV) return;

  // site: data-site | 현재 hostname
  var SITE = s.dataset.site || location.hostname || "unknown";
  var TRACK_URL = SRV + "/api/track";

  function path() {
    return location.pathname + (location.search || "");
  }

  function send(kind) {
    var body = JSON.stringify({
      site: SITE,
      kind: kind || "pageview",
      path: path(),
      referrer: document.referrer || null
    });
    try {
      // sendBeacon: fire-and-forget, 페이지 이동/종료 시에도 보장
      if (navigator.sendBeacon) {
        navigator.sendBeacon(TRACK_URL, new Blob([body], { type: "application/json" }));
        return;
      }
    } catch (_) {}
    // fallback
    fetch(TRACK_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: body,
      keepalive: true
    }).catch(function () {});
  }

  // 초기 pageview
  send("pageview");

  // SPA: pushState / replaceState / popstate 후크
  var origPush = history.pushState;
  var origReplace = history.replaceState;
  history.pushState = function () { origPush.apply(this, arguments); send("pageview"); };
  history.replaceState = function () { origReplace.apply(this, arguments); send("pageview"); };
  window.addEventListener("popstate", function () { send("pageview"); });

  // 디버깅/커스텀 이벤트
  window.psmeter = function (eventName) { send(eventName || "custom"); };
})();
