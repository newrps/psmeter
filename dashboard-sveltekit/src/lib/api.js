// Psmeter API client
const API = "/api";
const TOKEN_KEY = "psmeter.admin.token";

export function getToken() {
  if (typeof localStorage === "undefined") return "";
  return localStorage.getItem(TOKEN_KEY) || "";
}

export function setToken(t) { localStorage.setItem(TOKEN_KEY, t); }
export function clearToken() { localStorage.removeItem(TOKEN_KEY); }

async function request(method, path, body, opts = {}) {
  const headers = { "Content-Type": "application/json" };
  if (opts.auth !== false) headers.Authorization = `Bearer ${getToken()}`;
  const res = await fetch(`${API}${path}`, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (res.status === 401) throw new Error("토큰이 잘못되었습니다");
  if (res.status === 404) throw new Error("없는 항목입니다");
  if (res.status === 409) throw new Error("이미 존재합니다");
  if (res.status === 503) throw new Error("초기 설정이 필요합니다");
  if (!res.ok) throw new Error(`요청 실패 (${res.status})`);
  if (res.status === 204) return null;
  const ct = res.headers.get("content-type") || "";
  return ct.includes("json") ? res.json() : res.text();
}

export const isFirstRun = async () => {
  try {
    const r = await fetch(`${API}/admin/sites`, {
      headers: { Authorization: "Bearer __probe__" },
    });
    return r.status === 503;
  } catch (_) {
    return false;
  }
};

export const setup = (b) => request("POST", "/setup", b, { auth: false });

// Admin
export const listSites = () => request("GET", "/admin/sites");
export const createSite = (b) => request("POST", "/admin/sites", b);
export const deleteSite = (d) =>
  request("DELETE", `/admin/sites/${encodeURIComponent(d)}`);

export const getStats = (site, fromMs, toMs) => {
  const params = new URLSearchParams({ site });
  if (fromMs != null) params.set("from", fromMs);
  if (toMs != null) params.set("to", toMs);
  return request("GET", `/admin/stats?${params}`);
};

export const getLive = (site) =>
  request("GET", `/admin/live?site=${encodeURIComponent(site)}`);
