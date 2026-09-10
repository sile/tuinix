//! Property-based tests for `TerminalStyle`, driven by noprop.
//!
//! The properties covered here use only the public API:
//!
//! - A style emitted via `Display` parses back to the original style
//!   (`FromStr` round-trip).

mod helpers;

use std::cell::Cell;

use helpers::run;

/// Draws a color biased toward the `0` / `128` / `255` components, so that the
/// boundaries of the decimal rendering are exercised.
fn sample_color(ctx: &mut noprop::TestCaseContext) -> tuinix::TerminalColor {
    tuinix::TerminalColor::new(
        sample_color_component(ctx),
        sample_color_component(ctx),
        sample_color_component(ctx),
    )
}

/// Draws a single color component.
fn sample_color_component(ctx: &mut noprop::TestCaseContext) -> u8 {
    noprop::sample_with_boundaries(ctx, &[0u8, 128, 255], noprop::Ratio::one_nth(5), |ctx| {
        noprop::sample_usize_in(ctx, 0..=255) as u8
    })
}

/// Draws a random `TerminalStyle`, including the reset style.
fn sample_style(ctx: &mut noprop::TestCaseContext) -> tuinix::TerminalStyle {
    const SETTERS: [fn(tuinix::TerminalStyle) -> tuinix::TerminalStyle; 7] = [
        tuinix::TerminalStyle::bold,
        tuinix::TerminalStyle::italic,
        tuinix::TerminalStyle::underline,
        tuinix::TerminalStyle::blink,
        tuinix::TerminalStyle::reverse,
        tuinix::TerminalStyle::dim,
        tuinix::TerminalStyle::strikethrough,
    ];
    if noprop::sample_weighted_index(ctx, &[1, 9]) == 0 {
        return tuinix::TerminalStyle::new();
    }
    let mut style = tuinix::TerminalStyle::new();
    for setter in SETTERS {
        if noprop::sample_bool(ctx) {
            style = setter(style);
        }
    }
    if noprop::sample_bool(ctx) {
        style = style.fg_color(sample_color(ctx));
    }
    if noprop::sample_bool(ctx) {
        style = style.bg_color(sample_color(ctx));
    }
    style
}

/// Every `TerminalStyle` must round-trip through its ANSI escape sequence
/// representation.
#[test]
fn style_roundtrip_matches_display() -> noprop::TestResult {
    let observed_default = Cell::new(false);
    let observed_styled = Cell::new(false);
    let observed_fg = Cell::new(false);
    let observed_bg = Cell::new(false);
    let runner = run(256, |ctx| {
        let style = sample_style(ctx);
        let text = style.to_string();
        let parsed = text
            .parse::<tuinix::TerminalStyle>()
            .unwrap_or_else(|e| panic!("{style:?} emitted {text:?}, which fails to parse: {e}"));
        assert_eq!(parsed, style, "style round-trip mismatch");
        if style == tuinix::TerminalStyle::new() {
            observed_default.set(true);
        } else {
            observed_styled.set(true);
        }
        if style.fg_color.is_some() {
            observed_fg.set(true);
        }
        if style.bg_color.is_some() {
            observed_bg.set(true);
        }
        Ok(())
    })?;
    assert!(
        observed_default.get(),
        "no case exercised the default style\n{runner}"
    );
    assert!(
        observed_styled.get(),
        "no case exercised a styled style\n{runner}"
    );
    assert!(observed_fg.get(), "no case exercised an fg color\n{runner}");
    assert!(observed_bg.get(), "no case exercised a bg color\n{runner}");
    Ok(())
}
