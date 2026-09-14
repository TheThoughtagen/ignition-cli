//! Style tokens — the cockpit's single theming module (TUIX-03/TUIX-04
//! foundation). Every later theming change flows through THIS file:
//! screens never name a [`Color`], they name a semantic [`Palette`] slot
//! via the style helpers below. This is also the ONLY module in the repo
//! allowed to reference `Color::` — CI (12-03) enforces it.
//!
//! # Capability tiers, authored not derived
//!
//! Verified research fact (12-RESEARCH): ratatui's `CrosstermBackend`
//! passes colors through 1:1 with NO quantization — an `Rgb` value emits
//! truecolor SGR regardless of the terminal. Degradation is therefore a
//! property of the AUTHORING: each [`Theme`] carries four explicitly
//! written palettes ([`Tier::Truecolor`]/[`Tier::C256`]/[`Tier::C16`]/
//! [`Tier::Mono`]) and [`Theme::resolve`] just picks one. There is no
//! runtime RGB→256→16 conversion anywhere.
//!
//! Tier rules (enforced by tests below):
//! - `Rgb` appears ONLY in `truecolor` palettes.
//! - `c256` palettes use named colors + `Indexed`.
//! - `c16` palettes use only the 16 ANSI named colors (plus `Reset`,
//!   which `default` carries by design — see the theme).
//! - `mono` palettes are `Color::Reset` in EVERY slot — readability
//!   rides modifiers only, centralized in the style helpers.
//!
//! # Hues are adjustable post-UAT
//!
//! The exact color values of `dark`/`light` are planner-discretion and
//! EXPECTED to be tuned after UAT. Tests deliberately never pin literal
//! hues — they pin structural contracts only: palette-slot equality,
//! registry behavior, tier containment, and the all-Reset mono rule.
//!
//! # Mono adaptation is centralized here
//!
//! Because every mono palette is all-`Reset`, the style helpers detect
//! the mono tier by the slot's own value (a `Reset` error slot IS the
//! mono marker) and substitute modifier-based readability: `error` →
//! BOLD, `selection` → REVERSED. Everything else maps to plain `fg`,
//! where `Reset` fg is exactly default text — correct by design.

use ratatui::style::{Color, Modifier, Style};

/// Semantic style slots — the k9s/btop-convergent vocabulary, sized to
/// this cockpit's actual screens.
///
/// THIS STRUCT IS THE CONTRACT: screens never name a Color, they name a
/// slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Default body text (mono: Reset).
    pub text: Color,
    /// Hints, descriptions, DEBUG/TRACE (pairs with DIM at call sites).
    pub muted: Color,
    /// Active tab, section headers (pairs with BOLD at call sites).
    pub emphasis: Color,
    /// Pane borders.
    pub border: Color,
    /// Pane titles (k9s `frame.title` role).
    pub title: Color,
    /// Table headers (k9s `views.table.header` role).
    pub header: Color,
    /// Row highlight background (dashboard `bg(DarkGray)` today).
    pub selection_bg: Color,
    /// The `▸` selection marker.
    pub indicator: Color,
    /// ERROR/FATAL + panel error labels (k9s `status.errorColor` role).
    pub error: Color,
    /// WARN (k9s `status.modifyColor` role).
    pub warning: Color,
    /// Running/OK states (k9s `status.newColor` role; headroom).
    pub success: Color,
    /// Profile banner, status-line profile name.
    pub accent: Color,
}

/// Terminal color capability, most- to least-capable.
///
/// [`Tier::default`] is the strictest tier on purpose: unit tests and
/// pre-resolve [`AppState`] run the strictest tier — anything that
/// reads wrong at [`Tier::Mono`] is a bug, not a cosmetic nit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// 24-bit color (`COLORTERM=truecolor|24bit`). `Rgb` allowed.
    Truecolor,
    /// 256-color xterm convention (`TERM` contains `256color`).
    C256,
    /// Base 16-color terminal — ANSI named colors only.
    C16,
    /// No reliable color — `Color::Reset` only; structure via modifiers.
    Mono,
}

