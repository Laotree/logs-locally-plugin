/**
 * llp-chart Cloudflare Worker
 *
 * POST /api/push  — receive pre-rendered SVG + aggregated daily counts from `llp push`
 * GET  /chart.svg — serve the stored SVG publicly (safe to embed in GitHub README)
 *
 * Environment bindings (set via `wrangler secret put` or dashboard):
 *   PUSH_TOKEN  — shared secret; must match `pushToken` in local config.json
 *   CHART       — KV namespace binding (stores "chart.svg" key)
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

      const svg = typeof body.svg === "string" ? body.svg : null;
      if (!svg) return json({ error: "missing svg field" }, 400);

      await env.CHART.put("chart.svg", svg);
      return json({ ok: true });
    }

    // ── GET /chart.svg ────────────────────────────────────────────────────────
    if (request.method === "GET" && url.pathname === "/chart.svg") {
      const svg = (await env.CHART.get("chart.svg")) ?? EMPTY_SVG;
      return new Response(svg, {
        headers: {
          "Content-Type":  "image/svg+xml; charset=utf-8",
          "Cache-Control": "no-cache, max-age=0",
        },
      });
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
