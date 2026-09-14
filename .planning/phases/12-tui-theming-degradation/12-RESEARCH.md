# Phase 12: TUI Theming & Degradation - Research

**Researched:** 2026-09-14
**Domain:** ratatui 0.30 theming (style tokens, named palettes, terminal color-capability degradation), config theme selection, CI tokenization enforcement
**Confidence:** HIGH

## Summary

Phase 12 turns the config-plumbed `[ui].theme` key (Phase 8, currently carried but never rendered) into a working theme system, and formalizes the TUI's styling into a single token module with CI enforcement. The codebase inventory (verified this session) shows the job is smaller than "restyle everything": only **10 `Color::` references exist project-wide** — dashboard.rs (4× `fg(Color::Red)` error labels, 1× `bg(Color::DarkGray)` table selection) and logs.rs (`level_style` returns `Color::Red`/`Color::Yellow`; 3 test-side `Color` comparisons). The remaining ~30 styling sites are capability-safe `Modifier::BOLD`/`Modifier::DIM` compositions across all 8 screen modules. Phase 12 is therefore: one token module, a small named-theme set with per-capability-tier palettes, tier detection at startup, migration of the 10 literals + test assertions into token comparisons, and a CI grep.

The degradation strategy is settled by verified API facts, not opinion: **ratatui's CrosstermBackend performs NO automatic color reduction** — the 0.30 color mapping is a 1:1 variant passthrough (`Rgb(r,g,b) → CrosstermColor::Rgb`), and the official `Color` docs explicitly warn that truecolor in unsupported terminals yields "unpredictable visual artifacts." So themes must be **authored per capability tier** (explicit truecolor / 256 / 16 / mono palettes), not quantized at runtime. Hand-rolling an RGB→256→16 quantizer is the classic over-engineering trap; bottom (a flagship ratatui app) ships hand-rolled built-in themes with light variants and a plain `styles.theme = "gruvbox"` config key — exactly the shape this project needs, converged with k9s's skin-slot vocabulary (`body`/`frame.border`/`frame.status.errorColor`/`views.table.cursorColor` …).

Theme selection follows the already-locked lenient-degradation pattern: `theme = "dark"` loads as `Option<String>`; an **unknown name warns and falls back to the default theme** at TUI resolution time (`context::resolve`, where `load_for_tui` already degrades new-schema-surface failures). The resolved palette lands on `AppState` so the ~100 existing `AppState::new()` test callsites and every pure render fn keep their signatures.

**Primary recommendation:** Build `crates/ignition-tui/src/ui/theme.rs` as the single style-token module — a ~14-slot semantic `Palette` of ratatui `Color`s, four named themes (`default`, `mono`, `dark`, `light`) each authored at four tiers (truecolor/256/16/mono, all explicit — no quantization), env-based tier detection as a pure testable function, palettes resolved once onto `AppState`, all 10 `Color::` literals migrated to tokens, and a CI grep step in the `check` job forbidding `Color::` outside `ui/theme.rs`.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ratatui | 0.30.2 (workspace floor-pin) | Style/Color/Modifier types, TestBackend buffer assertions | Already the TUI stack; floor-pinned due to the 0.30.1 MSRV patch-drift (workspace comment) |
| crossterm | 0.29 (`event-stream`) | Backend; color emission; capability conventions (env) | Already pinned; single-major graph with ratatui-crossterm 0.29 |
| ratatui TestBackend | (in ratatui) | Deterministic style assertions on rendered buffer cells | Already the repo's test harness pattern (`buffer[(x,y)].fg`, `.modifier`) |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| (none new) | — | Everything needed is in-tree | Theme module needs no deps — `Palette` is a struct of `ratatui::style::Color` variants |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled token module | ratatui theme crates | **Already rejected by v1.1 stack research (roadmap-flagged decision)** — the crates are young/low-adoption and the slot vocabulary is app-specific; k9s/btop/bottom all hand-roll |
| Runtime RGB→256→16 quantization | Explicit per-tier palettes | Quantization is a hard problem (perceptual distance, contrast loss, unreadable results); authored tiers are deterministic, testable, and readability-guaranteed. Docs confirm backends do NOT quantize for you |
| crossterm `DetectColors::available_colors()` runtime query | Env-based tier detection (COLORTERM/TERM/NO_COLOR) | `available_colors()` needs a live terminal handle (untestable in CI, reports only 8–256 — cannot detect truecolor). Env heuristics are the ecosystem norm (bottom, gitui) and are pure + testable |

