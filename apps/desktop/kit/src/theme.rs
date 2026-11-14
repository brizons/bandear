//! The design tokens of the web console (`ui/app/tokens.css`), for GPUI: one dark theme, the
//! colors, the type scale, the 4 px spacing grid, the radii and the hit targets. The fonts (IBM Plex
//! Sans and JetBrains Mono, both variable, with their OFL texts) ship inside the binary.

use std::borrow::Cow;

use gpui::{App, BoxShadow, Hsla, Pixels, point, px, rgb, rgba};

/// The colors, as `0xRRGGBB`.
pub mod colors {
    pub const SIDEBAR: u32 = 0x0b0f14;
    pub const GROUND: u32 = 0x0d1117;
    pub const SURFACE_SUNKEN: u32 = 0x12171e;
    pub const SURFACE: u32 = 0x161b22;
    pub const SURFACE_RAISED: u32 = 0x1f2630;
    pub const BORDER_SUBTLE: u32 = 0x1f2630;
    pub const BORDER: u32 = 0x2b3440;
    pub const TEXT: u32 = 0xe6edf3;
    pub const TEXT_SECONDARY: u32 = 0xc5ced8;
    pub const TEXT_MUTED: u32 = 0x9ba8b5;
    pub const TEXT_ON_BRIGHT: u32 = 0x0d1117;
    pub const ACCENT: u32 = 0xf5a524;
    pub const ACCENT_HOVER: u32 = 0xffc35c;
    pub const GREEN: u32 = 0x3fb950;
    pub const RED: u32 = 0xf85149;
    pub const BLUE: u32 = 0x4cc2ff;
    pub const FOCUS: u32 = BLUE;
    pub const OK_BG: u32 = 0x10261a;
    pub const OK_BORDER: u32 = 0x1f6f3a;
    pub const WARN_BG: u32 = 0x2a1f0b;
    pub const WARN_BORDER: u32 = 0x6b4a12;
    pub const TRACK: u32 = 0x262f3a;
    pub const FADER_FILL: u32 = 0x5b6b7c;
    pub const FADER_FILL_MUTED: u32 = 0x3a4452;
    pub const THUMB: u32 = 0xe6edf3;
    pub const METER_GROUND: u32 = 0x0b0f14;
    pub const METER_SAFE: u32 = GREEN;
    pub const METER_HOT: u32 = ACCENT;
    pub const METER_CLIP: u32 = RED;
    pub const GROUP_TEAL_BG: u32 = 0x123a3a;
    pub const GROUP_TEAL_TEXT: u32 = 0x7fe0d0;
    pub const GROUP_BLUE_BG: u32 = 0x1c2a44;
    pub const GROUP_BLUE_TEXT: u32 = 0x9cc3ff;
    pub const GROUP_VIOLET_BG: u32 = 0x2a2346;
    pub const GROUP_VIOLET_TEXT: u32 = 0xc3b5ff;
    pub const GROUP_ORANGE_BG: u32 = 0x3a2412;
    pub const GROUP_ORANGE_TEXT: u32 = 0xffc58a;
    pub const GROUP_NONE_BG: u32 = 0x262f3a;
    pub const GROUP_NONE_TEXT: u32 = 0xc5ced8;
    pub const MARKER_BG: u32 = 0x10283a;
    pub const MARKER_TEXT: u32 = 0x9fdcff;
    pub const ACCENT_BG: u32 = 0x1a1610;
    pub const DANGER_BG: u32 = 0x2a1414;
    pub const DANGER_BORDER: u32 = 0x6b2a2a;
    pub const DANGER_TEXT: u32 = 0xffb4ad;
    /// The brand mark's stroke.
    pub const BRAND: u32 = 0xffbf43;
}

/// A token color.
pub fn color(hex: u32) -> Hsla {
    rgb(hex).into()
}

/// The sans font: every word.
pub const FONT_SANS: &str = "IBM Plex Sans";
/// The mono font: numbers (device and format readouts, chip times, strip readouts).
pub const FONT_MONO: &str = "JetBrains Mono";

