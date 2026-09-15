# dbt Cloud run-log TUI (Rust)

## Context

The dbt Cloud web UI's new interface makes it hard to follow a run's logs live —
especially useful right now, mid-backfill of `int_blink__ads`, where each chunk
runs for 10+ minutes and the only way to check progress has been manually
polling the Admin API and BigQuery `INFORMATION_SCHEMA` by hand. The user wants
a terminal tool, written in Rust, that shows a dbt Cloud run's step-by-step logs
live, refreshing automatically, without touching the web UI at all.

There is no existing polling/TUI/log-tailing tooling anywhere in the adp-dbt
repo and no Rust toolchain used there — this is a personal tool, kept outside
the repo at `~/dbt-log-tui/`, not part of adp-dbt. `cargo` is already installed
locally (1.88.0-nightly), so no new install is required to build it.

## Approach

Standalone Rust binary crate at `~/dbt-log-tui/`.

**Data source:** dbt Cloud Admin API, same auth pattern used by every script in
adp-dbt's `aller_data_platform/ops/*.zsh` — read the token from
`~/.dbt/dbt_cloud.yml` (`projects[0].token-value`), hit
`https://emea.dbt.com/api/v2/accounts/517/...`. No new credentials or config.

- `GET /runs/{run_id}/?include_related=["run_steps"]` — single call returns the
  run's status/timing plus every step (`Clone git repository`, `dbt deps`,
  `Invoke dbt with ...`) each with its full log text so far. This is the only
  endpoint needed; there's no separate log-streaming/websocket API.
- Each step's `logs` field is append-only plain text containing raw ANSI SGR
  escape codes (confirmed by inspecting a live run today) — a poll just needs
  to diff `logs.len()` against what was last rendered and append the new
  slice, no need to reparse the whole buffer each tick.
- When no run ID is given, resolve the latest run for a job via
  `GET /runs/?job_definition_id={job}&order_by=-id&limit=1` (default job
  `341099`, prod — override with `--job 341096` for staging).

**Architecture:** a background thread polls the API on a fixed interval
(default 3s, `--interval`) and pushes parsed state over an `mpsc` channel;
the main thread runs the `ratatui` + `crossterm` render loop, doing a
non-blocking key-event poll (~150ms) and a non-blocking channel `try_recv()`
each tick, then redrawing. Plain `reqwest::blocking` is enough — no `tokio`
needed since there's exactly one polling thread and one UI thread. Polling
slows to a single final fetch once the run reaches a terminal status
(Success/Error/Cancelled).

**Layout:**
- Header: run status, elapsed/total duration, model + command, git branch/sha.
- Step list (left/top): one row per step with a status glyph; the
  currently-`Running` step auto-focuses on first load.
- Log pane: the selected step's log, ANSI-rendered via the `ansi-to-tui` crate
  (converts raw SGR byte streams into styled `ratatui::text::Text` — the
  standard companion crate for exactly this, actively maintained under the
  `ratatui` GitHub org). Auto-scrolls to the bottom as new lines arrive;
  scrolling up manually pauses auto-follow (classic `tail -f` pattern) until
  `G`/`End` jumps back to the bottom.
- Footer: keybindings — `q`/`Esc` quit, `Tab`/`Shift+Tab` switch step,
  `↑↓`/`j k` scroll, `G` jump to bottom + resume auto-follow, `r` force
  refresh now.

**Crates:** `ratatui`, `crossterm` (backend), `ansi-to-tui` (log color
rendering), `reqwest` (blocking + json features), `serde` + `serde_json`
(API responses), `serde_yaml` (reading `dbt_cloud.yml`), `clap` (derive, CLI
args), `anyhow` (error handling).

**Files:**
- `~/dbt-log-tui/Cargo.toml`
- `~/dbt-log-tui/src/main.rs` — CLI parsing (`clap`), sets up the polling
  thread + channel, runs the event loop, restores the terminal on exit/panic.
- `~/dbt-log-tui/src/api.rs` — token loading, `Run`/`RunStep` structs, the two
  GET calls.
- `~/dbt-log-tui/src/app.rs` — `App` state (steps, selected index, per-step
  log buffers + last-seen length, scroll offset, auto-follow flag, run
  status/timing).
- `~/dbt-log-tui/src/ui.rs` — `ratatui` widget rendering for the layout above.
- `~/dbt-log-tui/.gitignore` — ignore `target/`.

**Not in v1** (flagging so it's an explicit choice, not an oversight): a
side-panel with the underlying BigQuery job's live bytes/slot-ms — that needs
a second data source (`INFORMATION_SCHEMA.JOBS_BY_PROJECT`, no direct link
from a dbt Cloud run to its BQ job IDs without a time-window heuristic like
the one used manually earlier today) and doubles the polling logic. Natural
follow-up once the log viewer itself is proven useful, not blocking this.

## Verification

1. `cargo build --release` from `~/dbt-log-tui/` compiles clean.
2. Run it against the currently in-flight backfill run (`dbt-log-tui 52843050`)
   and confirm: steps populate, the running step auto-focuses, logs append
   live as the run progresses, ANSI colors render (not raw escape codes), and
   the header status flips to Success/Error when it finishes.
3. Confirm terminal state is restored cleanly on `q` and on Ctrl+C (raw mode
   / alternate screen must not leak into the shell).
4. Run with no arguments (`dbt-log-tui`) and confirm it resolves and attaches
   to the latest run of job `341099` automatically.