impl Default for Tier {
    /// Unit tests and pre-resolve AppState run the strictest tier.
    fn default() -> Self {
        Tier::Mono
    }
}

/// One named theme; every tier is AUTHORED, never derived — the
/// 16-color and mono palettes ARE the readability guarantee (TUIX-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    /// 24-bit palette — may use `Rgb` (tier-gated: safe here ONLY).
    pub truecolor: Palette,
    /// 256-color palette — named colors + `Indexed`.
    pub c256: Palette,
    /// 16-color palette — the 16 ANSI named colors ONLY (+ Reset where
    /// intentional).
    pub c16: Palette,
    /// Monochrome palette — `Color::Reset` ONLY.
    pub mono: Palette,
}

impl Theme {
    /// The palette authored for `tier`.
    pub fn resolve(&self, tier: Tier) -> Palette {
        match tier {
            Tier::Truecolor => self.truecolor,
            Tier::C256 => self.c256,
            Tier::C16 => self.c16,
            Tier::Mono => self.mono,
        }
    }

    /// Resolve a theme by its config name (`[ui].theme`).
    ///
    /// `None` for unknown names — the caller warns + falls back (lenient
    /// degradation, same contract as load_for_tui's new-schema surface).
    /// Names are case-sensitive ("DARK" is not "dark").
    pub fn by_name(name: &str) -> Option<&'static Theme> {
        THEMES.iter().find(|t| t.name == name)
    }
}

/// The built-in registry: `default`, `mono`, `dark`, `light`. New themes
/// are added here — `by_name` needs no other change.
pub const THEMES: [Theme; 4] = [DEFAULT, MONO, DARK, LIGHT];

/// `default` — formalizes the current look: near-monochrome + error
/// red / warn yellow. Truecolor/C256/C16 are IDENTICAL (all named ANSI
/// colors — zero visual regression, no `Rgb`); `selection_bg` keeps the
/// existing `DarkGray`. The `Reset` slots are intentional: they render
/// as the terminal's own default colors, exactly today's behavior.
const DEFAULT: Theme = Theme {
    name: "default",
    truecolor: Palette {
        text: Color::Reset,
        muted: Color::Reset,
        emphasis: Color::Reset,
        border: Color::Reset,
        title: Color::Reset,
        header: Color::Reset,
        selection_bg: Color::DarkGray,
        indicator: Color::Reset,
        error: Color::Red,
        warning: Color::Yellow,
        success: Color::Reset,
        accent: Color::Reset,
    },
    c256: Palette {
        text: Color::Reset,
        muted: Color::Reset,
        emphasis: Color::Reset,
        border: Color::Reset,
        title: Color::Reset,
        header: Color::Reset,
        selection_bg: Color::DarkGray,
        indicator: Color::Reset,
        error: Color::Red,
        warning: Color::Yellow,
        success: Color::Reset,
        accent: Color::Reset,
    },
    c16: Palette {
        text: Color::Reset,
        muted: Color::Reset,
        emphasis: Color::Reset,
        border: Color::Reset,
        title: Color::Reset,
        header: Color::Reset,
        selection_bg: Color::DarkGray,
        indicator: Color::Reset,
        error: Color::Red,
        warning: Color::Yellow,
        success: Color::Reset,
        accent: Color::Reset,
    },
    mono: all_reset(),
};

/// `mono` — `Color::Reset` in EVERY slot at EVERY tier (yes, including
/// the truecolor palette). This makes the mono theme tier-independent:
/// which terminal it lands on cannot change what it looks like. 12-02's
/// wiring tests pin this against ambient `COLORTERM`.
const MONO: Theme = Theme {
    name: "mono",
    truecolor: all_reset(),
    c256: all_reset(),
    c16: all_reset(),
    mono: all_reset(),
};