/// The type scale, in px.
pub mod text {
    pub const XXS: f32 = 10.0;
    pub const XS: f32 = 11.0;
    pub const SM: f32 = 12.0;
    pub const MD: f32 = 13.0;
    pub const BASE: f32 = 14.0;
    pub const LG: f32 = 15.0;
    pub const XL: f32 = 20.0;
    pub const XXL: f32 = 26.0;
    /// The body's line height, as a multiple of the size.
    pub const LINE_HEIGHT: f32 = 1.4;
    /// The headings' line height.
    pub const HEADING_LINE_HEIGHT: f32 = 1.25;
    /// The tracking of caps captions, as a fraction of the size.
    pub const TRACKING_CAPS: f32 = 0.1;
}

/// The weights.
pub mod weight {
    use gpui::FontWeight;

    pub const REGULAR: FontWeight = FontWeight(400.0);
    pub const MEDIUM: FontWeight = FontWeight(500.0);
    pub const BOLD: FontWeight = FontWeight(600.0);
}

/// The 4 px spacing grid, in px.
pub mod space {
    pub const S1: f32 = 4.0;
    pub const S2: f32 = 8.0;
    pub const S3: f32 = 12.0;
    pub const S4: f32 = 16.0;
    pub const S5: f32 = 20.0;
    pub const S6: f32 = 24.0;
    pub const S8: f32 = 32.0;
}

/// The radii, in px.
pub mod radius {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 6.0;
    pub const MD: f32 = 10.0;
    pub const LG: f32 = 12.0;
    pub const XL: f32 = 16.0;
    pub const PILL: f32 = 999.0;
}

/// Every control's hit target.
pub const TARGET: f32 = 32.0;
/// A primary control's hit target.
pub const TARGET_PRIMARY: f32 = 40.0;
/// The sidebar, sized to fit "Network and system".
pub const SIDEBAR_WIDTH: f32 = 212.0;
/// The sidebar as an icon rail.
pub const RAIL_WIDTH: f32 = 64.0;
/// At this window width and below the sidebar is an icon rail.
pub const RAIL_BREAKPOINT: f32 = 1099.0;

/// A px length.
pub fn size(px_value: f32) -> Pixels {
    px(px_value)
}

/// The shadow under a fader's or a pan's thumb: `0 2px 6px rgb(0 0 0 / 50%)`.
pub fn thumb_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(0x0000_0080).into(),
        offset: point(px(0.0), px(2.0)),
        blur_radius: px(6.0),
        spread_radius: px(0.0),
        inset: false,
    }]
}

/// The fonts the kit ships.
const FONTS: [&[u8]; 2] = [
    include_bytes!("../assets/fonts/ibm_plex_sans_variable.ttf"),
    include_bytes!("../assets/fonts/jetbrains_mono_variable.ttf"),
];

/// Adds the kit's fonts to the app's text system.
pub fn load_fonts(cx: &App) -> gpui::Result<()> {
    cx.text_system()
        .add_fonts(FONTS.iter().map(|font| Cow::Borrowed(*font)).collect())
}

/// Puts the tokens on gpui-component's theme, so its select, input and popovers wear them too.
pub fn apply_to_components(cx: &mut App) {
    use gpui_component::{Theme, ThemeMode};

    Theme::change(ThemeMode::Dark, None, cx);
    Theme::update(cx, |theme| {
        theme.font_family = FONT_SANS.into();
        theme.mono_font_family = FONT_MONO.into();
        theme.font_size = px(text::BASE);
        theme.radius = px(radius::SM);
        let palette = &mut theme.colors;
        palette.background = color(colors::SURFACE_RAISED);
        palette.foreground = color(colors::TEXT);
        palette.border = color(colors::BORDER);
        palette.input = color(colors::BORDER);
        palette.ring = color(colors::FOCUS);
        palette.popover = color(colors::SURFACE);
        palette.popover_foreground = color(colors::TEXT);
        palette.accent = color(colors::SURFACE_RAISED);
        palette.accent_foreground = color(colors::TEXT);
        palette.muted = color(colors::SURFACE_SUNKEN);
        palette.muted_foreground = color(colors::TEXT_MUTED);
        palette.primary = color(colors::ACCENT);
        palette.primary_foreground = color(colors::TEXT_ON_BRIGHT);
        palette.list_active = color(colors::SURFACE_RAISED);
        palette.list_hover = color(colors::SURFACE_RAISED);
        palette.list_active_border = color(colors::FOCUS);
        palette.caret = color(colors::TEXT);
        palette.selection = color(colors::MARKER_BG);
    });
}
