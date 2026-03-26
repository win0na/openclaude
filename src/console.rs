use std::env;
use std::io::{IsTerminal, stderr, stdout};

#[derive(Clone, Copy)]
pub struct Style {
    enabled: bool,
}

impl Style {
    pub fn plain() -> Self {
        Self { enabled: false }
    }

    pub fn color() -> Self {
        Self { enabled: true }
    }

    pub fn paint(self, text: &str, ansi: &str) -> String {
        if self.enabled {
            format!("\x1b[{ansi}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn title(self, text: &str) -> String {
        self.paint(text, "1")
    }

    pub fn heading(self, text: &str) -> String {
        self.paint(text, "1")
    }

    pub fn command(self, text: &str) -> String {
        self.paint(text, "3")
    }

    pub fn option(self, text: &str) -> String {
        self.paint(text, "3")
    }
}

pub fn stdout_style() -> Style {
    if stdout_color_enabled() {
        Style::color()
    } else {
        Style::plain()
    }
}

pub fn stdout_color_enabled() -> bool {
    env::var_os("NO_COLOR").is_none() && stdout().is_terminal()
}

pub fn stderr_color_enabled() -> bool {
    env::var_os("NO_COLOR").is_none() && stderr().is_terminal()
}
