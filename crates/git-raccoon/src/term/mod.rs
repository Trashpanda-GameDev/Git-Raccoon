//! A terminal inside Git Raccoon: a shell in a pseudo-terminal (ConPTY on
//! Windows), its screen kept by Alacritty's terminal emulator, and drawn
//! with egui (`view`).

pub mod link;
pub mod view;

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use alacritty_terminal::event::{Event, EventListener, Notify, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Notifier};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{self, Term, TermMode};
use alacritty_terminal::tty;
use eframe::egui;

/// What to run, and where.
#[derive(Debug, Clone, Default)]
pub struct Start {
    /// The program and its arguments; `None` for the person's usual shell.
    pub shell: Option<(String, Vec<String>)>,
    pub folder: PathBuf,
    /// Variables set for it (and what it runs).
    pub env: HashMap<String, String>,
}

/// Passes the emulator's events on, and wakes the app to draw.
#[derive(Clone)]
pub(crate) struct Proxy {
    tx: Sender<Event>,
    ctx: egui::Context,
}

impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        let _ = self.tx.send(event);
        self.ctx.request_repaint();
    }
}

/// The size of the screen in cells, and of a cell in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub columns: usize,
    pub lines: usize,
    pub cell: egui::Vec2,
}

impl Size {
    fn window(self) -> WindowSize {
        let px = |v: f32| v.round().clamp(1.0, f32::from(u16::MAX)) as u16;
        WindowSize {
            num_lines: u16::try_from(self.lines).unwrap_or(u16::MAX),
            num_cols: u16::try_from(self.columns).unwrap_or(u16::MAX),
            cell_width: px(self.cell.x),
            cell_height: px(self.cell.y),
        }
    }
}

/// A running terminal.
pub struct Terminal {
    term: Arc<FairMutex<Term<Proxy>>>,
    notifier: Notifier,
    events: Receiver<Event>,
    size: Size,
    /// What it's called: the program's own title, else the shell's name.
    pub title: String,
    /// The shell has ended (with its exit code, when known).
    pub ended: Option<Option<i32>>,
    /// Text the program asked to be copied, to hand to the clipboard.
    pub copied: Option<String>,
    /// When the person last typed or drew a selection (the cursor stays
    /// solid for a moment after).
    pub(crate) last_input: f64,
}

impl Terminal {
    /// Starts the shell in its folder.
    pub fn start(ctx: &egui::Context, start: &Start, id: u64) -> Result<Terminal, String> {
        // What it understands, so programs (a pager, an editor) draw for it
        // rather than for whatever started the app (often nothing).
        let mut env = HashMap::from([
            ("TERM".to_string(), "xterm-256color".to_string()),
            ("COLORTERM".to_string(), "truecolor".to_string()),
        ]);
        env.extend(start.env.clone());
        let options = tty::Options {
            shell: start
                .shell
                .clone()
                .map(|(program, args)| tty::Shell::new(program, args)),
            working_directory: Some(start.folder.clone()),
            drain_on_exit: true,
            env,
            #[cfg(windows)]
            escape_args: true,
        };
        let size = Size {
            columns: 80,
            lines: 24,
            cell: egui::vec2(8.0, 16.0),
        };
        let pty = tty::new(&options, size.window(), id)
            .map_err(|e| format!("the terminal couldn't start: {e}"))?;
        let (tx, events) = mpsc::channel();
        let proxy = Proxy {
            tx,
            ctx: ctx.clone(),
        };
        let config = term::Config {
            scrolling_history: 10_000,
            ..term::Config::default()
        };
        let term = Term::new(
            config,
            &TermSize::new(size.columns, size.lines),
            proxy.clone(),
        );
        let term = Arc::new(FairMutex::new(term));
        let event_loop = EventLoop::new(Arc::clone(&term), proxy, pty, true, false)
            .map_err(|e| format!("the terminal couldn't start: {e}"))?;
        let notifier = Notifier(event_loop.channel());
        event_loop.spawn();
        let title = start
            .shell
            .as_ref()
            .map_or_else(default_shell_name, |(p, _)| {
                std::path::Path::new(p)
                    .file_stem()
                    .map_or_else(|| p.clone(), |s| s.to_string_lossy().to_string())
            });
        Ok(Terminal {
            term,
            notifier,
            events,
            size,
            title,
            ended: None,
            copied: None,
            last_input: 0.0,
        })
    }