**Installation:** none — no new dependencies.

## Codebase Ground Truth (verified this session)

### Current styling inventory — `crates/ignition-tui/src/ui/*.rs`
| File | Color usage | Modifier usage |
|------|-------------|----------------|
| `dashboard.rs` | 4× `fg(Color::Red)` (panel "error" labels, lines 44/102/135/168); `bg(Color::DarkGray)` session-table `row_highlight_style` (line 200); `fg`-header `Modifier::BOLD` | BOLD header (198) |
| `logs.rs` | `level_style`: ERROR/FATAL→`Color::Red`, WARN→`Color::Yellow` (46–47); tests compare `buffer[(x,y)].fg` to `Color::Red`/`Color::Yellow` (226/231/237) | DEBUG/TRACE→`DIM` (48) |
| `mod.rs` (chrome) | none | BOLD active tab (88), BOLD/DIM projects-menu headers/descriptions (42/50); 2 test assertions on `Modifier::BOLD` (453/461 — modifier-only, grep-safe) |
| `tags.rs` | none | BOLD ×9, DIM ×1 |
| `alarms.rs` | none | BOLD ×3 |
| `projects.rs` | none | BOLD ×4 |
| `rig.rs` | none | BOLD ×3, DIM ×1 (doc comment: "monochrome-theme overhaul is backlog" — **that backlog is this phase**) |
| `profiles.rs` | none | BOLD ×1 |

**Count: exactly 10 `Color::` references (7 production + 3 test-side).** ~30 modifier sites are capability-safe and stay inline (`Modifier::` is NOT restricted by CI).

### Config plumbing (Phase 8, in place)
- `crates/ignition-core/src/config/profile.rs`: `UiConfig { theme: Option<String> }`, `lenient_ui` warn+default deserializer, `skip_serializing_if` at defaults (legacy configs round-trip byte-identically). Tests prove `theme = "dark"` round-trips.
- `config::load_for_tui` (mod.rs:67) is the TUI entry point; new-schema-surface failures warn+degrade, resolution failures fatal.
- `crates/ignition-tui/src/context.rs` `resolve()` → `ResolvedContext` → `lib.rs run_loop` adopts fields one block (`state.client/profile/profile_url/poll_interval = ctx.…`). **Theme rides the same pattern: validate name in `context::resolve`, put the resolved `Palette` on `AppState`.**
- All render fns are pure over `&AppState`; ~100 test callsites use `AppState::new()` — default-theme-on-new keeps every existing test valid.

### CI (.github/workflows/ci.yml — NOTE: it's `ci.yml`, not `check.yml`)
The `check` job (ubuntu/macos matrix) runs: fmt → clippy `-D warnings` → build → test → lean build (`--no-default-features`). A tokenization grep step slots in as an additional `run:` step (cheap, no toolchain needed — put it after checkout, before clippy).

## Architecture Patterns

### Recommended module layout
```
crates/ignition-tui/src/ui/
├── theme.rs          # NEW: the ONLY module allowed to name Color::
│                     #   - Slot enum / Palette struct (semantic slots → ratatui Color)
│                     #   - Theme registry: default, mono, dark, light × 4 tiers
│                     #   - Tier detection fn (pure over env snapshot)
│                     #   - Style helpers: fn error(p:&Palette)->Style etc.
├── mod.rs            # existing chrome — migrate BOLD/DIM spans to token styles where they
│                     #   carry meaning (headers→emphasis); plain modifiers stay inline
├── dashboard.rs      # migrate 5 Color literals → tokens
├── logs.rs           # migrate level_style + 3 test color comparisons → tokens
└── (others)          # modifiers only; optional emphasis/accent treatment
```

### Pattern 1: Token module with per-tier palettes (the core)
**What:** One `Palette` struct of semantic slots holding `ratatui::style::Color` values; each theme is a set of four explicitly-authored palettes (Truecolor, C256, C16, Mono); startup resolves tier → one `Palette` onto `AppState`.
**When to use:** Always in this phase — this IS the architecture.

