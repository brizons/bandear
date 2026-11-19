import BandEarMix

/// The saved mixes in `SavedMixFixtures/`, as the app holds them in memory.
///
/// Each fixture file is the text the iOS app kept under `bandear.banks` in `UserDefaults` for these
/// mixes, written by its own Swift encoder (`SavedMixesJSON`, `JSONEncoder` with `.sortedKeys` and
/// `.withoutEscapingSlashes`) before the shared core took the saved mixes over. A saved mix sits on
/// a phone for weeks, so whatever reads it now must read these files to exactly these values.
///
/// Together they hold every field the format has (a name, and a snapshot with four per-channel
/// lists, the group levels, the master volume, the cap and the mute), an empty bank, the default
/// snapshot, the ends of every range, the desk's largest mix, and names with `/`, quotes, a
/// backslash, accented and non-Latin letters, an emoji, line breaks and other control characters.
enum SavedMixFixtures {
    /// Each fixture file's name, without `.json`, with the mixes it holds in the order saved.
    static let banks: [(file: String, mixes: [SavedMix])] = [
        ("empty-bank", []),
        (
            "one-mix",
            [
                SavedMix(
                    name: "Rehearsal",
                    snapshot: MixSnapshot(
                        channelGains: [80, 100, 40], channelPans: [0, 50, 100],
                        channelMutes: [false, true, false], channelSolos: [true, false, false],
                        groupLevels: [60, 90], volumePercent: 70, maxLevelPercent: 90, muted: true))
            ]
        ),
        (
            "several-mixes",
            [
                SavedMix(
                    name: "Verse",
                    snapshot: MixSnapshot(
                        channelGains: [30, 60], channelPans: [25, 75], channelMutes: [true, false],
                        channelSolos: [false, true], groupLevels: [40], volumePercent: 85,
                        maxLevelPercent: 95, muted: false)),
                SavedMix(name: "Show / A", snapshot: MixSnapshot()),
                SavedMix(
                    name: #"Lead "vox" \ back"#,
                    snapshot: MixSnapshot(
                        channelGains: [100], channelPans: [50], channelMutes: [false],
                        channelSolos: [false], groupLevels: [], volumePercent: 100,
                        maxLevelPercent: 100, muted: true)),
                SavedMix(
                    name: "Bühne – Monitor ñ 🎸 混音",
                    snapshot: MixSnapshot(
                        channelGains: [0, 100], channelPans: [0, 100], channelMutes: [true, true],
                        channelSolos: [true, true], groupLevels: [0, 100], volumePercent: 0,
                        maxLevelPercent: 0, muted: false)),
                SavedMix(
                    name: "Line\nbreak\tand tab \u{1} \u{7F} \u{2028}",
                    snapshot: MixSnapshot(
                        channelGains: [55], channelPans: [10], channelMutes: [false],
                        channelSolos: [true], groupLevels: [5], volumePercent: 45,
                        maxLevelPercent: 65, muted: true)),
            ]
        ),
        (
            "largest-desk",
            [
                SavedMix(
                    name: "Full desk",
                    snapshot: MixSnapshot(
                        channelGains: (0..<32).map { ($0 * 37) % 101 },
                        channelPans: (0..<32).map { ($0 * 13) % 101 },
                        channelMutes: (0..<32).map { $0 % 3 == 0 },
                        channelSolos: (0..<32).map { $0 % 5 == 0 },
                        groupLevels: (0..<16).map { 100 - $0 * 6 }, volumePercent: 64,
                        maxLevelPercent: 72, muted: false))
            ]
        ),
    ]
}
