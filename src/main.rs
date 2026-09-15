mod api;
mod app;
mod ui;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use api::{ApiClient, Run};
use app::App;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Optional Run ID. If omitted, fetches the latest run for the job.
    run_id: Option<i64>,

    /// Job ID to fetch the latest run for (if run_id is omitted)
    #[arg(short, long, default_value_t = 341099)]
    job: i64,

    /// Polling interval in seconds
    #[arg(short, long, default_value_t = 3)]
    interval: u64,
}

enum AppEvent {
    Tick,
    RunUpdate(Result<Run>),
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Setup terminal
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
    let res = run_app(&mut terminal, app, args);

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
    args: Args,
) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    let tick_rate = Duration::from_millis(250);
    let poll_interval = Duration::from_secs(args.interval);

    let api_client = ApiClient::new()?;

    // Polling thread
    let tx_clone = tx.clone();
    thread::spawn(move || {
        let mut last_poll = Instant::now() - poll_interval; // Poll immediately
        let mut resolved_run_id = args.run_id;
        
        loop {
            if last_poll.elapsed() >= poll_interval {
                let res = if let Some(id) = resolved_run_id {
                    api_client.get_run(id)
                } else {
                    match api_client.get_latest_run(args.job) {
                        Ok(run) => {
                            resolved_run_id = Some(run.id);
                            Ok(run)
                        }
                        Err(e) => Err(e),
                    }
                };

                let is_terminal = match &res {
                    Ok(run) => run.status == 10 || run.status == 20 || run.status == 30, // Success, Error, Cancelled
                    Err(_) => false,
                };

                if tx_clone.send(AppEvent::RunUpdate(res)).is_err() {
                    break;
                }
                last_poll = Instant::now();
                
                if is_terminal {
                    break;
                }
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
                AppEvent::RunUpdate(Ok(run)) => {
                    app.update_run(run);
                }
                AppEvent::RunUpdate(Err(_e)) => {
                    // Ignore errors for now, or display them in UI
                }
                AppEvent::Tick => {}
            }
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    app.should_quit = true;
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                    KeyCode::Tab => app.next_step(),
                    KeyCode::BackTab => app.prev_step(),
                    KeyCode::Up | KeyCode::Char('k') => app.scroll_up(),
                    KeyCode::Down | KeyCode::Char('j') => app.scroll_down(),
                    KeyCode::Char('G') | KeyCode::End => app.jump_to_bottom(),
                    KeyCode::Char('r') => {
                        // Force refresh not implemented for now since we have interval polling
                    }
                    _ => {}
                }
            }
        }
    }
}