```rust
// Source: pattern per k9s skin slots (k9scli.io/topics/skins) + bottom built-in themes
// (clementtsang.github.io/bottom, styling page, fetched 2026-09); API per ratatui 0.30 docs.
use ratatui::style::{Color, Modifier, Style};

/// Semantic style slots — the k9s/btop-convergent vocabulary, sized to this
/// cockpit's actual screens. THIS STRUCT IS THE CONTRACT: screens never
/// name a Color, they name a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub text: Color,        // default body text (mono: Reset)
    pub muted: Color,       // hints, descriptions, DEBUG/TRACE (pairs w/ DIM)
    pub emphasis: Color,    // active tab, section headers (pairs w/ BOLD)
    pub border: Color,      // pane borders
    pub title: Color,       // pane titles (k9s frame.title)
    pub header: Color,      // table headers (k9s views.table.header)
    pub selection_bg: Color,// row highlight (dashboard bg(DarkGray) today)
    pub indicator: Color,   // the ▸ selection marker
    pub error: Color,       // ERROR/FATAL + panel error labels (k9s status.errorColor)
    pub warning: Color,     // WARN (k9s status.modifyColor role)
    pub success: Color,     // running/OK states (k9s status.newColor role; headroom)
    pub accent: Color,      // profile banner, status-line profile name
}

/// Capability tiers, most- to least-capable. Mono = Reset + modifiers only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier { Truecolor, C256, C16, Mono }

/// One named theme; every tier is AUTHORED, never derived — the 16-color
/// and mono palettes are the readability guarantee (TUIX-04).
pub struct Theme {
    pub name: &'static str,
    pub truecolor: Palette, // may use Rgb — safe, tier-gated
    pub c256: Palette,      // named colors + Indexed(0..=255)
    pub c16: Palette,       // the 16 ANSI named colors ONLY
    pub mono: Palette,      // Color::Reset ONLY (readability by modifiers)
}

impl Theme {
    pub fn resolve(&self, tier: Tier) -> Palette {
        match tier { Tier::Truecolor => self.truecolor, Tier::C256 => self.c256,
                     Tier::C16 => self.c16, Tier::Mono => self.mono }
    }
    /// `None` for unknown names — the caller warns + falls back (lenient
    /// degradation, same contract as load_for_tui's new-schema surface).
    pub fn by_name(name: &str) -> Option<&'static Theme> { /* registry match */ }
}

/// Semantic style constructors — screens compose these with inline
/// modifiers; CI only forbids Color::, not Modifier::.
pub fn error(p: &Palette) -> Style { Style::default().fg(p.error) }
pub fn muted(p: &Palette)  -> Style { Style::default().fg(p.muted) }
pub fn selection(p: &Palette) -> Style { Style::default().bg(p.selection_bg) }
```

### Pattern 2: Tier detection as a pure function over an env snapshot
**What:** Map the terminal-capability env conventions to a `Tier`. Ecosystem-standard signals:
- `COLORTERM` == `truecolor` or `24bit` → `Truecolor` (the de-facto truecolor convention)
- `TERM` contains `256color` → `C256`
- `TERM` == `dumb`/unset or non-TTY → `Mono`
- `NO_COLOR` set non-empty → `Mono` (no-color.org convention: suppress added color)
- otherwise → `C16`
**When to use:** once in `context::resolve` / `run_loop`; the function itself takes an env snapshot (closure or `HashMap`) so tests need no real environment.

```rust
// Conventions: COLORTERM/truecolor + NO_COLOR are the widely-used heuristics
// (bottom, gitui et al.). crossterm's DetectColors::available_colors is NOT
// used: it requires a live terminal handle, reports only 8–256, and cannot
// detect truecolor — untestable in CI.
pub fn detect_tier(env: &dyn Fn(&str) -> Option<String>) -> Tier {
    if env("NO_COLOR").is_some_and(|v| !v.is_empty()) { return Tier::Mono; }
    match env("COLORTERM").as_deref() {
        Some("truecolor" | "24bit") => return Tier::Truecolor,
        _ => {}
    }
    match env("TERM").as_deref() {
        None | Some("dumb") => Tier::Mono,
        Some(t) if t.contains("256color") => Tier::C256,
        Some(_) => Tier::C16,
    }
}
```
The crate already has `ENV_LOCK` (`lib.rs:39`) serializing env-mutating tests crate-wide — reuse it if tests set real env vars; otherwise inject a fake snapshot.

