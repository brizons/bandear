//! The kit's own icons, served to GPUI by path, and gpui-component's icons for every other path.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

/// The brand mark (`ui/app/brand/bandear-mark.svg`, from `docs/brand/bandear-mark.svg`).
pub const BRAND_MARK: &str = "bandear/brand-mark.svg";
/// The warning triangle of the top bar's attention link and of alerts.
pub const WARNING: &str = "bandear/warning.svg";
/// The play triangle of the soundcheck chip.
pub const PLAY: &str = "bandear/play.svg";

/// The triangle of a link's warning mark.
pub const TRIANGLE: &str = "bandear/triangle.svg";

/// The kit's icons, by path.
const ICONS: [(&str, &[u8]); 10] = [
    (TRIANGLE, include_bytes!("../assets/icons/triangle.svg")),
    (
        BRAND_MARK,
        include_bytes!("../assets/icons/bandear-mark.svg"),
    ),
    (WARNING, include_bytes!("../assets/icons/warning.svg")),
    (PLAY, include_bytes!("../assets/icons/play.svg")),
    (
        "bandear/nav-mix.svg",
        include_bytes!("../assets/icons/nav-mix.svg"),
    ),
    (
        "bandear/nav-record.svg",
        include_bytes!("../assets/icons/nav-record.svg"),
    ),
    (
        "bandear/nav-channels.svg",
        include_bytes!("../assets/icons/nav-channels.svg"),
    ),
    (
        "bandear/nav-processing.svg",
        include_bytes!("../assets/icons/nav-processing.svg"),
    ),
    (
        "bandear/nav-effects.svg",
        include_bytes!("../assets/icons/nav-effects.svg"),
    ),
    (
        "bandear/nav-network.svg",
        include_bytes!("../assets/icons/nav-network.svg"),
    ),
];

/// The path of a section's sidebar icon.
pub fn nav_icon(section: bandear_desktop_console::router::Section) -> &'static str {
    use bandear_desktop_console::router::Section;
    match section {
        Section::Mix => "bandear/nav-mix.svg",
        Section::Record => "bandear/nav-record.svg",
        Section::Channels => "bandear/nav-channels.svg",
        Section::Processing => "bandear/nav-processing.svg",
        Section::Effects => "bandear/nav-effects.svg",
        Section::Network => "bandear/nav-network.svg",
    }
}

/// The app's asset source: the kit's icons, then gpui-component's.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = ICONS.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(*bytes)));
        }
        gpui_kit_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut listed: Vec<SharedString> = ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect();
        listed.extend(gpui_kit_assets::Assets.list(path)?);
        Ok(listed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bandear_desktop_console::router::Section;

    #[test]
    fn every_section_has_its_icon_and_the_kit_serves_it() {
        for section in Section::ALL {
            let icon = Assets
                .load(nav_icon(section))
                .expect("loads")
                .expect("an icon");
            assert!(icon.starts_with(b"<svg"), "{section:?}");
        }
        assert!(Assets.load(BRAND_MARK).expect("loads").is_some());
    }
}
