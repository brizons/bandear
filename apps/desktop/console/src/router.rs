//! The console's six sections and where the window is (`router.js`).
//!
//! A native window has no address bar, so the section shown is app state ([Route]); the web
//! console's hashes still read into it, so a deep link such as `#/processing/5` keeps meaning "the
//! Processing screen on CH 5".

/// A section of the console, in the sidebar's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Section {
    Mix,
    Record,
    Channels,
    Processing,
    Effects,
    Network,
}

/// The last input a processing route can name.
pub const LAST_CHANNEL: u8 = 32;

impl Section {
    /// Every section, in the sidebar's order, Mix first.
    pub const ALL: [Section; 6] = [
        Section::Mix,
        Section::Record,
        Section::Channels,
        Section::Processing,
        Section::Effects,
        Section::Network,
    ];

    /// The section a window opens on, and the one an unknown route opens.
    pub const DEFAULT: Section = Section::Mix;

    /// The section's id (`mix`, `record`, ...).
    pub fn id(self) -> &'static str {
        match self {
            Self::Mix => "mix",
            Self::Record => "record",
            Self::Channels => "channels",
            Self::Processing => "processing",
            Self::Effects => "effects",
            Self::Network => "network",
        }
    }

    /// The section's hash in the web console (`#/mix`).
    pub fn hash(self) -> String {
        format!("#/{}", self.id())
    }

    /// The sidebar's word for the section, also its title.
    pub fn label(self) -> &'static str {
        match self {
            Self::Mix => "Mix",
            Self::Record => "Record",
            Self::Channels => "Channels and groups",
            Self::Processing => "Processing",
            Self::Effects => "Effects",
            Self::Network => "Network and system",
        }
    }

    /// The section's own title, as the heading and the document title name it.
    pub fn title(self) -> &'static str {
        self.label()
    }
}

/// Where the window is: a section, and for Processing the channel it opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Route {
    pub section: Section,
    /// The channel (1-32) a Processing route names, if any.
    pub channel: Option<u8>,
}

impl Route {
    /// The route a web console hash names (see [resolve_route] and [route_channel]).
    pub fn from_hash(hash: &str) -> Self {
        let section = resolve_route(hash);
        let channel = if section == Section::Processing {
            route_channel(hash)
        } else {
            None
        };
        Self { section, channel }
    }

    /// The route's canonical hash.
    pub fn hash(self) -> String {
        match self.channel {
            Some(channel) if self.section == Section::Processing => processing_hash(channel),
            _ => self.section.hash(),
        }
    }
}

impl From<Section> for Route {
    fn from(section: Section) -> Self {
        Self {
            section,
            channel: None,
        }
    }
}

/// A hash's path: without its leading `#/` or `#` and trailing slashes, lowercase, split on `/`.
fn path_of(hash: &str) -> Vec<String> {
    let path = hash.strip_prefix('#').unwrap_or(hash);
    let path = path.strip_prefix('/').unwrap_or(path);
    path.trim_end_matches('/')
        .to_lowercase()
        .split('/')
        .map(str::to_owned)
        .collect()
}

/// The section a hash opens: a leading `#/` or `#`, trailing slashes and case do not matter, and
/// extra segments are allowed only for Processing; anything else opens the Mix.
pub fn resolve_route(hash: &str) -> Section {
    let path = path_of(hash);
    let id = path.first().map(String::as_str).unwrap_or("");
    if path.len() > 1 && id != "processing" {
        return Section::DEFAULT;
    }
    Section::ALL
        .into_iter()
        .find(|section| section.id() == id)
        .unwrap_or(Section::DEFAULT)
}

/// The channel a `processing/<n>` hash names, 1-32, or `None`.
pub fn route_channel(hash: &str) -> Option<u8> {
    let path = path_of(hash);
    if path.len() != 2 || path[0] != "processing" {
        return None;
    }
    let channel = &path[1];
    if channel.is_empty() || !channel.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    channel
        .parse::<u32>()
        .ok()
        .filter(|number| (1..=u32::from(LAST_CHANNEL)).contains(number))
        .map(|number| number as u8)
}

/// The hash of the Processing screen on `channel`.
pub fn processing_hash(channel: u8) -> String {
    format!("#/processing/{channel}")
}

/// The hash a hash stands for: its section's, or `#/processing/<n>` when it names a channel.
pub fn canonical_hash(hash: &str) -> String {
    Route::from_hash(hash).hash()
}

/// The document title of a section: `Mix · BandEar`.
pub fn document_title(section: Section) -> String {
    format!("{} \u{b7} BandEar", section.title())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sidebar_holds_every_section_in_the_designs_order_mix_first() {
        let sections: Vec<_> = Section::ALL
            .iter()
            .map(|section| (section.id(), section.hash(), section.label()))
            .collect();
        assert_eq!(
            sections,
            [
                ("mix", "#/mix".to_owned(), "Mix"),
                ("record", "#/record".to_owned(), "Record"),
                ("channels", "#/channels".to_owned(), "Channels and groups"),
                ("processing", "#/processing".to_owned(), "Processing"),
                ("effects", "#/effects".to_owned(), "Effects"),
                ("network", "#/network".to_owned(), "Network and system"),
            ]
        );
        assert_eq!(Section::DEFAULT, Section::Mix);
    }

    #[test]
    fn a_hash_opens_its_section() {
        assert_eq!(resolve_route("#/mix"), Section::Mix);
        assert_eq!(resolve_route("#/record"), Section::Record);
        assert_eq!(resolve_route("#/channels"), Section::Channels);
        assert_eq!(resolve_route("#/processing"), Section::Processing);
        assert_eq!(resolve_route("#/effects"), Section::Effects);
        assert_eq!(resolve_route("#/network"), Section::Network);
    }

    #[test]
    fn a_hash_written_a_little_differently_still_opens_its_section() {
        assert_eq!(resolve_route("#/network/"), Section::Network);
        assert_eq!(resolve_route("#/Network"), Section::Network);
        assert_eq!(resolve_route("#network"), Section::Network);
    }

    #[test]
    fn no_hash_or_one_that_names_no_section_opens_the_mix() {
        for hash in ["", "#", "#/", "#/reverb", "#main", "#/mix/3"] {
            assert_eq!(resolve_route(hash), Section::Mix, "{hash}");
        }
    }

    #[test]
    fn the_processing_editors_hash_names_the_channel_it_opens_on() {
        assert_eq!(processing_hash(5), "#/processing/5");
        assert_eq!(resolve_route("#/processing/5"), Section::Processing);
        assert_eq!(route_channel("#/processing/5"), Some(5));
        assert_eq!(route_channel("#/Processing/12/"), Some(12));
        for hash in [
            "#/processing",
            "#/processing/0",
            "#/processing/33",
            "#/processing/x",
            "#/mix",
        ] {
            assert_eq!(route_channel(hash), None, "{hash}");
        }
        assert_eq!(canonical_hash("#/processing/5/"), "#/processing/5");
        assert_eq!(canonical_hash("#/processing/99"), "#/processing");
        assert_eq!(canonical_hash("#/Network"), "#/network");
        assert_eq!(canonical_hash(""), "#/mix");
        assert_eq!(
            Route::from_hash("#/processing/7"),
            Route {
                section: Section::Processing,
                channel: Some(7)
            }
        );
        assert_eq!(Route::from_hash("#/processing/7").hash(), "#/processing/7");
    }

    #[test]
    fn the_window_title_names_the_section() {
        assert_eq!(
            document_title(resolve_route("#/network")),
            "Network and system \u{b7} BandEar"
        );
    }
}
