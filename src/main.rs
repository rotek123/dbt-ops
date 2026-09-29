mod api;
mod app;
mod errors;
mod list;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use api::{ApiClient, Run};
use app::App;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Launch the TUI to watch a specific Run ID (you can find Run IDs using the 'list' command)
    #[arg(long)]
    run_id: Option<i64>,

    /// Launch the TUI to watch the latest run for a specific Job ID
    #[arg(short, long)]
    job: Option<i64>,

    /// Polling interval in seconds
    #[arg(short, long, default_value_t = 3)]
    interval: u64,
}

#[derive(Subcommand)]
enum Commands {
    /// List recent dbt runs (alias: ls)
    #[command(alias = "ls")]
    List {
        /// Number of runs to fetch
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: u64,

        /// Job ID to filter runs by (optional)
        #[arg(short, long)]
        job: Option<i64>,
    },

    /// Fetch and analyze errors or warnings for a specific run
    Errors {
        /// The Run ID to analyze
        #[arg(long)]
        run_id: i64,

        /// Include warnings in the output alongside errors
        #[arg(long)]
        include_warnings: bool,

        /// Only show warnings (ignore errors)
        #[arg(long)]
        warnings_only: bool,
    },
}

enum AppEvent {
    Tick,
    RunUpdate(Box<Result<Run>>),
    DashboardUpdate(Box<Result<Vec<Run>>>),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let api_client = ApiClient::new()?;

    // Handle subcommands
    if let Some(command) = cli.command {
        match command {
            Commands::List { limit, job } => {
                list::print_runs(&api_client, limit, job)?;
                return Ok(());
            }
            Commands::Errors {
                run_id,
                include_warnings,
                warnings_only,
            } => {
                errors::print_run_errors(&api_client, run_id, include_warnings, warnings_only)?;
                return Ok(());
            }
        }
    }

    // TUI setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Handle panic to restore terminal
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(panic);
    }));

    let app = App::new();
    let res = run_app(
        &mut terminal,
        app,
        api_client,
        cli.run_id,
        cli.job,
        cli.interval,
    );

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    api_client: ApiClient,
    opt_run_id: Option<i64>,
    opt_job_id: Option<i64>,
    interval: u64,
) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    let (cmd_tx, cmd_rx) = mpsc::channel::<Option<i64>>();
    let tick_rate = Duration::from_millis(250);
    let poll_interval = Duration::from_secs(interval);

    // Polling thread
    let tx_clone = tx.clone();
    let api_client_clone = api_client.clone();

    thread::spawn(move || {
        let mut last_poll = Instant::now() - poll_interval; // Poll immediately
        let mut resolved_run_id = opt_run_id;

        loop {
            if let Ok(new_run_id) = cmd_rx.try_recv() {
                resolved_run_id = new_run_id;
                last_poll = Instant::now() - poll_interval; // force immediate poll
            }

            if last_poll.elapsed() >= poll_interval {
                if let Some(id) = resolved_run_id {
                    let res = api_client_clone.get_run(id);
                    let is_terminal = match &res {
                        Ok(run) => run.status == 10 || run.status == 20 || run.status == 30, // Success, Error, Cancelled
                        Err(_) => false,
                    };
                    if tx_clone.send(AppEvent::RunUpdate(Box::new(res))).is_err() {
                        break;
                    }
                    if is_terminal {
                        // In run view, if terminal we could break, but user might press Esc to go back to dashboard.
                        // Actually, if we break, the polling thread dies. Let's just NOT break, but maybe stop polling?
                        // For simplicity, let's keep polling or just sleep.
                        // We will just not update last_poll so it keeps hitting if we don't change logic,
                        // wait, if we don't break, it'll poll repeatedly? No, last_poll is updated.
                        // Let's just let it poll or maybe we shouldn't break so we can return to dashboard.
                    }
                } else if let Some(j_id) = opt_job_id {
                    let res = api_client_clone.get_latest_run(j_id);
                    match &res {
                        Ok(run) => {
                            resolved_run_id = Some(run.id);
                            if tx_clone
                                .send(AppEvent::RunUpdate(Box::new(Ok(run.clone()))))
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = tx_clone.send(AppEvent::RunUpdate(Box::new(Err(
                                anyhow::anyhow!("Error: {}", e),
                            ))));
                        }
                    }
                } else {
                    // Dashboard mode
                    let res = api_client_clone.list_runs(20, None);
                    if tx_clone
                        .send(AppEvent::DashboardUpdate(Box::new(res)))
                        .is_err()
                    {
                        break;
                    }
                }

                last_poll = Instant::now();
            }
            thread::sleep(Duration::from_millis(100));
        }
    });

    // Tick thread
    let tx_tick = tx.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(tick_rate);
            if tx_tick.send(AppEvent::Tick).is_err() {
                break;
            }
        }
    });

    loop {
        terminal.draw(|f| ui::render(f, &mut app))?;

        if app.should_quit {
            return Ok(());
        }

        // Handle events
        while let Ok(evt) = rx.try_recv() {
            match evt {
                AppEvent::RunUpdate(boxed_res) => match *boxed_res {
                    Ok(run) => {
                        app.update_run(run);
                    }
                    Err(_e) => {
                        // Ignore errors for now, or display them in UI
                    }
                },
                AppEvent::DashboardUpdate(boxed_res) => match *boxed_res {
                    Ok(runs) => {
                        app.runs = runs;
                    }
                    Err(_e) => {}
                },
                AppEvent::Tick => {}
            }
        }

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        app.should_quit = true;
                        continue;
                    }

                    match app.state {
                        app::AppState::Dashboard => match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                            KeyCode::Down | KeyCode::Char('j') => app.dashboard_next(),
                            KeyCode::Up | KeyCode::Char('k') => app.dashboard_prev(),
                            KeyCode::Enter if !app.runs.is_empty() => {
                                let selected_run = &app.runs[app.dashboard_selected_idx];
                                let _ = cmd_tx.send(Some(selected_run.id));
                                app.state = app::AppState::RunView;
                            }
                            _ => {}
                        },
                        app::AppState::RunView => {
                            match key.code {
                                KeyCode::Char('q') => app.should_quit = true,
                                KeyCode::Esc => {
                                    app.state = app::AppState::Dashboard;
                                    let _ = cmd_tx.send(None);
                                }
                                KeyCode::Tab => app.next_step(),
                                KeyCode::BackTab => app.prev_step(),
                                KeyCode::Left | KeyCode::Char('h') => app.scroll_left(),
                                KeyCode::Down | KeyCode::Char('j') => app.scroll_down(1),
                                KeyCode::Up | KeyCode::Char('k') => app.scroll_up(1),
                                KeyCode::Right | KeyCode::Char('l') => app.scroll_right(),
                                KeyCode::Char('H') | KeyCode::Home => app.jump_to_top(),
                                KeyCode::Char('G') | KeyCode::End => app.jump_to_bottom(),
                                KeyCode::Char('d') => app.toggle_log_mode(),
                                KeyCode::Char('r') => {
                                    // Force refresh not implemented for now since we have interval polling
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Event::Mouse(mouse) => match mouse.kind {
                    crossterm::event::MouseEventKind::ScrollDown => {
                        if app.state == app::AppState::RunView {
                            app.scroll_down(3)
                        } else {
                            app.dashboard_next();
                            app.dashboard_next();
                            app.dashboard_next();
                        }
                    }
                    crossterm::event::MouseEventKind::ScrollUp => {
                        if app.state == app::AppState::RunView {
                            app.scroll_up(3)
                        } else {
                            app.dashboard_prev();
                            app.dashboard_prev();
                            app.dashboard_prev();
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
}
