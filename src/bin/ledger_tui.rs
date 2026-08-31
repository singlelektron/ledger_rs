use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::Frame;
use std::io;

fn main() -> io::Result<()> {
    ratatui::run(|terminal| {
        loop {
            terminal.draw(render)?;

            let event = event::read()?;

            if matches!(
                event,
                Event::Key(key)
                    if key.kind == KeyEventKind::Press
                        && key.code == KeyCode::Char('q')
            ) {
                return Ok(());
            }
        }
    })
}

fn render(frame: &mut Frame) {
    frame.render_widget("ledger_rs TUI — press q to quit", frame.area());
}