### Pattern 3: Resolution + state placement (matches Phase 8 contract)
```rust
// context.rs resolve(): after load_for_tui succeeds
let theme = Theme::by_name(ctx_config.ui.theme.as_deref().unwrap_or("default"))
    .unwrap_or_else(|| {
        tracing::warn!(
            theme = ctx_config.ui.theme.as_deref().unwrap_or(""),
            "unknown [ui].theme — using default theme"
        );
        Theme::by_name("default").expect("default theme exists")
    });
// ResolvedContext gains: theme: &'static Theme  (or Palette + Tier)
// run_loop: state.palette = ctx.theme.resolve(ctx.tier);
```
`AppState::new()` initializes the default-theme default-tier palette, so every existing test and render fn is untouched by signature.

### Pattern 4: Buffer-level style assertions (extends the existing harness)
**What:** The repo already asserts `buffer[(x,y)].modifier.contains(Modifier::BOLD)` (ui/mod.rs:453) and `buffer[(x,y)].fg` (ui/logs.rs:220). Theme tests extend this: assert against the *token-resolved* value, never a literal.

```rust
// Cell has public fg: Color, bg: Color, modifier: Modifier fields
// (ratatui 0.30 buffer::Cell docs) — TestBackend makes them assertable.
let p = Theme::by_name("dark").unwrap().resolve(Tier::C16);
// ... render error state ...
assert_eq!(cell.fg, p.error, "error label uses the theme's error slot");
// Mono-tier readability contract:
for slot in [p.mono.text, p.mono.error /* ... */] {
    assert_eq!(slot, Color::Reset, "mono palette carries no color");
}
```

### Anti-Patterns to Avoid
- **Runtime quantization:** don't write an RGB→256→16 converter. Backends don't do it (verified), and naive quantization is exactly what produces unreadable contrast. Author the tiers.
- **Widget-level style leaking over span tokens:** ratatui styles are *incremental patches* — "when multiple styles are applied to a terminal buffer cell, the final result is a merge of all applied styles" (0.30 Style docs). Adding a widget-level `.style()` with a `fg` set will override/compose over span colors unpredictably. Keep color assignment at the span/cell level via tokens; don't add catch-all widget styles during tokenization.
- **Hardcoded literal colors in tests:** any test comparing `fg` to `Color::Red` breaks the moment a theme changes and violates the CI grep. Assert `cell.fg == palette.error`.
- **Applying theme colors to arbitrary content:** logs render messages as `Span::raw` deliberately ("the rest stays default so long messages never inherit a shouty hue" — logs.rs:28-29). Keep level styling span-scoped to the level token.
- **Treating `Modifier::DIM` as a color:** DIM is capability-universal and stays. Slots that *pair* with modifiers (muted+DIM, emphasis+BOLD) carry only the color; the modifier stays inline.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Truecolor→256→16 color reduction | RGB quantizer (median-cut etc.) | Authored per-tier palettes | Quantization is perceptually hard, silently destroys contrast, and is untestable for readability; authored tiers make "readable at 16" a code-review fact |
| Terminal capability probing | Queries of terminal DB / OSC responses | `detect_tier` over COLORTERM/TERM/NO_COLOR conventions | crossterm's `DetectColors::available_colors` needs a live handle, reports only 8–256, cannot see truecolor; env conventions are the ecosystem norm and pure-testable |
| Theme file format / user-defined palettes | TOML/YAML skin parser | `Theme::by_name` over a fixed built-in registry | TUIX-03 scopes *named* themes via `[ui].theme`; k9s ships YAML skins, bottom ships built-ins selected by name — for built-ins a Rust match is less code and fully typed. (User-defined palettes = future milestone, explicitly not Phase 12) |
| Rect/layout/geometry | anything | existing helpers (`Rect::centered`, `Layout`) | unchanged doctrine; theming must not touch geometry |

**Key insight:** every hand-rolled candidate above is a trap because its failure mode is *silent visual wrongness* — exactly what explicit authored palettes + buffer-asserted tests eliminate.

## Common Pitfalls