    /// Takes in what the emulator said since the last frame.
    pub fn poll(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Title(t) => self.title = t,
                Event::ChildExit(status) => self.ended = Some(status.code()),
                Event::Exit => {
                    if self.ended.is_none() {
                        self.ended = Some(None);
                    }
                }
                Event::PtyWrite(text) => self.write(text.into_bytes()),
                Event::ClipboardStore(_, text) => self.copied = Some(text),
                Event::ColorRequest(index, answer) => {
                    let c = view::colour_of_index(index);
                    let rgb = alacritty_terminal::vte::ansi::Rgb {
                        r: c.r(),
                        g: c.g(),
                        b: c.b(),
                    };
                    self.write(answer(rgb).into_bytes());
                }
                Event::TextAreaSizeRequest(answer) => {
                    self.write(answer(self.size.window()).into_bytes());
                }
                _ => {}
            }
        }
    }

    /// Sends what was typed (or pasted) to the program.
    pub fn write(&mut self, bytes: impl Into<Cow<'static, [u8]>>) {
        if self.ended.is_none() {
            self.notifier.notify(bytes);
        }
    }

    /// Sends typed text, back at the bottom of the scrollback.
    pub fn type_text(&mut self, text: &str) {
        self.scroll(Scroll::Bottom);
        self.term.lock().selection = None;
        self.write(text.as_bytes().to_vec());
    }

    /// Pastes text: marked as a paste when the program asked for that.
    pub fn paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        if self.mode().contains(TermMode::BRACKETED_PASTE) {
            // A paste can't end the paste early.
            let text = text.replace("\x1b[201~", "");
            self.type_text(&format!("\x1b[200~{text}\x1b[201~"));
        } else {
            self.type_text(&text);
        }
    }

    pub fn size(&self) -> Size {
        self.size
    }

    /// Fits the screen to `size` (cells), telling the program.
    pub fn resize(&mut self, size: Size) {
        if size == self.size || size.columns < 2 || size.lines < 1 {
            return;
        }
        self.size = size;
        self.notifier.on_resize(size.window());
        self.term
            .lock()
            .resize(TermSize::new(size.columns, size.lines));
    }

    pub fn scroll(&mut self, scroll: Scroll) {
        self.term.lock().scroll_display(scroll);
    }

    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    /// The screen, locked for drawing or selecting.
    pub(crate) fn lock(&self) -> parking_lot::MutexGuard<'_, Term<Proxy>> {
        self.term.lock()
    }

    /// The text selected, if any.
    pub fn selected(&self) -> Option<String> {
        self.term
            .lock()
            .selection_to_string()
            .filter(|s| !s.is_empty())
    }

    /// The lines on screen now, as text (trailing spaces trimmed).
    pub fn screen_text(&self) -> String {
        let term = self.term.lock();
        let grid = term.grid();
        let offset = grid.display_offset() as i32;
        (0..grid.screen_lines())
            .map(|row| {
                let line = Line(row as i32 - offset);
                (0..grid.columns())
                    .map(|c| grid[Point::new(line, Column(c))].c)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end()
            .to_string()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self
            .notifier
            .0
            .send(alacritty_terminal::event_loop::Msg::Shutdown);
    }
}

/// The usual shell's name, for a title before it gives one.
fn default_shell_name() -> String {
    if cfg!(windows) {
        "PowerShell".into()
    } else {
        std::env::var("SHELL")
            .ok()
            .and_then(|s| s.rsplit('/').next().map(str::to_string))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "shell".into())
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    fn run(script: &str) -> Terminal {
        let ctx = egui::Context::default();
        let mut t = Terminal::start(
            &ctx,
            &Start {
                shell: Some(("/bin/sh".into(), vec!["-c".into(), script.into()])),
                folder: std::env::temp_dir(),
                env: HashMap::from([("RACCOON".into(), "yes".into())]),
            },
            1,
        )
        .unwrap();
        let begun = std::time::Instant::now();
        while t.ended.is_none() && begun.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(20));
            t.poll();
        }
        assert!(t.ended.is_some(), "it ended");
        t
    }

    #[test]
    fn runs_a_shell_and_keeps_its_screen() {
        let t = run("printf 'one\\ntwo %s\\n' \"$RACCOON\"; printf '\\033[31mred\\033[0m'; exit 3");
        let screen = t.screen_text();
        assert!(screen.starts_with("one\ntwo yes\nred"), "{screen:?}");
        assert_eq!(t.ended, Some(Some(3)));
        // Red, from the escape code.
        let term = t.lock();
        let cell = &term.grid()[Point::new(Line(2), Column(0))];
        assert_eq!(
            cell.fg,
            alacritty_terminal::vte::ansi::Color::Named(
                alacritty_terminal::vte::ansi::NamedColor::Red
            )
        );
    }

    #[test]
    fn types_into_it_and_follows_its_size() {
        let ctx = egui::Context::default();
        let mut t = Terminal::start(
            &ctx,
            &Start {
                shell: Some(("/bin/sh".into(), Vec::new())),
                folder: std::env::temp_dir(),
                env: HashMap::new(),
            },
            2,
        )
        .unwrap();
        t.resize(Size {
            columns: 50,
            lines: 10,
            cell: egui::vec2(8.0, 16.0),
        });
        t.type_text("stty size; echo done-$((40+2))\r");
        let begun = std::time::Instant::now();
        while !t.screen_text().contains("done-42")
            && begun.elapsed() < std::time::Duration::from_secs(10)
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
            t.poll();
        }
        let screen = t.screen_text();
        assert!(
            screen.contains("10 50"),
            "the program sees its size: {screen}"
        );
        assert!(screen.contains("done-42"), "{screen}");
        t.type_text("exit\r");
    }
}
