//! Colour themes.
//!
//! A [`Palette`] names foreground colours by what they are *for*; a [`Theme`]
//! is a palette resolved against the terminal's real background, which adds
//! the tinted surfaces (chips, keycaps, the selection band) so they sit right
//! on whatever background the terminal has. Views never hardcode a colour.
//!
//! The look: quiet chrome, loud content. The accent means "you are here / this
//! is a key / this is the main action"; green, amber and red mean state;
//! everything else is a step on one grey ramp (`strong` → `text` → `subtle` →
//! `dim` → `muted` → `faint`). Background tints only ever fill a single row.

use ratatui::style::Color;

/// What the terminal told us about its colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Terminal {
    pub light: bool,
    pub background: Option<Color>,
    /// It didn't answer at all, so further queries would only time out too.
    pub silent: bool,
}

impl Terminal {
    /// Asks the terminal for its background colour; assumes a dark one when it
    /// doesn't answer. Must run before the input reader starts, since the
    /// reply arrives on stdin.
    pub fn detect() -> Self {
        use terminal_colorsaurus::{QueryOptions, ThemeMode, color_palette};
        match color_palette(QueryOptions::default()) {
            Ok(palette) => {
                let (r, g, b) = palette.background.scale_to_8bit();
                Self {
                    light: palette.theme_mode() == ThemeMode::Light,
                    background: Some(Color::Rgb(r, g, b)),
                    silent: false,
                }
            }
            Err(e) => Self {
                silent: matches!(e, terminal_colorsaurus::Error::Timeout(_)),
                ..Self::default()
            },
        }
    }
}

/// Foreground roles of one theme. See the module docs for how each is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Registry name, as stored in config.
    pub name: &'static str,
    /// The background the palette is designed on, used when the terminal
    /// doesn't report its own.
    pub background: Color,

    pub accent: Color,
    /// Quiet secondary accent (the "new" tag).
    pub secondary: Color,
    pub strong: Color,
    pub text: Color,
    /// Group titles inside cards.
    pub title: Color,
    pub subtle: Color,
    pub dim: Color,
    pub muted: Color,
    pub faint: Color,
    /// Horizontal rules and the backdrop behind overlays.
    pub rule: Color,
    pub success: Color,
    pub warning: Color,
    /// The splash line's gentle yellow.
    pub warning_soft: Color,
    pub error: Color,
}

impl Palette {
    /// The built-in palettes, in picker order (after `auto`).
    pub const ALL: [Palette; 6] = [DARK, LIGHT, GRUVBOX, CATPPUCCIN, DRACULA, NORD];
}

/// A palette resolved against a background: everything a frame draws with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    pub accent: Color,
    pub secondary: Color,
    pub strong: Color,
    pub text: Color,
    pub title: Color,
    pub subtle: Color,
    pub dim: Color,
    pub muted: Color,
    pub faint: Color,
    pub rule: Color,
    pub success: Color,
    pub warning: Color,
    pub warning_soft: Color,
    pub error: Color,

    /// Keycaps, chips and the hover band: a whisper above the background.
    pub surface: Color,
    /// A hovered chip.
    pub surface_hi: Color,
    /// The focused selection band: the background tinted with the accent.
    pub selection: Color,
    /// A hovered primary button.
    pub accent_hi: Color,
    /// Toast backgrounds.
    pub success_bg: Color,
    pub warning_bg: Color,
    /// Text behind an open overlay: barely there.
    pub backdrop: Color,
}

impl Theme {
    /// Resolves a config theme name for this terminal. `auto` (and anything
    /// unknown) picks `light` or `dark` to match the background.
    pub fn resolve(name: &str, terminal: Terminal) -> Theme {
        let palette = Palette::ALL
            .into_iter()
            .find(|p| p.name == name)
            .unwrap_or(if terminal.light { LIGHT } else { DARK });
        Theme::new(&palette, terminal.background)
    }

    /// `palette` on `background` (or the background it was designed on).
    pub fn new(p: &Palette, background: Option<Color>) -> Theme {
        let bg = background.unwrap_or(p.background);
        let light = luminance(bg).is_some_and(|l| l > 0.4);
        // Tints need less strength on light backgrounds to read the same.
        let (surface, surface_hi, selection, tint) = if light {
            (0.06, 0.11, 0.13, 0.14)
        } else {
            (0.08, 0.14, 0.26, 0.18)
        };
        Theme {
            name: p.name,
            accent: p.accent,
            secondary: p.secondary,
            strong: p.strong,
            text: p.text,
            title: p.title,
            subtle: p.subtle,
            dim: p.dim,
            muted: p.muted,
            faint: p.faint,
            rule: p.rule,
            success: p.success,
            warning: p.warning,
            warning_soft: p.warning_soft,
            error: p.error,
            surface: mix(bg, p.text, surface),
            surface_hi: mix(bg, p.text, surface_hi),
            selection: mix(bg, p.accent, selection),
            accent_hi: mix(p.accent, p.strong, 0.2),
            success_bg: mix(bg, p.success, tint),
            warning_bg: mix(bg, p.warning, tint),
            backdrop: mix(bg, p.text, 0.15),
        }
    }
}

