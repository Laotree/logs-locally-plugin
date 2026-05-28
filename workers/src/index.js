/**
 * llp-chart Cloudflare Worker
 *
 * Single-user (direct push):
 *   POST /api/push  { svg }          → stores as "chart.svg"
 *   GET  /chart.svg                  → serves stored SVG
 *
 * Multi-user (via llp relay):
 *   POST /api/push  { hash, svg }    → stores as "chart:<hash>"
 *   GET  /chart/:hash.svg            → serves per-user SVG
 *
 * Environment bindings:
 *   PUSH_TOKEN  — shared secret for /api/push (set via wrangler secret put)
 *   CHART       — KV namespace binding
 */

const EMPTY_SVG = `<svg xmlns="http://www.w3.org/2000/svg" width="400" height="60"
  style="background:#0d1017;border-radius:6px">
  <text x="16" y="36" fill="rgba(255,255,255,0.3)"
    font-family="monospace" font-size="12">No data yet — run llp push</text>
</svg>`;

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    // ── POST /api/push ────────────────────────────────────────────────────────
    if (request.method === "POST" && url.pathname === "/api/push") {
      const token = env.PUSH_TOKEN ?? "";
      const auth  = request.headers.get("Authorization") ?? "";
      if (token && auth !== `Bearer ${token}`) {
        return json({ error: "unauthorized" }, 401);
      }

      let body;
      try { body = await request.json(); }
      catch { return json({ error: "invalid JSON" }, 400); }

      const svg  = typeof body.svg  === "string" ? body.svg  : null;
      const hash = typeof body.hash === "string" ? body.hash : null;

      if (!svg) return json({ error: "missing svg field" }, 400);

      // Multi-user: store under "chart:<hash>"; single-user: store as "chart.svg"
      const key = hash ? `chart:${hash}` : "chart.svg";
      await env.CHART.put(key, svg);
      return json({ ok: true });
    }

    // ── GET /chart/:hash.svg ──────────────────────────────────────────────────
    const hashMatch = url.pathname.match(/^\/chart\/([a-f0-9]{16})\.svg$/);
    if (request.method === "GET" && hashMatch) {
      const svg = (await env.CHART.get(`chart:${hashMatch[1]}`)) ?? EMPTY_SVG;
      return svgResponse(svg);
    }

    // ── GET /chart.svg  (single-user / backward compat) ───────────────────────
    if (request.method === "GET" && url.pathname === "/chart.svg") {
      const svg = (await env.CHART.get("chart.svg")) ?? EMPTY_SVG;
      return svgResponse(svg);
    }

    return new Response("Not found", { status: 404 });
  },
};

function json(body, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function svgResponse(svg) {
  return new Response(svg, {
    headers: {
      "Content-Type":  "image/svg+xml; charset=utf-8",
      "Cache-Control": "no-cache, max-age=0",
    },
  });
}
