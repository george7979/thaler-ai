# CLAUDE.md — thaler-ai

## Project Overview

**thaler-ai** — document anonymization tool using local LLM (Ollama).

Localhost web app (Rust/axum backend + vanilla HTML/JS frontend served in browser).
Detects sensitive entities (persons, companies, amounts, dates, addresses, IDs) and replaces them with deterministic tokens. All processing is local — data never leaves the machine.

## Tech Stack

- **Backend:** Rust / axum — `src-tauri/src/` (HTTP server on localhost)
- **Frontend:** Vanilla HTML/CSS/JS — `src/` (embedded via `include_str!`, served by axum)
- **NER:** Ollama API, model selectable in UI (no fallback — single model, user's choice)
- **File readers:** calamine (XLSX), zip + quick-xml (DOCX), std::fs (MD/TXT/CSV)
- **Build targets:** .deb (Linux), .msi (Windows) — via GitHub Actions CI/CD

## Architecture

```
[Browser: http://localhost:3000] → [axum REST API] → [Ollama NER] → [token replacement]
         ↓                                                              ↓
   input panel                                                   output panel
         ↓                                                              ↓
   mapping table ←────────────── entity map ──────────────→ deanonymize
```

**Lifecycle:** binary starts HTTP server → opens default browser → heartbeat keeps server alive → closing browser tab shuts down server (120s timeout). Client uses `visibilitychange` to send immediate heartbeat on tab focus (prevents background-tab throttling from killing server).

## Models (Ollama)

- **No default model** — user must select from dropdown after clicking "Sprawdź"
- **No fallback** — single model, user's explicit choice
- **Tested:** Bielik 11B Q8_0 (fast, Polish), Gemma4 26B A4B Q4_K_M (thorough, multilingual)
- **API:** `/api/chat` with system + user messages (not `/api/generate`)
- **Default endpoint:** http://localhost:11434 (configurable in UI or via OLLAMA_URL env)
- **Model list:** populated from Ollama `/api/tags`
- **Robust JSON parsing:** handles truncated responses, missing `]`, markdown code blocks, bare objects

## Commands

```bash
cd src-tauri
cargo build --release    # build binary
cargo run --release      # run locally
cargo deb                # build .deb (Linux) — output: target/debian/*.deb
cargo wix --no-build     # build .msi (Windows) — output: target/wix/*.msi
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Serve frontend (index.html) |
| GET | `/api/check-ollama` | Test Ollama connection |
| GET | `/api/list-models` | List available Ollama models |
| GET | `/api/get-config` | Get current URL + model |
| POST | `/api/set-config` | Set Ollama URL + model |
| POST | `/api/load-file` | Upload file (multipart), returns text |
| POST | `/api/anonymize` | NER + tokenize text (accepts optional `categories` array) |
| GET | `/api/get-mapping` | Get entity mapping table |
| GET | `/api/export-map` | Export full AnonMap JSON |
| GET | `/api/export-anon-native` | Export anonymized DOCX (native format) |
| POST | `/api/deanonymize` | Restore original from tokens + map (text) |
| POST | `/api/deanonymize-docx` | Restore original DOCX (multipart: file + map) |
| GET | `/api/logs` | Poll new log entries |
| POST | `/api/heartbeat` | Keep server alive |
| POST | `/api/shutdown` | Shut down server |

## Security Rules

- All NER processing happens locally via Ollama — no cloud API calls
- Server binds to 127.0.0.1 only — not exposed to network
- Mapping lives in RAM during session; saved to disk only on explicit user action (as .map.json)
- `.gitignore` blocks map files and anonymized outputs
- Anonymized documents are safe to share with cloud AI
- Auto-shutdown: server dies after 120s without heartbeat (refresh-safe, no beforeunload, resilient to background-tab throttling)

## Language

Polish (PL). UI and entity labels in Polish. Tokens: `[TH_OSOBA_001]`, `[TH_FIRMA_002]`, etc. (TH_ prefix prevents collisions with document text).