const fn hex(rgb: u32) -> Color {
    Color::Rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// `a` moved `t` of the way towards `b` (RGB colours; others pass through).
pub fn mix(a: Color, b: Color, t: f64) -> Color {
    let (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) = (a, b) else {
        return a;
    };
    let channel = |x: u8, y: u8| {
        let v = f64::from(x) + (f64::from(y) - f64::from(x)) * t;
        #[expect(clippy::cast_sign_loss, reason = "clamped to 0..=255")]
        let v = v.round().clamp(0.0, 255.0) as u8;
        v
    };
    Color::Rgb(channel(ar, br), channel(ag, bg), channel(ab, bb))
}

/// The default: one violet accent bright enough to read as text, and a grey
/// ramp that steps down evenly from content to hints to decoration.
pub const DARK: Palette = Palette {
    name: "dark",
    background: hex(0x18181B),
    accent: hex(0xA78BFA),
    secondary: hex(0xC4B5FD),
    strong: hex(0xFAFAFA),
    text: hex(0xE4E4E7),
    title: hex(0xE4E4E7),
    subtle: hex(0xA1A1AA),
    dim: hex(0x8E8E97),
    muted: hex(0x71717A),
    faint: hex(0x5E5E66),
    rule: hex(0x3F3F46),
    success: hex(0x10B981),
    warning: hex(0xFBBF24),
    warning_soft: hex(0xFCD34D),
    error: hex(0xEF4444),
};

/// Tuned for light terminals: dark text, deeper accent.
pub const LIGHT: Palette = Palette {
    name: "light",
    background: hex(0xFFFFFF),
    accent: hex(0x6D28D9),
    secondary: hex(0x7C3AED),
    strong: hex(0x18181B),
    text: hex(0x27272A),
    title: hex(0x3F3F46),
    subtle: hex(0x52525B),
    dim: hex(0x71717A),
    muted: hex(0x8A8A93),
    faint: hex(0xA1A1AA),
    rule: hex(0xD4D4D8),
    success: hex(0x047857),
    warning: hex(0xB45309),
    warning_soft: hex(0x78350F),
    error: hex(0xDC2626),
};

pub const GRUVBOX: Palette = Palette {
    name: "gruvbox",
    background: hex(0x282828),
    accent: hex(0xD3869B),
    secondary: hex(0x83A598),
    strong: hex(0xFBF1C7),
    text: hex(0xEBDBB2),
    title: hex(0xEBDBB2),
    subtle: hex(0xD5C4A1),
    dim: hex(0xBDAE93),
    muted: hex(0xA89984),
    faint: hex(0x928374),
    rule: hex(0x504945),
    success: hex(0x98971A),
    warning: hex(0xD79921),
    warning_soft: hex(0xFABD2F),
    error: hex(0xFB4934),
};

pub const CATPPUCCIN: Palette = Palette {
    name: "catppuccin",
    background: hex(0x1E1E2E),
    accent: hex(0xCBA6F7),
    secondary: hex(0x89B4FA),
    strong: hex(0xCDD6F4),
    text: hex(0xCDD6F4),
    title: hex(0xBAC2DE),
    subtle: hex(0xA6ADC8),
    dim: hex(0x9399B2),
    muted: hex(0x7F849C),
    faint: hex(0x585B70),
    rule: hex(0x45475A),
    success: hex(0xA6E3A1),
    warning: hex(0xF9E2AF),
    warning_soft: hex(0xF9E2AF),
    error: hex(0xF38BA8),
};

pub const DRACULA: Palette = Palette {
    name: "dracula",
    background: hex(0x282A36),
    accent: hex(0xBD93F9),
    secondary: hex(0xFF79C6),
    strong: hex(0xFFFFFF),
    text: hex(0xF8F8F2),
    title: hex(0xF8F8F2),
    subtle: hex(0xC8C8E0),
    dim: hex(0x8B8FB0),
    muted: hex(0x6272A4),
    faint: hex(0x6272A4),
    rule: hex(0x3A3D4D),
    success: hex(0x50FA7B),
    warning: hex(0xF1FA8C),
    warning_soft: hex(0xF1FA8C),
    error: hex(0xFF5555),
};

pub const NORD: Palette = Palette {
    name: "nord",
    background: hex(0x2E3440),
    accent: hex(0xC6A3BF),
    secondary: hex(0x88C0D0),
    strong: hex(0xECEFF4),
    text: hex(0xE5E9F0),
    title: hex(0xD8DEE9),
    subtle: hex(0xC0C8D4),
    dim: hex(0x9AA3B2),
    muted: hex(0x7B8494),
    faint: hex(0x616E88),
    rule: hex(0x434C5E),
    success: hex(0xA3BE8C),
    warning: hex(0xEBCB8B),
    warning_soft: hex(0xEBCB8B),
    error: hex(0xBF616A),
};

/// Black or white, whichever reads better on `bg` (WCAG contrast). For text
/// painted on an accent-filled single-row button.
pub fn on_color(bg: Color) -> Color {
    const BLACK: Color = Color::Rgb(0, 0, 0);
    const WHITE: Color = Color::Rgb(255, 255, 255);
    let Some(l) = luminance(bg) else {
        return WHITE;
    };
    if contrast(l, 0.0) >= contrast(l, 1.0) {
        BLACK
    } else {
        WHITE
    }
}

/// WCAG contrast ratio between two RGB colours (1.0–21.0).
#[cfg(test)]
pub fn contrast_ratio(a: Color, b: Color) -> Option<f64> {
    Some(contrast(luminance(a)?, luminance(b)?))
}

fn contrast(l1: f64, l2: f64) -> f64 {
    let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

fn luminance(color: Color) -> Option<f64> {
    let Color::Rgb(red, green, blue) = color else {
        return None;
    };
    let linear = |channel: u8| {
        let c = f64::from(channel) / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    Some(0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Text roles must be readable for what they carry and step down in order:
    /// content ≥ 7:1, secondary copy ≥ 4.5:1, hints ≥ 3:1, decoration ≥ 2:1.
    /// The accent marks selection and keys, so it must read as text too.
    #[test]
    fn text_roles_are_readable_and_ordered() {
        for p in Palette::ALL {
            let ratio = |c| contrast_ratio(c, p.background).unwrap_or_default();
            for (role, c, min) in [
                ("text", p.text, 7.0),
                ("strong", p.strong, 7.0),
                ("subtle", p.subtle, 4.5),
                ("dim", p.dim, 4.5),
                ("muted", p.muted, 3.0),
                ("faint", p.faint, 2.0),
                ("accent", p.accent, 4.5),
            ] {
                let r = ratio(c);
                assert!(r >= min, "{}: {role} is {r:.1}:1, want ≥ {min}", p.name);
            }
            let ramp = [p.subtle, p.dim, p.muted, p.faint];
            for pair in ramp.windows(2) {
                assert!(
                    ratio(pair[1]) <= ratio(pair[0]) + 0.01,
                    "{}: grey ramp inverts",
                    p.name
                );
            }
        }
    }

    /// Text stays readable on every tinted surface: on the palette's own
    /// background and on a plain black (or, for light, white) terminal.
    #[test]
    fn tinted_surfaces_keep_text_readable() {
        for p in Palette::ALL {
            let plain = if p.name == "light" {
                hex(0xFFFFFF)
            } else {
                hex(0x000000)
            };
            for bg in [None, Some(plain)] {
                let t = Theme::new(&p, bg);
                for (role, fg, surface, min) in [
                    ("strong on selection", t.strong, t.selection, 4.5),
                    ("text on surface", t.text, t.surface, 7.0),
                    ("dim on surface", t.dim, t.surface, 3.0),
                    ("success on its toast", t.success, t.success_bg, 3.0),
                ] {
                    let r = contrast_ratio(fg, surface).unwrap_or_default();
                    assert!(r >= min, "{} on {bg:?}: {role} is {r:.1}:1", p.name);
                }
            }
        }
    }

    #[test]
    fn auto_and_unknown_follow_the_terminal() {
        let light = Terminal {
            light: true,
            ..Terminal::default()
        };
        let dark = Terminal::default();
        assert_eq!(Theme::resolve("auto", light).name, "light");
        assert_eq!(Theme::resolve("auto", dark).name, "dark");
        assert_eq!(Theme::resolve("nope", dark).name, "dark");
        assert_eq!(Theme::resolve("nord", light).name, "nord");
    }

    #[test]
    fn surfaces_follow_the_real_background() {
        let on_black = Theme::new(&DARK, Some(hex(0x000000)));
        let on_grey = Theme::new(&DARK, Some(hex(0x303030)));
        assert_ne!(on_black.surface, on_grey.surface);
        assert_eq!(mix(hex(0x000000), hex(0xFFFFFF), 0.5), hex(0x808080));
    }

    #[test]
    fn on_color_picks_the_readable_side() {
        assert_eq!(on_color(hex(0xFFFFFF)), hex(0x000000));
        assert_eq!(on_color(hex(0x101010)), hex(0xFFFFFF));
        for p in Palette::ALL {
            let r = contrast_ratio(on_color(p.accent), p.accent).unwrap_or_default();
            assert!(r >= 4.5, "{}: text on accent is {r:.1}:1", p.name);
        }
    }
}
