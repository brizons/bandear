#if DEBUG
    import BandEarCore
    import BandEarDiscovery
    import BandEarScreens
    import BandEarSession
    import SwiftUI

    /// Sample mixes for the Mix screen's previews, built with the shared core's `MixModel` from what
    /// a desk would announce, so they hold only states the app can reach. Each one is a model of its
    /// own, so a preview's edits stay in it.
    enum MixSamples {
        static let desk = DiscoveredDesk(
            name: "BandEar on studio-mac",
            endpoint: .bonjour(name: "BandEar on studio-mac", type: "_bandear._tcp", domain: "local."),
            sampleRate: 48_000)

        static let connected = ConnectionStatus(
            connection: .connected, sessionRunning: true, desk: desk.endpoint, mixHeldByDesk: true)

        static let reconnecting = ConnectionStatus(
            connection: .reconnecting, sessionRunning: true, desk: desk.endpoint, mixHeldByDesk: true)

        /// Eight channels: a named voice, a keys pair, an unnamed guitar, bass, two drums the
        /// engineer locked, and a hidden talkback; three groups; More of me lifting the voice; an
        /// engineer's limit of 85%.
        static var moreOfMeOn: MixModel { model(following: nil, moreOfMe: true) }

        /// The same mix while the engineer has it follow Ana's.
        static var following: MixModel { model(following: "Ana", moreOfMe: false) }

        /// A session whose desk has not announced its channels yet.
        static var waiting: MixModel { model(with: [.sessionStarted]) }

        private static func model(following: String?, moreOfMe: Bool) -> MixModel {
            let channels = [
                SourceChannel(name: "Lead vox", kind: .voice, isHidden: false, isLinkedToNext: false),
                SourceChannel(name: "Keys", kind: .keys, isHidden: false, isLinkedToNext: true),
                SourceChannel(name: "Keys R", kind: .keys, isHidden: false, isLinkedToNext: false),
                SourceChannel(name: "", kind: .guitar, isHidden: false, isLinkedToNext: false),
                SourceChannel(name: "Bass", kind: .bass, isHidden: false, isLinkedToNext: false),
                SourceChannel(name: "Kick", kind: .drums, isHidden: false, isLinkedToNext: false),
                SourceChannel(name: "Snare", kind: .drums, isHidden: false, isLinkedToNext: false),
                SourceChannel(name: "Talkback", kind: .other, isHidden: true, isLinkedToNext: false),
            ]
            let config = DeskConfiguration(
                sourceChannels: 8, sampleRate: 48_000,
                groups: [
                    ChannelGroup(id: "g1", name: "Vocals", channels: [0]),
                    ChannelGroup(id: "g2", name: "Band", channels: [1, 2, 3, 4]),
                    ChannelGroup(id: "g3", name: "Drums", channels: [5, 6]),
                ],
                channels: channels)
            let report = MixReport(
                sequence: nil, volumePercent: 80, maxLevelPercent: 90, muted: false,
                channelGains: [72, 60, 60, 45, 70, 55, 50, 100],
                pans: [50, 0, 100, 30, 50, 50, 50, 50],
                channelMutes: [false, false, false, true, false, false, false, false],
                channelSolos: [false, false, false, false, true, false, false, false],
                groupLevels: [90, 75, 60], engineerLimitPercent: 85,
                locked: [false, false, false, false, false, true, true, false],
                following: following)
            var events: [MixEvent] = [
                .sessionStarted, .configReceived(config), .mixPushed(report),
                .musicianChannelChosen(channel: 0),
            ]
            if moreOfMe { events.append(.moreOfMeToggled(on: true)) }
            return model(with: events)
        }

        private static func model(with events: [MixEvent]) -> MixModel {
            let model = MixModel(musicianChannel: nil, savedMixes: [])
            for event in events { _ = model.apply(event: event) }
            return model
        }
    }

    /// The Mix screen over a sample mix that takes its own edits through its model, so the controls
    /// move in a preview as they do in the app. The panel host stands in for the root's.
    struct MixPreviewHost: View {
        let model: MixModel
        var status = MixSamples.connected
        @State private var mix: MixModelState

        init(model: MixModel, status: ConnectionStatus = MixSamples.connected) {
            self.model = model
            self.status = status
            _mix = State(initialValue: model.state())
        }

        var body: some View {
            MixContent(
                musicianName: "Carlos", desk: MixSamples.desk, status: status, mix: mix,
                apply: { mix = model.apply(event: $0).state }
            )
            .modalPanelHost()
        }
    }

    /// One of the Mix screen's panels already open over the console, as a scribble strip or the
    /// master's level cap opens it, with edits going through its model.
    struct MixPanelPreviewHost: View {
        enum Panel {
            /// The panel of the strip known by this channel.
            case channel(Int64)
            case master
        }

        let model: MixModel
        let panel: Panel
        @State private var mix: MixModelState

        init(model: MixModel, panel: Panel) {
            self.model = model
            self.panel = panel
            _mix = State(initialValue: model.state())
        }

        var body: some View {
            let apply: MixApply = { mix = model.apply(event: $0).state }
            MixContent(
                musicianName: "Carlos", desk: MixSamples.desk, status: MixSamples.connected, mix: mix,
                apply: apply
            )
            .modalPanel(isPresented: .constant(true), doneTitle: ChannelSheetWords.done) {
                switch panel {
                case .channel(let channel):
                    if let strip = ChannelSheetWords.sheetStrip(in: mix.strips, open: channel) {
                        ChannelSheetTitle(strip: strip)
                    }
                case .master:
                    MasterPanelTitle()
                }
            } content: {
                switch panel {
                case .channel(let channel):
                    if let strip = ChannelSheetWords.sheetStrip(in: mix.strips, open: channel) {
                        ChannelSheet(strip: strip, moreOfMe: mix.moreOfMe, apply: apply)
                    }
                case .master:
                    MasterPanel(mix: mix, apply: apply)
                }
            }
        }
    }

    // The console in both orientations: the drums locked, the unnamed guitar (CH 4) muted, the
    // bass soloed, the keys a stereo pair, the lead vox the musician's own and lifted, and the
    // engineer's limit of 85% marked on the master.

    @available(iOS 17.0, *)
    #Preview("Mix · portrait", traits: .fixedLayout(width: 390, height: 844)) {
        MixPreviewHost(model: MixSamples.moreOfMeOn)
    }

    @available(iOS 17.0, *)
    #Preview("Mix · landscape", traits: .fixedLayout(width: 844, height: 390)) {
        MixPreviewHost(model: MixSamples.moreOfMeOn)
    }

    @available(iOS 17.0, *)
    #Preview("Mix · following Ana's mix · portrait", traits: .fixedLayout(width: 390, height: 844)) {
        MixPreviewHost(model: MixSamples.following)
    }

    @available(iOS 17.0, *)
    #Preview("Mix · following Ana's mix · landscape", traits: .fixedLayout(width: 844, height: 390)) {
        MixPreviewHost(model: MixSamples.following)
    }

    #Preview("Mix · reconnecting") {
        MixPreviewHost(model: MixSamples.moreOfMeOn, status: MixSamples.reconnecting)
    }

    #Preview("Mix · waiting for channels") {
        MixPreviewHost(model: MixSamples.waiting)
    }

    @available(iOS 17.0, *)
    #Preview("Channel panel · portrait", traits: .fixedLayout(width: 390, height: 844)) {
        MixPanelPreviewHost(model: MixSamples.moreOfMeOn, panel: .channel(1))
    }

    @available(iOS 17.0, *)
    #Preview("Channel panel · landscape", traits: .fixedLayout(width: 844, height: 390)) {
        MixPanelPreviewHost(model: MixSamples.moreOfMeOn, panel: .channel(1))
    }

    @available(iOS 17.0, *)
    #Preview("Channel panel · locked", traits: .fixedLayout(width: 390, height: 844)) {
        MixPanelPreviewHost(model: MixSamples.moreOfMeOn, panel: .channel(5))
    }

    @available(iOS 17.0, *)
    #Preview("Master panel · portrait", traits: .fixedLayout(width: 390, height: 844)) {
        MixPanelPreviewHost(model: MixSamples.moreOfMeOn, panel: .master)
    }

    @available(iOS 17.0, *)
    #Preview("Master panel · landscape", traits: .fixedLayout(width: 844, height: 390)) {
        MixPanelPreviewHost(model: MixSamples.moreOfMeOn, panel: .master)
    }
#endif