### Pitfall 1: Assuming the backend degrades colors for you
**What goes wrong:** `Rgb` values emit truecolor SGR sequences regardless of `TERM`; on old terminals the docs warn of "unpredictable visual artifacts" — TUIX-04 fails in the field, not in tests.
**Why it happens:** ratatui-crossterm's color mapping is a 1:1 variant passthrough (verified from source); there is no capability-aware downgrade.
**How to avoid:** `Rgb` appears ONLY in a theme's `truecolor` palette; `c256` uses named colors/`Indexed`; `c16` uses only the 16 ANSI named colors; `mono` uses only `Color::Reset`.
**Warning signs:** any `Color::Rgb` reachable outside `Theme.truecolor`.

### Pitfall 2: ratatui named colors are the ANSI-16 names — know which half you get
**What goes wrong:** confusion porting themes: ratatui `Color::Red` ↔ crossterm `DarkRed` (SGR 31, the dim ANSI red); ratatui `Color::LightRed` ↔ crossterm `Red` (SGR 91, bright). A palette that "looks right" in one viewer can render muddy/washed in another.
**How to avoid:** author the `c16` tier from actual 16-color swatches (or verify on a real `TERM=xterm` run); don't assume `Light*` is "same hue, brighter" semantically across terminals.
**Source:** ratatui-crossterm `FromCrossterm<CrosstermColor> for Color` mapping (verified, docs.rs).

### Pitfall 3: Style patching overrides token colors
**What goes wrong:** a widget-level `.style(Style::default().fg(x))` merges over span styles (incremental-patch semantics, verified) and a later token change "mysteriously" stops showing.
**How to avoid:** token colors only at span/cell level; widget styles stay `None` unless deliberately overriding everything.
**Warning signs:** a buffer assertion that passes at one tier and fails at another with no palette change.

### Pitfall 4: Light-terminal unreadability
**What goes wrong:** the current `bg(Color::DarkGray)` selection and "Light*" accents assume a dark background; on a white terminal they vanish or glare.
**How to avoid:** that's precisely why `light` ships as its own named theme rather than a re-tint of `dark`; never assume the terminal background (k9s even has a `default` "transparent" color concept for this reason).
**Warning signs:** UAT on `TERM=xterm` with a white background.

### Pitfall 5: Mono tier contrast collapse
**What goes wrong:** `bg(DarkGray)` selection with default fg, or color-only distinctions, become invisible when the mono palette zeroes all colors.
**How to avoid:** mono tier leans on modifiers for structure — selection becomes `Modifier::REVERSED` (the k9s/btop mono-cursor move), errors pair REVERSED or BOLD with `Reset`; enforce in a test that every mono palette slot == `Color::Reset`.
**Warning signs:** a mono-tier test that needs a non-Reset color to pass.