/// `dark` — the full color cockpit for dark-background terminals. The
/// showcase palette; hues are conservative ANSI-family at the c16 tier
/// so the 16-color fallback stays readable, and ARE ADJUSTABLE POST-UAT
/// (tests pin structure, not hues).
const DARK: Theme = Theme {
    name: "dark",
    truecolor: Palette {
        text: Color::Rgb(220, 223, 228),
        muted: Color::Rgb(127, 140, 141),
        emphasis: Color::Rgb(244, 246, 248),
        border: Color::Rgb(90, 98, 110),
        title: Color::Rgb(244, 246, 248),
        header: Color::Rgb(244, 246, 248),
        selection_bg: Color::Rgb(60, 64, 72),
        indicator: Color::Rgb(52, 152, 219),
        error: Color::Rgb(231, 76, 60),
        warning: Color::Rgb(241, 196, 15),
        success: Color::Rgb(46, 204, 113),
        accent: Color::Rgb(52, 152, 219),
    },
    c256: Palette {
        text: Color::Indexed(253),
        muted: Color::Indexed(244),
        emphasis: Color::Indexed(255),
        border: Color::Indexed(60),
        title: Color::Indexed(255),
        header: Color::Indexed(255),
        selection_bg: Color::Indexed(238),
        indicator: Color::Indexed(75),
        error: Color::Indexed(203),
        warning: Color::Indexed(220),
        success: Color::Indexed(41),
        accent: Color::Indexed(75),
    },
    c16: Palette {
        text: Color::White,
        muted: Color::DarkGray,
        emphasis: Color::White,
        border: Color::Gray,
        title: Color::White,
        header: Color::White,
        selection_bg: Color::DarkGray,
        indicator: Color::White,
        error: Color::Red,
        warning: Color::Yellow,
        success: Color::Green,
        accent: Color::Blue,
    },
    mono: all_reset(),
};

/// `light` — for white-background terminals; NO `DarkGray`-background
/// assumptions (Pitfall 4). Warning rides ratatui's `Yellow` (SGR brown,
/// SGR 33 — readable on white; never the `Light*` variants per RESEARCH
/// Pitfall 2). The truecolor tier darkens warning/accent with `Rgb` for
/// contrast on white; c256 mirrors via `Indexed`.
const LIGHT: Theme = Theme {
    name: "light",
    truecolor: Palette {
        text: Color::Black,
        muted: Color::DarkGray,
        emphasis: Color::Black,
        border: Color::Black,
        title: Color::Black,
        header: Color::Black,
        selection_bg: Color::Gray,
        indicator: Color::Blue,
        error: Color::Red,
        warning: Color::Rgb(176, 128, 0),
        success: Color::Green,
        accent: Color::Rgb(0, 90, 170),
    },
    c256: Palette {
        text: Color::Black,
        muted: Color::DarkGray,
        emphasis: Color::Black,
        border: Color::Black,
        title: Color::Black,
        header: Color::Black,
        selection_bg: Color::Gray,
        indicator: Color::Blue,
        error: Color::Red,
        warning: Color::Indexed(136),
        success: Color::Green,
        accent: Color::Indexed(40),
    },
    c16: Palette {
        text: Color::Black,
        muted: Color::DarkGray,
        emphasis: Color::Black,
        border: Color::Black,
        title: Color::Black,
        header: Color::Black,
        selection_bg: Color::Gray,
        indicator: Color::Blue,
        error: Color::Red,
        warning: Color::Yellow,
        success: Color::Green,
        accent: Color::Blue,
    },
    mono: all_reset(),
};

/// The all-`Reset` palette — the mono contract, shared by every theme's
/// mono tier and by the mono theme's EVERY tier.
const fn all_reset() -> Palette {
    Palette {
        text: Color::Reset,
        muted: Color::Reset,
        emphasis: Color::Reset,
        border: Color::Reset,
        title: Color::Reset,
        header: Color::Reset,
        selection_bg: Color::Reset,
        indicator: Color::Reset,
        error: Color::Reset,
        warning: Color::Reset,
        success: Color::Reset,
        accent: Color::Reset,
    }
}

