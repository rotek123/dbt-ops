# dbt-log-tui — todo

- [x] `cargo init --name dbt-log-tui --bin` in `~/dbt-log-tui/`
- [x] Add dependencies to `Cargo.toml`: `ratatui`, `crossterm`, `ansi-to-tui`, `reqwest` (blocking + json), `serde` + `serde_json`, `serde_yaml`, `clap` (derive), `anyhow`
- [x] `src/api.rs` — load token from `~/.dbt/dbt_cloud.yml`, `Run`/`RunStep` structs, `get_run(run_id)`, `get_latest_run(job_id)`
- [x] `src/app.rs` — `App` state: steps, selected index, per-step log buffers + last-seen length, scroll offset, auto-follow flag, run status/timing
- [x] `src/ui.rs` — header / step list / log pane (ANSI via `ansi-to-tui`) / footer layout
- [x] `src/main.rs` — `clap` CLI (`run_id` optional positional, `--job`, `--interval`), polling thread + `mpsc` channel, `crossterm` event loop, terminal setup/teardown (incl. panic hook restoring the terminal)
- [x] `.gitignore` — ignore `target/`
- [x] `cargo build --release` compiles clean
- [ ] Manual test against live run `52843050`: steps populate, running step auto-focuses, logs append live, ANSI colors render, status flips to terminal on completion
- [ ] Manual test: quit with `q` and with Ctrl+C, confirm terminal restored cleanly both times
- [ ] Manual test: run with no args, confirm it attaches to latest run of job `341099`