### Pitfall 6: The CI grep false-positive/negative holes
**What goes wrong:** a grep for `Color::` misses `use ratatui::style::Color;`-style imports (usage then reads `Color::Red` — actually caught) or fully-qualified `ratatui::style::Color::Red` (also caught by `Color::`), but an alias like `use ratatui::style::Color as C;` slips through; conversely grepping bare `Color` false-positives on `ContentStyle`/comments.
**How to avoid:** forbid `Color::` **and** the import/alias forms: `Color::`, `style::Color` (covers `use …::Color;` and `use …::{…Color…}` only if pattern extended — use `[^A-Za-z]Color[^A-Za-z:]`-ish is overkill; simplest robust rule: forbid `Color::` plus `style::Color` and `Color as`), restricted to `crates/` with `src/ui/theme.rs` (and `theme.rs`'s own `#[cfg(test)]` block) exempted.
**Warning signs:** CI green while a screen file grows a new hue.

### Pitfall 7: Env-var tests racing
**What goes wrong:** capability tests that mutate real env vars race other tests (edition 2024 makes `set_var` unsafe for good reason).
**How to avoid:** inject an env snapshot into `detect_tier`; if real env is needed, take the existing crate-wide `ENV_LOCK` (`ignition-tui/src/lib.rs:39`).

## CI Tokenization Enforcement (concrete)

Add to the `check` job in `.github/workflows/ci.yml` (after `actions-rust-lang/setup-rust-toolchain`/cache, alongside the other `run:` steps — it needs no toolchain):

```yaml
      # Phase 12 tokenization-first discipline: Color:: literals live ONLY
      # in the style-tokens module (ui/theme.rs). Also catches the import
      # forms that could smuggle usage past a bare-Color:: grep.
      - name: style tokens only (Color:: confined to ui/theme.rs)
        run: |
          offenders=$(grep -rn -e 'Color::' -e 'style::Color' -e 'Color as' crates/ \
            --include='*.rs' \
            | grep -v 'src/ui/theme.rs' || true)
          if [ -n "$offenders" ]; then
            echo "::error::Color:: outside ui/theme.rs — add a slot instead:"
            echo "$offenders"
            exit 1
          fi
```

Migration consequences to plan for: `ui/logs.rs` tests (lines 226/231/237) currently compare `buffer[(x,y)].fg` to `ratatui::style::Color::Red`/`Yellow` — they must move to palette comparisons in the same change, or CI goes red. `ui/mod.rs` test assertions on `Modifier::BOLD` (453/461) are modifier-only and safe.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| ratatui theme crates (early `ratatui-theme`-style experiments) | Hand-rolled theme structs in mature apps (bottom: Default/Nord/Gruvbox + light variants; k9s: YAML skins; btop: `.theme` files) | Stable pattern through 2025–2026 | Confirms this phase's direction (already locked by v1.1 stack research) |
| Trusting terminal/backends to map colors | Explicit capability tiers authored per theme | Longstanding (truecolor ecosystem reality) | TUIX-04 must be authored, not assumed |
| `NO_COLOR` ignored | Honored as a signal (suppress added color) | no-color.org convention, widely adopted | Cheap mono-tier trigger; plan should decide whether ign honors it (recommend yes) |

**Deprecated/outdated:** none of the in-use APIs. ratatui 0.30's `Style`/`Color`/`Stylize` are current; `Rect::centered` (already used in mod.rs) is the 0.30 idiom.

## Theme Set Recommendation

Ship four named themes (bottom ships three + light variants; k9s ships dozens — four is the right floor for TUIX-03's "monochrome + color palettes"):

| Name | Character | Purpose |
|------|-----------|---------|
| `default` | Current look formalized: near-monochrome + error red / warn yellow | `None`/unknown falls back here — zero visual regression risk |
| `mono` | Pure monochrome: `Color::Reset` everywhere, structure via BOLD/DIM/REVERSED | The strictest degradation witness; also the accessibility/minimal pick |
| `dark` | The full color cockpit for dark terminals (all ~14 slots colored) | The showcase palette |
| `light` | Full color for light backgrounds (different accent/dim choices, no `DarkGray`-bg assumptions) | Readability on white-background terminals |

`theme = "unknown-name"` → `tracing::warn!` + default (lenient-degradation pattern; the warning surfaces on stderr before the alternate screen, matching the established `load_for_tui` contract).

## Open Questions

1. **Exact accent choices for `dark`/`light` palettes (which hues for accent/success/border).**
   - What we know: slot structure, tier authoring, and readability contracts are settled; the *aesthetic* values are taste.
   - Recommendation: planner treats palette VALUES as low-stakes (adjustable post-UAT); the structure/tests are the deliverable. Keep `dark` conservative (ANSI-family hues) for c16 fidelity.
2. **`NO_COLOR` — honor or ignore?**
   - What we know: it's the established convention (no-color.org) and costs one line in `detect_tier`; no project decision exists either way.
   - Recommendation: honor it (→ `Mono`). It's also the easiest manual degradation test (`NO_COLOR=1 ign tui`).
3. **Whether screens other than dashboard/logs get *new* color treatment in this phase or just tokenization scaffolding.**
   - What we know: rig.rs's doc comment invites the overhaul; success criteria require consistency + tokens, not maximally colorful screens.
   - Recommendation: planner scopes "apply tokens everywhere, color only where semantics demand (error/warn/selection/accent)" — the `success`/`accent` slots exist but using them broadly is optional polish.

## Sources

### Primary (HIGH confidence)
- Context7 `/websites/rs_ratatui_0_30_0` — `Style` (patch/incremental-merge semantics, `Style::reset`, `fg/bg`), `Color` enum (Rgb terminal-support warning, `from_str`), `Cell` public fields (fg/bg/modifier), `TestBackend`, `CrosstermBackend::available_colors` (8–256 floor)
- Context7 `/websites/rs_ratatui-crossterm` — full `FromCrossterm` color mapping source (proves 1:1 passthrough, no quantization; ratatui-Red ↔ crossterm-DarkRed mapping) and `IntoCrossterm<ContentStyle> for Style` modifier mapping
- k9s official skins docs — https://k9scli.io/topics/skins/ (slot vocabulary: body/frame.border/frame.menu/frame.crumbs/frame.status.*/frame.title/views.table(+header)/views.yaml/views.logs; `skin:` config key)
- bottom official styling docs — https://clementtsang.github.io/bottom/nightly/configuration/config-file/styling/ (fetched 2026-09-14, dated Sep 12 2026: built-in themes incl. light variants, `styles.theme = "gruvbox"` config selection, ratatui named-color vocabulary)
- Repo ground truth (this session): ui/*.rs inventory, `Cargo.toml` workspace pins (ratatui 0.30.2 / crossterm 0.29 event-stream), `.github/workflows/ci.yml`, `config/profile.rs` (`UiConfig`, `lenient_ui`), `config/mod.rs` (`load_for_tui`), `ignition-tui/src/lib.rs` (run/resolve adoption pattern, `ENV_LOCK`)

### Secondary (MEDIUM confidence)
- COLORTERM `truecolor`/`24bit`, `TERM=…256color`, `TERM=dumb` heuristics — the de-facto ecosystem convention (used by bottom/gitui et al.); consistent across sources but not standardized by any spec
- `NO_COLOR` → suppress color — no-color.org convention, widely adopted

### Tertiary (LOW confidence)
- none — no load-bearing claim rests on unverified single sources

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new deps; all APIs verified against ratatui 0.30 docs via Context7; workspace pins confirmed in-repo
- Architecture (tokens/tiers/registry/state placement): HIGH — pattern verified against two production precedents (k9s, bottom) and the repo's own Phase 8 degradation contract; slot vocabulary is a grounded synthesis (MEDIUM on exact aesthetic values, which are adjustable)
- Pitfalls: HIGH — each pitfall tied to verified API behavior (patch semantics, passthrough color mapping, Cell fields) or in-repo facts (ENV_LOCK, test assertions that must migrate)
- CI enforcement: HIGH — exact workflow file and step shape confirmed in-repo; grep migration consequences identified (logs.rs test literals)

**Research date:** 2026-09-14
**Valid until:** ~2026-10-14 (ratatui 0.30.x is floor-pinned; palette aesthetics are timeless)

## RESEARCH COMPLETE

**Phase:** 12 - tui-theming-degradation
**Confidence:** HIGH

### Key Findings
- Exactly 10 `Color::` references exist project-wide (dashboard ×5, logs ×2 production, logs tests ×3); ~30 more styling sites are capability-safe `Modifier::BOLD/DIM` — tokenization is a bounded migration, not a restyle of everything.
- Verified: CrosstermBackend does NO color quantization (1:1 variant passthrough; docs warn of "unpredictable visual artifacts" for Rgb on old terminals) — TUIX-04 must be authored as explicit per-tier palettes (truecolor/256/16/mono), never derived.
- Ecosystem convergence (k9s skins, bottom built-in themes) validates hand-rolled named themes + semantic slots, selected by a plain config key with fallback to default — matching the already-locked Phase 8 lenient-degradation contract.
- Theme resolution rides the existing `context::resolve` → `run_loop` → `AppState` adoption pattern; default-theme `AppState::new()` keeps all ~100 test callsites and every pure render fn signature untouched.
- CI enforcement is a one-step grep in the existing `ci.yml` `check` job (file is `ci.yml`, not `check.yml`); it forces the logs.rs test color literals to migrate to palette comparisons in the same change.

### File Created
`.planning/phases/12-tui-theming-degradation/12-RESEARCH.md`

### Confidence Assessment
| Area | Level | Reason |
|------|-------|--------|
| Standard Stack | HIGH | No new deps; all APIs Context7-verified against ratatui 0.30 |
| Architecture | HIGH | Two production precedents + in-repo degradation contract; slot vocab grounded in real call sites |
| Pitfalls | HIGH | Each tied to verified API behavior or in-repo facts |

### Open Questions
- Exact hue choices for dark/light palettes (taste — planner should treat values as adjustable post-UAT)
- Honor `NO_COLOR`? (recommended yes — one line, best manual degradation test)
- Scope of *new* color application beyond dashboard/logs (recommended: tokens everywhere, color only where semantics demand)

### Ready for Planning
Research complete. Planner can now create PLAN.md files.