/// Error style. Color tiers: fg = the theme's error slot. Mono tier
/// (error slot == Reset — the mono marker): BOLD instead — errors shout
/// by weight, never by hue (RESEARCH Pitfall 5; the k9s/btop mono move).
pub fn error(p: &Palette) -> Style {
    if p.error == Color::Reset {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.error)
    }
}

/// Selection/row-highlight. Color tiers: bg = `selection_bg`. Mono tier
/// (selection_bg == Reset): `Modifier::REVERSED` — a Reset bg would be
/// invisible (RESEARCH Pitfall 5).
pub fn selection(p: &Palette) -> Style {
    if p.selection_bg == Color::Reset {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default().bg(p.selection_bg)
    }
}

/// Body text. Mono: fg Reset = plain default text, correct by design.
pub fn text(p: &Palette) -> Style {
    Style::default().fg(p.text)
}

/// Hints/descriptions — pair with inline DIM for hierarchy (modifiers
/// stay at call sites; DIM is not-a-color doctrine).
pub fn muted(p: &Palette) -> Style {
    Style::default().fg(p.muted)
}

/// Active tab / section headers — pair with inline BOLD.
pub fn emphasis(p: &Palette) -> Style {
    Style::default().fg(p.emphasis)
}

/// Pane borders.
pub fn border(p: &Palette) -> Style {
    Style::default().fg(p.border)
}

/// Pane titles.
pub fn title(p: &Palette) -> Style {
    Style::default().fg(p.title)
}

/// Table headers.
pub fn header(p: &Palette) -> Style {
    Style::default().fg(p.header)
}

/// The `▸` selection marker.
pub fn indicator(p: &Palette) -> Style {
    Style::default().fg(p.indicator)
}

/// WARN — plain. Advisory by design: DEBUG/TRACE keep their inline DIM
/// for hierarchy, so no mono substitution here (mono: plain default
/// text; weight hierarchy stays inline).
pub fn warning(p: &Palette) -> Style {
    Style::default().fg(p.warning)
}

/// Running/OK states.
pub fn success(p: &Palette) -> Style {
    Style::default().fg(p.success)
}

/// Profile banner / status-line profile name.
pub fn accent(p: &Palette) -> Style {
    Style::default().fg(p.accent)
}

