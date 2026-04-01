mod app;
mod net;
mod ui;

use anyhow::Result;
use app::{App, Modal};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io,
    time::{Duration, Instant},
};

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new()?;
    let tick_rate = Duration::from_millis(800);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if app.modal != Modal::None {
                    handle_modal_key(&mut app, key.code);
                } else {
                    handle_key(&mut app, key.code, key.modifiers);
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.tick()?;
            last_tick = Instant::now();
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') => app.should_quit = true,
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => app.should_quit = true,
        KeyCode::Tab => app.next_tab(),
        KeyCode::Left => app.prev_iface(),
        KeyCode::Right => app.next_iface(),
        KeyCode::Down => app.scroll_down(),
        KeyCode::Up => app.scroll_up(),
        KeyCode::Enter => app.enter_selected(),
        _ => {}
    }
}

fn handle_modal_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Left | KeyCode::Up => app.modal_move(-1),
        KeyCode::Right | KeyCode::Down => app.modal_move(1),
        KeyCode::Enter => app.modal_confirm(),
        KeyCode::Esc | KeyCode::Char('q') => app.modal_cancel(),
        _ => {}
    }
}
