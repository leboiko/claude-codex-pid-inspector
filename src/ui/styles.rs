use ratatui::style::{Color, Modifier, Style};

/// Selectable color themes.
///
/// Changing the theme regenerates a [`Palette`] which is then threaded through
/// every renderer. Semantic colors (e.g. red-for-warning in the kill popup,
/// green/yellow/red thresholds in the status bar) are NOT themed — only the
/// neutral UI chrome and the Claude/Codex brand accents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Default,
    Dracula,
    Solarized,
}

impl Theme {
    /// All themes in user-facing display order.
    pub const ALL: [Theme; 3] = [Theme::Default, Theme::Dracula, Theme::Solarized];

    /// Short label shown in the config popup.
    pub fn label(self) -> &'static str {
        match self {
            Theme::Default => "Default",
            Theme::Dracula => "Dracula",
            Theme::Solarized => "Solarized Dark",
        }
    }
}

/// Style of the time-series charts in the detail view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphStyle {
    /// Scatter plot with dot markers (ratatui `GraphType::Scatter` + `Marker::Dot`).
    #[default]
    Dots,
    /// Vertical bars, one per sample, drawn with Unicode block characters
    /// (ratatui `GraphType::Bar` + `Marker::Bar`).
    Bars,
}

impl GraphStyle {
    pub const ALL: [GraphStyle; 2] = [GraphStyle::Dots, GraphStyle::Bars];

    pub fn label(self) -> &'static str {
        match self {
            GraphStyle::Dots => "Dots",
            GraphStyle::Bars => "Bars",
        }
    }
}

/// Concrete color values for one theme.
///
/// A `Palette` is constructed once via [`Palette::from_theme`] and stored on
/// the `App` struct. All render functions take `&Palette` so changing the
/// theme is just a matter of swapping the struct.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Brand accent for Claude Code processes (root rows, CPU chart).
    pub claude: Color,
    /// Brand accent for Codex CLI processes (root rows, memory chart).
    pub codex: Color,
    /// Dim text color for child (non-root) table rows.
    pub child: Color,
    /// Border color for every bordered block.
    pub border: Color,
    /// Foreground color for block/widget titles.
    pub title: Color,
    /// Foreground for column headers and emphasized labels.
    pub header: Color,
    /// Background color for the highlighted table row.
    pub selected_bg: Color,
    /// Foreground for key/value labels and chart-axis tick text.
    pub label: Color,
}

impl Palette {
    /// Build a [`Palette`] for the given [`Theme`].
    pub fn from_theme(theme: Theme) -> Self {
        match theme {
            Theme::Default => Self {
                // Original agentop palette — warm orange Claude, bright green Codex.
                claude: Color::Rgb(204, 120, 50),
                codex: Color::Rgb(100, 200, 100),
                child: Color::Rgb(160, 160, 160),
                border: Color::Rgb(90, 90, 90),
                title: Color::Rgb(240, 240, 240),
                header: Color::Rgb(235, 206, 50),
                selected_bg: Color::Rgb(60, 60, 60),
                label: Color::Rgb(235, 206, 50),
            },
            Theme::Dracula => Self {
                // Official Dracula palette: https://draculatheme.com/contribute
                claude: Color::Rgb(255, 121, 198), // pink
                codex: Color::Rgb(80, 250, 123),   // green
                child: Color::Rgb(98, 114, 164),   // comment
                border: Color::Rgb(68, 71, 90),    // current line
                title: Color::Rgb(248, 248, 242),  // foreground
                header: Color::Rgb(241, 250, 140), // yellow
                selected_bg: Color::Rgb(68, 71, 90),
                label: Color::Rgb(189, 147, 249), // purple
            },
            Theme::Solarized => Self {
                // Ethan Schoonover's Solarized Dark: https://ethanschoonover.com/solarized/
                claude: Color::Rgb(203, 75, 22),  // orange
                codex: Color::Rgb(133, 153, 0),   // green
                child: Color::Rgb(131, 148, 150), // base0 (body text)
                border: Color::Rgb(88, 110, 117), // base01
                title: Color::Rgb(147, 161, 161), // base1 (emphasized)
                header: Color::Rgb(181, 137, 0),  // yellow
                selected_bg: Color::Rgb(7, 54, 66), // base02
                label: Color::Rgb(181, 137, 0),   // yellow
            },
        }
    }

    // --- Derived styles ----------------------------------------------------

    pub fn claude_style(&self) -> Style {
        Style::new().fg(self.claude)
    }

    pub fn codex_style(&self) -> Style {
        Style::new().fg(self.codex)
    }

    pub fn child_style(&self) -> Style {
        Style::new().fg(self.child)
    }

    pub fn border_style(&self) -> Style {
        Style::new().fg(self.border)
    }

    pub fn title_style(&self) -> Style {
        Style::new().fg(self.title).add_modifier(Modifier::BOLD)
    }

    pub fn header_style(&self) -> Style {
        Style::new().fg(self.header).add_modifier(Modifier::BOLD)
    }

    pub fn selected_style(&self) -> Style {
        Style::new().bg(self.selected_bg).add_modifier(Modifier::BOLD)
    }

    pub fn label_style(&self) -> Style {
        Style::new().fg(self.label)
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::from_theme(Theme::Default)
    }
}
