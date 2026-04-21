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

/// Tier for idle-duration color escalation.
///
/// Used by the tree-view renderer to pick the right style when a root process
/// has been idle for an extended period. The thresholds follow the senior UI
/// designer's spec:
///
/// | Tier    | Duration   |
/// |---------|------------|
/// | Fresh   | 0 – 60 s   |
/// | Warning | 60 s – 5 m |
/// | Stale   | > 5 m      |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleTier {
    /// Process has been idle for less than 60 seconds.
    Fresh,
    /// Process has been idle for 60 seconds to 5 minutes.
    Warning,
    /// Process has been idle for more than 5 minutes.
    Stale,
}

/// Classify how long a process has been idle into a display tier.
///
/// This is a pure function suitable for unit testing independent of the
/// renderer. It encodes the spec thresholds in one place.
///
/// # Arguments
///
/// * `elapsed` - How long the process has been continuously idle.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use agentop::ui::styles::classify_idle;
/// use agentop::ui::styles::IdleTier;
///
/// assert_eq!(classify_idle(Duration::from_secs(30)), IdleTier::Fresh);
/// assert_eq!(classify_idle(Duration::from_secs(90)), IdleTier::Warning);
/// assert_eq!(classify_idle(Duration::from_secs(400)), IdleTier::Stale);
/// ```
pub fn classify_idle(elapsed: std::time::Duration) -> IdleTier {
    const WARNING_SECS: u64 = 60;
    const STALE_SECS: u64 = 5 * 60;
    let secs = elapsed.as_secs();
    if secs < WARNING_SECS {
        IdleTier::Fresh
    } else if secs < STALE_SECS {
        IdleTier::Warning
    } else {
        IdleTier::Stale
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
    /// `true` for light-background themes; drives activity badge color choices.
    pub is_light: bool,
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
                is_light: false,
            },
            Theme::Dracula => Self {
                // Official Dracula palette: https://draculatheme.com/contribute
                background: Color::Rgb(40, 42, 54), // background
                foreground: Color::Rgb(248, 248, 242), // foreground
                claude: Color::Rgb(255, 121, 198),  // pink
                codex: Color::Rgb(80, 250, 123),    // green
                child: Color::Rgb(98, 114, 164),    // comment
                border: Color::Rgb(68, 71, 90),     // current line
                title: Color::Rgb(248, 248, 242),   // foreground
                header: Color::Rgb(241, 250, 140),  // yellow
                selected_bg: Color::Rgb(68, 71, 90),
                label: Color::Rgb(189, 147, 249), // purple
                dim: Color::Rgb(98, 114, 164),    // comment
                is_light: false,
            },
            Theme::SolarizedDark => Self {
                // Ethan Schoonover's Solarized Dark
                background: Color::Rgb(0, 43, 54),     // base03
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
                is_light: false,
            },
            Theme::SolarizedLight => Self {
                // Ethan Schoonover's Solarized Light
                background: Color::Rgb(253, 246, 227),  // base3
                foreground: Color::Rgb(101, 123, 131),  // base00
                claude: Color::Rgb(203, 75, 22),        // orange
                codex: Color::Rgb(133, 153, 0),         // green
                child: Color::Rgb(101, 123, 131),       // base00
                border: Color::Rgb(147, 161, 161),      // base1
                title: Color::Rgb(88, 110, 117),        // base01
                header: Color::Rgb(181, 137, 0),        // yellow
                selected_bg: Color::Rgb(238, 232, 213), // base2
                label: Color::Rgb(38, 139, 210),        // blue
                dim: Color::Rgb(147, 161, 161),         // base1
                is_light: true,
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
                is_light: false,
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
                is_light: true,
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
        Style::new()
            .bg(self.selected_bg)
            .add_modifier(Modifier::BOLD)
    }

    pub fn label_style(&self) -> Style {
        Style::new().fg(self.label)
    }

    pub fn dim_style(&self) -> Style {
        Style::new().fg(self.dim)
    }

    // --- Activity-state semantic roles ------------------------------------
    // These styles implement the senior UI designer's spec. Colors are
    // contained here so dark/light branching stays in one place.

    /// Style for an actively-working root agent process.
    pub fn activity_active_style(&self) -> Style {
        if self.is_light {
            // Dark green for readability on light backgrounds.
            Style::new().fg(Color::Rgb(0, 100, 0))
        } else {
            Style::new().fg(Color::Green)
        }
    }

    /// Style for a root process that has been idle for < 60 s (Fresh tier).
    pub fn activity_idle_fresh_style(&self) -> Style {
        if self.is_light {
            Style::new().fg(Color::DarkGray)
        } else {
            Style::new().fg(Color::Gray)
        }
    }

    /// Style for a root process that has been idle for 60 s – 5 m (Warning tier).
    pub fn activity_idle_warning_style(&self) -> Style {
        if self.is_light {
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::Yellow)
        }
    }

    /// Style for a root process that has been idle for > 5 m (Stale tier).
    pub fn activity_idle_stale_style(&self) -> Style {
        Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)
    }

    /// Style for a root process whose activity state is `Unknown`.
    pub fn activity_unknown_style(&self) -> Style {
        if self.is_light {
            Style::new().fg(Color::Gray)
        } else {
            Style::new().fg(Color::DarkGray)
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::from_theme(Theme::Default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn classify_idle_fresh_below_60s() {
        assert_eq!(classify_idle(Duration::from_secs(0)), IdleTier::Fresh);
        assert_eq!(classify_idle(Duration::from_secs(30)), IdleTier::Fresh);
        assert_eq!(classify_idle(Duration::from_secs(59)), IdleTier::Fresh);
    }

    #[test]
    fn classify_idle_warning_at_60s() {
        assert_eq!(classify_idle(Duration::from_secs(60)), IdleTier::Warning);
        assert_eq!(classify_idle(Duration::from_secs(90)), IdleTier::Warning);
        assert_eq!(classify_idle(Duration::from_secs(299)), IdleTier::Warning);
    }

    #[test]
    fn classify_idle_stale_at_5_minutes() {
        assert_eq!(classify_idle(Duration::from_secs(300)), IdleTier::Stale);
        assert_eq!(classify_idle(Duration::from_secs(600)), IdleTier::Stale);
        assert_eq!(classify_idle(Duration::from_secs(3600)), IdleTier::Stale);
    }

    #[test]
    fn classify_idle_boundary_exactly_warning_start() {
        // 60 s is the first Warning second.
        assert_eq!(classify_idle(Duration::from_secs(60)), IdleTier::Warning);
    }

    #[test]
    fn classify_idle_boundary_exactly_stale_start() {
        // 300 s (5 min) is the first Stale second.
        assert_eq!(classify_idle(Duration::from_secs(300)), IdleTier::Stale);
    }
}