/// Map terminal-capability env conventions to a [`Tier`]. Pure over an
/// injected env snapshot — tests never touch the real environment
/// (edition 2024 `set_var` is unsafe; the crate's `ENV_LOCK` exists but
/// injection is cleaner). Priority order matters:
///
/// 1. `NO_COLOR` set non-empty -> [`Tier::Mono`] (no-color.org convention)
/// 2. `COLORTERM` `truecolor`|`24bit` -> [`Tier::Truecolor`] (de-facto convention)
/// 3. `TERM` contains `"256color"` -> [`Tier::C256`]
/// 4. `TERM` missing or `"dumb"` -> [`Tier::Mono`]
/// 5. otherwise -> [`Tier::C16`]
///
/// 12-02's `build_context` will call it as
/// `detect_tier(&|k| std::env::var(k).ok())` — the signature is exactly
/// `&dyn Fn(&str) -> Option<String>` so that closure coerces.
pub fn detect_tier(env: &dyn Fn(&str) -> Option<String>) -> Tier {
    if env("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return Tier::Mono;
    }
    if let Some("truecolor" | "24bit") = env("COLORTERM").as_deref() {
        return Tier::Truecolor;
    }
    match env("TERM").as_deref() {
        None | Some("dumb") => Tier::Mono,
        Some(term) if term.contains("256color") => Tier::C256,
        Some(_) => Tier::C16,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All 12 slots in declaration order — the structural tests walk
    /// every theme's every tier through this.
    fn slots(p: &Palette) -> [Color; 12] {
        [
            p.text,
            p.muted,
            p.emphasis,
            p.border,
            p.title,
            p.header,
            p.selection_bg,
            p.indicator,
            p.error,
            p.warning,
            p.success,
            p.accent,
        ]
    }

    const ALL_TIERS: [Tier; 4] = [Tier::Truecolor, Tier::C256, Tier::C16, Tier::Mono];

    #[test]
    fn registry_resolves_all_four_names() {
        for name in ["default", "mono", "dark", "light"] {
            let theme =
                Theme::by_name(name).unwrap_or_else(|| panic!("registry missing theme {name:?}"));
            assert_eq!(theme.name, name);
        }
    }

    #[test]
    fn registry_rejects_unknown_names_case_sensitive() {
        assert_eq!(Theme::by_name("banana"), None);
        assert_eq!(Theme::by_name(""), None);
        // case-sensitive: "DARK" is not "dark"
        assert_eq!(Theme::by_name("DARK"), None);
    }

    #[test]
    fn tier_resolution_returns_that_tiers_palette() {
        for theme in &THEMES {
            for tier in ALL_TIERS {
                assert_eq!(theme.resolve(tier), theme_by_slot(theme, tier));
            }
        }
    }

    /// Resolved slot lookup used only to make the equality above
    /// meaningful (resolve must return THE tier palette, not any).
    fn theme_by_slot(theme: &Theme, tier: Tier) -> Palette {
        match tier {
            Tier::Truecolor => theme.truecolor,
            Tier::C256 => theme.c256,
            Tier::C16 => theme.c16,
            Tier::Mono => theme.mono,
        }
    }

    #[test]
    fn mono_contract_every_slot_of_every_theme_is_reset() {
        for theme in &THEMES {
            for slot in slots(&theme.mono) {
                assert_eq!(
                    slot,
                    Color::Reset,
                    "theme {:?} carries color in its mono palette — mono tier \
                     readability rides modifiers only",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn mono_theme_is_tier_independent_all_reset_at_every_tier() {
        let mono = Theme::by_name("mono").expect("mono in registry");
        for tier in ALL_TIERS {
            for slot in slots(&mono.resolve(tier)) {
                assert_eq!(
                    slot,
                    Color::Reset,
                    "mono theme carries color at tier {tier:?} — it must look \
                     identical on every terminal (12-02 wiring pins this)"
                );
            }
            assert_eq!(mono.resolve(tier), all_reset());
        }
    }

    #[test]
    fn rgb_confined_to_truecolor_palettes() {
        for theme in &THEMES {
            for tier in [Tier::C256, Tier::C16, Tier::Mono] {
                for slot in slots(&theme.resolve(tier)) {
                    assert!(
                        !matches!(slot, Color::Rgb(..)),
                        "theme {:?} has an Rgb value in its {tier:?} palette — \
                         Rgb emits truecolor SGR on any terminal (no backend \
                         quantization); it may appear ONLY in truecolor",
                        theme.name
                    );
                }
            }
        }
        // sanity: the dark theme's truecolor tier really does carry Rgb
        let dark = Theme::by_name("dark").expect("dark in registry");
        assert!(
            slots(&dark.truecolor)
                .iter()
                .any(|c| matches!(c, Color::Rgb(..)))
        );
    }

    #[test]
    fn c16_palettes_use_only_ansi_16_names() {
        // `Reset` is in the allowed set INTENTIONALLY: the `default`
        // theme's c16 palette carries Reset slots by design (they render
        // as the terminal default — that IS the zero-regression look).
        // Everything else must be one of the 16 ANSI named variants —
        // no Rgb, no Indexed.
        for theme in &THEMES {
            for slot in slots(&theme.c16) {
                assert!(
                    matches!(
                        slot,
                        Color::Reset
                            | Color::Black
                            | Color::Red
                            | Color::Green
                            | Color::Yellow
                            | Color::Blue
                            | Color::Magenta
                            | Color::Cyan
                            | Color::Gray
                            | Color::DarkGray
                            | Color::LightRed
                            | Color::LightGreen
                            | Color::LightYellow
                            | Color::LightBlue
                            | Color::LightMagenta
                            | Color::LightCyan
                            | Color::White
                    ),
                    "theme {:?} has a non-ANSI-16 value ({slot:?}) in its c16 \
                     palette — the 16-color tier degrades to artifacts",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn error_helper_shouts_by_weight_on_mono_by_hue_on_color_tiers() {
        let mono = all_reset();
        assert_eq!(
            error(&mono),
            Style::default().add_modifier(Modifier::BOLD),
            "mono error = BOLD, never a hue"
        );
        let colored = Palette {
            error: Color::Red,
            ..all_reset()
        };
        assert_eq!(error(&colored), Style::default().fg(Color::Red));
    }

    #[test]
    fn selection_helper_reverses_on_mono_fills_on_color_tiers() {
        let mono = all_reset();
        assert_eq!(
            selection(&mono),
            Style::default().add_modifier(Modifier::REVERSED),
            "mono selection = REVERSED (a Reset bg would be invisible)"
        );
        let colored = Palette {
            selection_bg: Color::DarkGray,
            ..all_reset()
        };
        assert_eq!(selection(&colored), Style::default().bg(Color::DarkGray));
    }

    #[test]
    fn tier_default_is_mono_strictest_tier() {
        assert_eq!(Tier::default(), Tier::Mono);
    }

    /// Env snapshot for detect_tier — a plain `HashMap` behind a
    /// closure. NO real-environment access anywhere (no ENV_LOCK
    /// needed): the injected snapshot makes every rule branch
    /// deterministic.
    fn detect(vars: &[(&str, &str)]) -> Tier {
        let map: std::collections::HashMap<&str, String> =
            vars.iter().map(|(k, v)| (*k, (*v).to_string())).collect();
        detect_tier(&|key| map.get(key).cloned())
    }

    #[test]
    fn no_color_wins_over_everything() {
        assert_eq!(
            detect(&[
                ("NO_COLOR", "1"),
                ("COLORTERM", "truecolor"),
                ("TERM", "xterm-256color"),
            ]),
            Tier::Mono,
            "NO_COLOR set non-empty forces Mono even with truecolor + 256color signals"
        );
    }

    #[test]
    fn empty_no_color_falls_through_the_rest_of_the_matrix() {
        // Empty NO_COLOR means unset per the convention — the tier must
        // come from the TERM/COLORTERM rules, not a Mono short-circuit.
        assert_eq!(detect(&[("NO_COLOR", ""), ("TERM", "xterm")]), Tier::C16);
        assert_eq!(
            detect(&[("NO_COLOR", ""), ("COLORTERM", "truecolor")]),
            Tier::Truecolor
        );
        assert_eq!(
            detect(&[("NO_COLOR", ""), ("TERM", "xterm-256color")]),
            Tier::C256
        );
    }

    #[test]
    fn colorterm_truecolor_and_24bit_map_to_truecolor() {
        assert_eq!(detect(&[("COLORTERM", "truecolor")]), Tier::Truecolor);
        assert_eq!(detect(&[("COLORTERM", "24bit")]), Tier::Truecolor);
        // COLORTERM wins before TERM is even consulted
        assert_eq!(
            detect(&[("COLORTERM", "truecolor"), ("TERM", "xterm")]),
            Tier::Truecolor
        );
    }

    #[test]
    fn term_256color_maps_to_c256() {
        assert_eq!(detect(&[("TERM", "xterm-256color")]), Tier::C256);
        assert_eq!(detect(&[("TERM", "screen-256color")]), Tier::C256);
    }

    #[test]
    fn term_absent_or_dumb_maps_to_mono() {
        assert_eq!(detect(&[]), Tier::Mono, "TERM absent = no reliable color");
        assert_eq!(detect(&[("TERM", "dumb")]), Tier::Mono);
    }

    #[test]
    fn plain_term_maps_to_c16() {
        assert_eq!(detect(&[("TERM", "xterm")]), Tier::C16);
    }
}
