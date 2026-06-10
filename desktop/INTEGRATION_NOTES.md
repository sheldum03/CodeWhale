# CodeWhale Desktop Integration Notes

## P0 status

- `desktop/` is a Tauri 2 + React + TypeScript + Vite app.
- The UI defaults to Simplified Chinese and can switch to English in-process.
- Runtime configuration is read from `desktop/.env` in development.
- `.env.example` contains the supported DeepSeek / OpenAI-compatible fields.
- The Tauri supervisor can start `codewhale.cmd serve --http` with a generated runtime token.
- The supervisor checks `/health` before spawning and attaches to an existing local runtime when available.
- Runtime host is limited to `127.0.0.1`, `localhost`, or `::1` for P0.
- The frontend polls `/health` and `/v1/runtime/info`, and the Tauri side exposes `codewhale.cmd doctor --json`.

## Current machine prerequisites

`npm.cmd run tauri -- info` reports:

- WebView2 is installed.
- Rust, Cargo, and rustup are not installed or not on `PATH`.
- Visual Studio Build Tools with MSVC and Windows SDK are not detected.

Install those prerequisites before running `npm.cmd run desktop:dev` or `cargo check`.

## API notes

- `/health` is public and used for supervision.
- `/v1/runtime/info` is requested with the runtime bearer token even though the current runtime contract allows it during bootstrap.
- No P0 runtime API gap is known yet; P1 should verify thread body shapes against `docs/RUNTIME_API.md` before implementing chat.

## P1 progress

- Project path entry is implemented without a native directory picker yet.
- Recent projects and trusted projects are stored in `localStorage`.
- The workbench can list threads with `GET /v1/threads`.
- The workbench can create a trusted project thread with `POST /v1/threads`.
- The composer sends prompts with `POST /v1/threads/{id}/turns`.
- SSE replay/live listening is wired through `GET /v1/threads/{id}/events?since_seq=0&token=<token>`.
- The active turn can be interrupted through `POST /v1/threads/{id}/turns/{turn_id}/interrupt`.
- Thread selection loads persisted history with `GET /v1/threads/{id}` and renders stored items before live SSE deltas.
- Existing runtime threads can be resumed with `POST /v1/threads/{id}/resume`.
- Legacy sessions are listed with `GET /v1/sessions` and resumed with `POST /v1/sessions/{id}/resume-thread`.
- Language, theme, last project, recent projects, and trusted projects are persisted in browser `localStorage` for the current desktop frontend.
- The settings panel can save provider, base URL, API key, model, runtime host, runtime port, runtime command, language, and theme back to `desktop/.env`.
- The `.env` writer is allow-listed to DeepSeek and OpenAI-compatible providers and keeps runtime host loopback-only.
- Project selection now has a native directory picker command (`select_project_directory`) using `rfd`, with manual path input kept as a fallback.

The request bodies are aligned with the current Rust structs in
`crates/tui/src/runtime_threads.rs`:

- `CreateThreadRequest`: `workspace`, `model`, `mode`, `trust_mode`, `auto_approve`
- `StartTurnRequest`: `prompt`, `model`, `mode`, `trust_mode`, `auto_approve`

Remaining P1 work:

- Persist language, theme, recent project, and trust settings outside browser `localStorage` once the Tauri side can be compiled and verified.
- Verify the full prompt/SSE path against a live runtime once Rust/MSVC prerequisites are installed.
- Compile-check and smoke-test the native Windows directory picker after Rust/MSVC prerequisites are installed.
