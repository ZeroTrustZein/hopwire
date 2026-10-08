# AegisGraph console

Investigator UI for the AegisGraph engine: alert queue, alert detail with the explanation subgraph,
dispositions, and cases. Vite + React 19 + TypeScript (strict) + Cytoscape.js + react-router.
Codes against the frozen contract in [`../docs/api.md`](../docs/api.md); types live in `src/api/types.ts`.

## Dev

```bash
npm install
npm run dev            # http://localhost:5173, proxies /v1 -> http://localhost:8080
```

Sign in with the server's `AEGIS_API_KEY`. The key is validated with `GET /v1/stats` and kept in
`sessionStorage` (cleared when the tab closes). Any `401` sends you back to the login screen.

## Mock mode (no backend)

```bash
VITE_MOCK=1 npm run dev        # PowerShell: $env:VITE_MOCK='1'; npm run dev
```

`src/api/mock.ts` serves in-memory fixtures at the client level: a 3-hop cycle, a mule fan-in from
15 senders, a shared device/IP ring, ~34 lower-signal alerts for paging and filters, and a case with notes.
Any non-empty key logs in. Mutations (disposition, escalate, case edits, notes) work, but they reset on a full page reload.

## Build

```bash
npm run build          # tsc type-check + vite build -> dist/
npm run preview
```

`dist/` is static. Serve it behind nginx with an SPA fallback (`try_files $uri /index.html`), and
either proxy `/v1` to the engine or set `VITE_API_URL` at build time.

## Env vars (build-time)

| Var | Default | Meaning |
|---|---|---|
| `VITE_API_URL` | `/` (same origin) | Engine base URL. If cross-origin, set `AEGIS_CORS_ORIGIN` on the server. |
| `VITE_MOCK` | unset | `1` = use built-in fixtures instead of the API. |
