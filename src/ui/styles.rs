use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

/// Selectable color themes.
///
/// Changing the theme regenerates a [`Palette`] which is then threaded through
/// every renderer. Semantic colors (e.g. red-for-warning in the kill popup,
/// green/yellow/red thresholds in the status bar) are NOT themed — only the
/// neutral UI chrome and the Claude/Codex brand accents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Default,
    Dracula,
    SolarizedDark,
    SolarizedLight,
    GruvboxDark,
    GruvboxLight,
}

impl Theme {
    /// All themes in user-facing display order.
    pub const ALL: [Theme; 6] = [
        Theme::Default,
        Theme::Dracula,
        Theme::SolarizedDark,
        Theme::SolarizedLight,
        Theme::GruvboxDark,
        Theme::GruvboxLight,
    ];

    /// Short label shown in the config popup.
    pub fn label(self) -> &'static str {
        match self {
            Theme::Default => "Default",
            Theme::Dracula => "Dracula",
            Theme::SolarizedDark => "Solarized Dark",
            Theme::SolarizedLight => "Solarized Light",
            Theme::GruvboxDark => "Gruvbox Dark",
            Theme::GruvboxLight => "Gruvbox Light",
        }
    }
}

/// Style of the time-series charts in the detail view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
    /// Full-terminal background color, rendered as a coverage block before
    /// any widget so the entire screen adopts the theme's base tone.
    pub background: Color,
    /// Primary text foreground for body text and general content.
    pub foreground: Color,
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
    /// Dim text for descriptions and non-interactive hints.
    pub dim: Color,
}

impl Palette {
    /// Build a [`Palette`] for the given [`Theme`].
    pub fn from_theme(theme: Theme) -> Self {
        match theme {
            Theme::Default => Self {
                // Original agentop palette — warm orange Claude, bright green Codex.
                background: Color::Rgb(20, 20, 35),
                foreground: Color::Rgb(220, 220, 220),
                claude: Color::Rgb(204, 120, 50),
                codex: Color::Rgb(100, 200, 100),
                child: Color::Rgb(160, 160, 160),
                border: Color::Rgb(90, 90, 90),
                title: Color::Rgb(240, 240, 240),
                header: Color::Rgb(235, 206, 50),
                selected_bg: Color::Rgb(60, 60, 60),
                label: Color::Rgb(235, 206, 50),
                dim: Color::Rgb(100, 100, 100),
            },
            Theme::Dracula => Self {
                // Official Dracula palette: https://draculatheme.com/contribute
                background: Color::Rgb(40, 42, 54),   // background
                foreground: Color::Rgb(248, 248, 242), // foreground
                claude: Color::Rgb(255, 121, 198),     // pink
                codex: Color::Rgb(80, 250, 123),       // green
                child: Color::Rgb(98, 114, 164),       // comment
                border: Color::Rgb(68, 71, 90),        // current line
                title: Color::Rgb(248, 248, 242),      // foreground
                header: Color::Rgb(241, 250, 140),     // yellow
                selected_bg: Color::Rgb(68, 71, 90),
                label: Color::Rgb(189, 147, 249),      // purple
                dim: Color::Rgb(98, 114, 164),         // comment
            },
            Theme::SolarizedDark => Self {
                // Ethan Schoonover's Solarized Dark
                background: Color::Rgb(0, 43, 54),    // base03
                foreground: Color::Rgb(131, 148, 150), // base0
                claude: Color::Rgb(203, 75, 22),       // orange
                codex: Color::Rgb(133, 153, 0),        // green
                child: Color::Rgb(131, 148, 150),      // base0
                border: Color::Rgb(88, 110, 117),      // base01
                title: Color::Rgb(147, 161, 161),      // base1
                header: Color::Rgb(181, 137, 0),       // yellow
                selected_bg: Color::Rgb(7, 54, 66),    // base02
                label: Color::Rgb(181, 137, 0),        // yellow
                dim: Color::Rgb(88, 110, 117),         // base01
            },
            Theme::SolarizedLight => Self {
                // Ethan Schoonover's Solarized Light
                background: Color::Rgb(253, 246, 227), // base3
                foreground: Color::Rgb(101, 123, 131), // base00
                claude: Color::Rgb(203, 75, 22),       // orange
                codex: Color::Rgb(133, 153, 0),        // green
                child: Color::Rgb(101, 123, 131),      // base00
                border: Color::Rgb(147, 161, 161),     // base1
                title: Color::Rgb(88, 110, 117),       // base01
                header: Color::Rgb(181, 137, 0),       // yellow
                selected_bg: Color::Rgb(238, 232, 213), // base2
                label: Color::Rgb(38, 139, 210),       // blue
                dim: Color::Rgb(147, 161, 161),        // base1
            },
            Theme::GruvboxDark => Self {
                // Gruvbox Dark: https://github.com/morhetz/gruvbox
                background: Color::Rgb(40, 40, 40),    // bg
                foreground: Color::Rgb(235, 219, 178), // fg
                claude: Color::Rgb(254, 128, 25),      // orange
                codex: Color::Rgb(184, 187, 38),       // green
                child: Color::Rgb(168, 153, 132),      // fg4
                border: Color::Rgb(80, 73, 69),        // bg2
                title: Color::Rgb(235, 219, 178),      // fg
                header: Color::Rgb(250, 189, 47),      // yellow
                selected_bg: Color::Rgb(80, 73, 69),   // bg2
                label: Color::Rgb(250, 189, 47),       // yellow
                dim: Color::Rgb(146, 131, 116),        // fg3
            },
            Theme::GruvboxLight => Self {
                // Gruvbox Light
                background: Color::Rgb(251, 241, 199), // light bg
                foreground: Color::Rgb(60, 56, 54),    // dark fg
                claude: Color::Rgb(175, 58, 3),        // dark orange
                codex: Color::Rgb(121, 116, 14),       // dark green
                child: Color::Rgb(102, 92, 84),        // fg4
                border: Color::Rgb(213, 196, 161),     // bg2
                title: Color::Rgb(60, 56, 54),         // dark fg
                header: Color::Rgb(181, 118, 20),      // dark yellow
                selected_bg: Color::Rgb(213, 196, 161), // bg2
                label: Color::Rgb(7, 102, 120),        // dark aqua
                dim: Color::Rgb(146, 131, 116),        // fg3
            },
        }
    }

    // --- Derived styles ----------------------------------------------------

    /// Base style with background and foreground applied. Use this to set up
    /// the full-coverage background block on every frame.
    pub fn base_style(&self) -> Style {
        Style::new().bg(self.background).fg(self.foreground)
    }

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

    pub fn dim_style(&self) -> Style {
        Style::new().fg(self.dim)
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::from_theme(Theme::Default)
    }
}
