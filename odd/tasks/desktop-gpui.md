# Desktop on GPUI

Objective: replace the desktop app's Tauri shell and web UI (`apps/desktop`) with a native GPUI app at
feature parity. Branch `claude/desktop-gpui-c7705d`; delivery is work-unit commits on this branch only (no
push, PR or merge). Test runner: `cargo test` on Rust 1.100.0 (pinned).

## Specs

- S1 "Replace the BandEar desktop app's Tauri + web UI (`apps/desktop`) with a native GPUI app (GPUI =
  Zed's GPU-rendered Rust UI framework), keeping feature parity."
- S2 "Inventory every screen, control, state and command of the current UI; that inventory is the spec
  (feature parity)." The inventory is the Parity inventory below (areas A-E); each area's rules (C#, R#,
  M#, CH#, P#, E#, N#), copy and tests are requirements.
- S3 "The Tauri commands in `apps/desktop/src-tauri/src/commands/` (for example `soundcheck.rs`) bridge
  the web UI to the engine; reuse their logic instead of rewriting it."
- S4 "Build the GPUI app, keep app-state/view-model logic testable without a window, and test it."
- S5 "`apps/server` has default feature `asio` (Windows ASIO), so the desktop app must keep working on
  Windows."
- S6 "Remove Tauri and `apps/desktop/ui` only after parity is verified; update the README and the
  workspace members."
- S7 "Done = the GPUI desktop app builds and runs on macOS with the same screens and behavior, the
  workspace `cargo test` passes, Tauri and the web UI are gone, docs updated, all committed on this
  branch. Do not push, open a PR or merge; that is the user's call."
- S8 Parallel work: another session edits `apps/ios`, `apps/android`, a new shared crate, possibly
  `apps/server/src/protocol.rs`, and the root `Cargo.toml` members and `Cargo.lock`. "Do not edit those
  paths, except the root `Cargo.toml` member list and `Cargo.lock` as your change requires."
- S9 GPUI source (user decision, L2): "A: gpui-pre + Rust 1.100 (Recommended)" = `gpui-pre =0.3.12` +
  `gpui-component =0.9.1`, Rust >= 1.100 for the whole workspace. Toolchain pin (L3): "put
  `rust-toolchain.toml` at the repo root in its own small commit, with `channel = "1.100.0"` ... and
  `targets = ["aarch64-apple-ios", "aarch64-apple-ios-sim", "aarch64-linux-android",
  "armv7-linux-androideabi", "x86_64-linux-android"]`".
- S10 Machine limits (L3): "keep one GPUI build at a time, delete probe target dirs as soon as you're done
  with them, and stop and report instead of retrying if a build fails for space"; "surface any further
  toolchain or SDK installs (for example the Metal Toolchain) to the user first; crates fetched by cargo
  are fine."
- S11 Copy, user-visible strings, code, comments and docs are in English (repository convention); the
  GPUI app keeps the web UI's copy verbatim (U+2212 minus, U+2026 ellipsis, `·` separators).

### Design decisions

- D1 Crates. `apps/desktop/core` = `bandear-desktop-core`: the backend moved from `src-tauri/src`
  (`backups`, `commands`, `dto`, `events` without Tauri, `logfile`, `migration`, `session`, `sessions`)
  with its tests, plus the app folders (same paths as Tauri 2.12) and the shared startup and exit;
  UI-agnostic (S3). `apps/desktop/console` = `bandear-desktop-console`: the console's core state
  (`console-state.js`), the engine lifecycle part of `console-app.js`, send lanes, the typed commands
  and answers, values, formats and routes; pure Rust, no GPUI, tested with plain `#[test]` (S4).
  `apps/desktop/kit` = `bandear-desktop-kit`: GPUI theme, fonts, icons and the shared atoms.
  `apps/desktop/screens/<name>` = `bandear-desktop-<name>` for `mix`, `record`, `channels`,
  `processing`, `effects`, `network`: each holds its screen's model (pure module, plain tests) and its
  GPUI view, and depends only on `console` and `kit`. `apps/desktop/app` = `bandear-desktop`: the
  window, shell (sidebar, chips, routing), the adapter that runs commands and pumps events, and the
  composition of the screens. While Tauri still exists its crate is renamed `bandear-desktop-tauri`
  and uses the core crate.
- D9 Parallel units: one crate per screen in this worktree, so writers edit disjoint crates, one
  writer's unfinished code never breaks another's build, and cargo's lock on the shared `target` keeps
  one build at a time (S10). Writers do not edit `console`, `kit`, `app` or the root `Cargo.toml`;
  a missing shared piece is reported to the parent, who adds it.
- D2 The console model is a state machine: an intent or a reply changes the state and returns effects
  (commands to run, timers); the app executes them (commands on GPUI's background executor through a
  `Desk` port over `core::commands`) and feeds the answers back. Tests drive it with scripted answers,
  so the JS suites port as deterministic tests.
- D3 Events: the core pump's `PumpSink` sends into a channel read on GPUI's foreground; meters (33 ms)
  reach the views without going through the model's undo/echo logic, as in the web UI (C-rules).
- D4 Formatting keeps JavaScript's `Math.round` (half toward +infinity), never `f64::round`, so
  readouts match (`formatDb(-6.5)` prints "−6").
- D5 macOS builds use `gpui-pre-platform` features `font-kit` (text) and `runtime_shaders` (no Metal
  Toolchain needed); fonts IBM Plex Sans and JetBrains Mono (variable TTFs with their OFL texts) and the
  brand mark ship as embedded assets.
- D6 Routing has no URL: the six sections are app state; `processing/<n>` becomes the selected channel.
  The native window title stays "BandEar Server — <device>" (C39), 1180 x 780, min 820 x 600.
- D7 Assumption: the browser-only tooling of the web UI is not ported (`?fake` simulated engine,
  `fake-bridge.js`, `ui/dev` gallery and server): it exists because a web page cannot run the engine.
  The engine's Demo source covers running without an interface.
- D8 Accessibility: views set GPUI AccessKit roles, labels, values and toggled states where the web UI
  set ARIA; keyboard behaviour (slider steps, EQ map, chart keys, Escape) is ported. Screen-reader
  output is checked where the machine allows (L4: not verifiable with this session's tools so far).

### Parity inventory

Drafted by read-only explorers from the web UI and Tauri source (2026-11-12); paths are relative to
`apps/desktop/`. Where an area says "unsure", the JS source is the reference.

#### Inventory A: shell and core client state (BandEar desktop to GPUI)

Paths are relative to `apps/desktop/`. `ui/` = `ui/app/` unless shown. JS is plain ES modules; tests are `node --test` (`ui/package.json`).

---

##### 1. Shell

###### 1.1 Window settings (`src-tauri/tauri.conf.json:2-25`, `src-tauri/capabilities/default.json`)

| Setting | Value |
|---|---|
| productName / version / identifier | `BandEar` / `0.3.0` / `com.bandear.desktop` |
| window label | `main` |
| initial window title | `BandEar Server` (replaced at runtime, see C35) |
| size | 1180 x 780 |
| min size | 820 x 600 |
| resizable / fullscreen | true / false |
| `withGlobalTauri` | true (JS reaches `window.__TAURI__`) |
| CSP | null |
| capabilities | `core:default`, `core:window:allow-set-title` |
| bundle icons | `icons/32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico`; macOS `signingIdentity: "-"` |
| HTML `<title>` | `BandEar` (`ui/index.html:6`), favicon `app/brand/bandear-mark.svg` (`index.html:7`) |

On app exit (`RunEvent::Exit`) the backend flushes the local mix, channel processing and a mix backup, then the log (`src-tauri/src/lib.rs:818-828`). Setup opens the log file, the session store in the app config dir, the recordings folder default and runs the MixLink to BandEar folder migration (`lib.rs:680-736`).

###### 1.2 Page structure (`ui/index.html:17-29`)

```
<a class="skip-link" href="#main">Skip to content</a>
<div class="shell">                       grid: [sidebar 212px][body 1fr]
  <nav class="sidebar" aria-label="Sections">   brand row + nav list (+ "Simulated engine" note)
  <div class="shell-body">
    <div class="shell-status" hidden>    chips: demo (prepended at runtime), rec, soundcheck
    <main id="main" class="main" tabindex="-1">   current screen
```

###### 1.3 Sidebar and navigation

Sections, in order (`ui/app/lib/router.js:6-23`); label = window-title section name:

| # | id | hash | label (verbatim) |
|---|---|---|---|
| 1 | mix | `#/mix` | Mix |
| 2 | record | `#/record` | Record |
| 3 | channels | `#/channels` | Channels and groups |
| 4 | processing | `#/processing` (or `#/processing/<1-32>`) | Processing |
| 5 | effects | `#/effects` | Effects |
| 6 | network | `#/network` | Network and system |

- Brand row: mark image (`alt=""`, decorative) + text `BandEar` (`shell.js:54-56`). Brand 20px bold, gap 10px, padding `6px 10px 18px`; mark 32x32 with margins `-6px -6px -6px -9px` to trim the empty canvas (`shell.css:45-65`).
- Each link: `<a class="nav-link" href=<hash> data-route=<id> title=<label>>` + 18px SVG icon + `<span class="nav-label">` (`shell.js:58-64`). `title` is the tooltip in the icon rail.
- Active link: `aria-current="page"`, background `--ml-surface-raised`, text `--ml-text`, weight 600, icon colour `--ml-accent` (`shell.css:95-109`). Hover: background `--ml-surface`, text `--ml-text` (`shell.css:90-93`). Idle: text `--ml-text-secondary`, 14px weight 500, min-height 40px, padding `6px 12px`, radius 10px, gap 10px (`shell.css:76-88`).
- Sidebar: sticky, full height, scrolls vertically, padding `20px 12px`, right border 1px `--ml-border-subtle`, background `--ml-sidebar`, gap 4px (`shell.css:32-43`).
- `?fake` mode only: a note `Simulated engine` appended at the sidebar bottom (`margin-top:auto`, 1px dashed `--ml-warn-border`, radius 10px, 12px text) (`shell.js:193-198`, `shell.css:111-118`).
- Responsive (`shell.css:171-237`): width <= 1099px: sidebar becomes a 64px icon rail, links 44px wide centred, brand name and labels visually hidden (still read and used as tooltip), note hidden. Width <= 639px: sidebar becomes a horizontal top row (no right border, bottom border), nav list is a row, main padding 12px. (Window min width 820 means only the icon-rail breakpoint is reachable in the app.)

Nav icon SVGs (24x24 viewBox, `fill=none stroke=currentColor stroke-width=2` round caps/joins) (`shell.js:28-35`, `47`):

```
mix:        M6 4v16M12 4v16M18 4v16 | M4 14h4M10 8h4M16 12h4
record:     circle(12,12,r8) + circle(12,12,r3.5, fill=currentColor)
channels:   M4 6h16M4 12h16M4 18h10
processing: M3 15c3 0 3-6 6-6s3 6 6 6 3-6 6-6
effects:    circle r3 + circle r6.5 opacity .6 + circle r10 opacity .3 (all at 12,12)
network:    M5 12.5a10 10 0 0 1 14 0M8.5 16a5 5 0 0 1 7 0 + circle(12,19.5,r1)
```

Brand mark (`ui/app/brand/bandear-mark.svg`): viewBox 1024, `stroke="#FFBF43" stroke-width="44"`, round caps/joins, no fill, six paths:
`M520 224 H344 A48 48 0 0 0 296 272 V738 A70 70 0 0 0 366 808 H556 A186 186 0 0 0 692 528`; `M736 446 Q720 486 678 514`; `M314 522 C354 482 404 470 476 470 H606`; `M402 708 H562 A71 71 0 0 0 562 566 H474 A72 72 0 0 0 402 638`; `M586 302 A92 92 0 0 1 662 386`; `M584 224 A162 162 0 0 1 738 386`.

###### 1.4 Routing and keyboard

- R1 `resolveRoute(hash)`: strip leading `#/` or `#`, trailing slashes, lowercase, split on `/`; first segment picks the section; extra segments allowed only for `processing`, else default Mix; unknown or empty opens Mix (`router.js:31-50`).
- R2 `routeChannel(hash)`: `processing/<digits>` with exactly one extra segment, 1-32, else null (`router.js:28,53-60`).
- R3 `canonicalHash`: the section's hash, or `#/processing/<n>` when it names a valid channel; a non-canonical hash is rewritten with `history.replaceState` (no new history entry) (`router.js:68-72`, `shell.js:90-94`).
- R4 Window document title per section: `"<title> · BandEar"`, e.g. `Mix · BandEar` (`router.js:75-77`, `shell.js:102`). Separate from the native window title (C39). Unsure whether Tauri mirrors `document.title` into the native title bar; the code sets the native one explicitly.
- R5 On every `hashchange`: update `aria-current`, set title, call the previous screen's cleanup, empty `main`, mount the new screen (`shell.js:88-113`, `121`).
- R6 Focus: after a navigation (not the first paint) focus moves to the new screen's `h1` (or `main` if none) so a screen reader starts there; headings have `tabindex=-1` and no focus ring (`shell.js:107-112`, `shell.css:166-169`).
- R7 Skip link `Skip to content`: off-screen (`top:-100px`) until focused, then at `top:8px`; click is `preventDefault` (no hash change) and focuses `main` (`shell.js:116-119`, `shell.css:11-24`).
- Keyboard: no app-level shortcuts exist in this area (grep of `keydown` in shell/router/top-bar/rec-chip finds none). Navigation is native Tab order (skip link, nav links, chips, screen) and Enter on links. Global focus ring: `:focus-visible` 2px solid `--ml-focus` offset 2px; plain `:focus` has no outline (`tokens.css:198-205`).

###### 1.5 Top bar (`ui/app/components/top-bar.js`; on the Mix screen only, not shell-wide)

Data `{ phase, device, format, attention, attentionLabel, action }` from `topBarView` (`lib/mix-view.js:38-53`). Left to right (`top-bar.js:100-109`), flex-wrap, gap 10px:

1. Status pill (`components/status-pill.js`), words by phase (`status-pill.js:10-17`): idle `Not running`, starting `Starting`, running `Running`, stopping `Stopping`, stopped `Stopped`, error `Error`. Host has `role="status"` (polite live region) (`:97`). Optional `label` attribute overrides the word. Look (`:19-79`): pill min-height 32px, padding `0 12px`, radius 16px, 12px bold, nowrap; dot 8px. running: ok bg/border, green filled dot; starting/stopping: warn bg/border, accent dot pulsing (`opacity 1 -> 0.3`, 1s ease-in-out infinite alternate); error: danger bg/border, red dot with radius 1px (square); idle/stopped/no state: raised surface, `--ml-border`, hollow ring dot (2px border, muted).
2. Readout `<device> · <format>` (12px secondary; device in `--ml-text`; both spans mono), hidden when `device === null` (`top-bar.js:23-32,136-138`). Format = `formatCaptureFormat`, e.g. `48 kHz · 24-bit · 256`.
3. Spacer (flex-grow).
4. Attention link to `#/network`, hidden when null: warning-triangle icon (14px, stroke 2.4, accent) + `1 phone needs attention` / `N phones need attention` (`mix-view.js:40-44`, `top-bar.js:104-107`). Warn bg/border, radius 16px, min-height 32px; hover border accent.
5. Engine button (`top-bar.js:141-147`, `mix-view.js:25-32`): action per phase idle `start`, error `start`, running `stop`, stopped `stop`, starting `null`, stopping `null`. Text `Start engine` / `Stop engine`; with no action: `Starting…` / `Stopping…` (busy words, `top-bar.js:11`) and disabled. Look: min-height 40px, padding `0 16px`, radius 10px, 13px bold; start = accent fill, accent border, `--ml-text-on-bright` text; stop = danger bg, danger border, `--ml-danger-text`. Click emits `engine-start` / `engine-stop`, which the Mix screen maps to `app.start()` / `app.stop()` (`screens/mix-screen.js:281-282`).

###### 1.6 Chips above every screen (`ui/app/components/rec-chip.js`, `shell.js:166-179`, `lib/record-controller.js:374-394`)

The `.shell-status` strip is sticky at top (z 5), right-aligned, wraps, gap 8px, padding `8px 22px 0`, ground background; hidden when all three chips are null. Order: demo, rec, soundcheck. Each chip is one link (`href` default `#/record`), whose accessible name and tooltip are `label`; the inner word/time/detail are `aria-hidden` (`rec-chip.js:120-157`). Chip look: min-height 32px, padding `0 12px`, radius 16px, 12px bold, gap 8px; time in mono; detail weight 500 secondary.

| kind | word | time | detail | label (verbatim template) | look |
|---|---|---|---|---|---|
| demo (`lib/demo-source.js:47-61`) | `Demo source` | `` | `— no interface in use` | `Demo source — no interface in use: the engine plays a generated band. Open Network and system to choose an interface` (href `#/network`) | blue border, raised bg, blue square-ish dot (radius 2px) |
| recording | `REC` | duration | `N track(s)` | `Recording, <spoken>, <detail>. Open Record` | danger bg/border, red dot pulsing 1.2s |
| stopping | `Finishing` | duration | tracks | `Finishing the take's files after <spoken>. Open Record` | warn bg/border, accent dot |
| failed | `REC stopped` | duration | tracks | `The recording stopped: <reason>. Open Record` | danger, square dot (radius 2px) |
| soundcheck | `SOUNDCHECK` | position | take name | `Soundcheck: "<name>" plays in place of the live inputs, at <at>. Open Record` | accent-bg, warn border, accent play-triangle (10x12) pulsing 1.6s |
| soundcheck-paused | `SOUNDCHECK` | position | `<name> · paused` | `Soundcheck: "<name>" is paused in place of the live inputs, at <at>. Open Record` | as soundcheck, no pulse |
| soundcheck-failed | `SOUNDCHECK stopped` | position | name | `The soundcheck stopped playing: <reason>. Open Record` | danger, red triangle |

Pulse keyframe: 50% opacity 0.35 (`rec-chip.js:104-108`). Demo is shown while `status && !status.stopped && status.demo` (`demo-source.js:37-40`). REC and SOUNDCHECK views come from `record-view.js` / `soundcheck-view.js` (other inventory areas).

###### 1.7 Engine start/stop flow and states

Phases: `idle`, `starting`, `running`, `stopping`, `stopped`, `error` (`lib/console-state.js:108`). Connection: `connecting`, `ready`, `failed` (`console-state.js:145,349-355`; section 4.1).

```
window opens -> connect(): connecting
  listen to status/mix-changed/meters (all or none) --fail--> failed "Cannot hear the engine: <e>"
  audio_settings (until read once)
  engine_status --fail--> failed "<e>"
  -> ready; read presets, processing, strip presets, sessions, session notice, mix backups, recording settings (each only if unknown)
  answer notRunning + autoStart:
      choices unread -> failed "<choiceUnread>"
      else start (starting -> running, or error)
  answer running -> status taken (running/stopped), refresh disk estimate
Start / Try again -> startChoice(): starting -> api.start -> readStatus (running)
                     failure -> error "<e>"; status unreadable after start -> 3 retries x 500 ms, then failed
Stop -> stopping -> api.stop -> idle (run ended); refused -> phase back to last status + commandError
```

Mix screen notice per state (`screens/mix-screen.js:32-50`, chosen by `mixNotice`, `lib/mix-view.js:245-265`, in this priority):

| condition | title | text | action button |
|---|---|---|---|
| connection failed | `The console cannot reach the engine` | the error message | `Try again` (calls `reconnect`) |
| connecting and phase idle | `Connecting to the engine…` | | |
| idle or stopped | `The engine is stopped` | `Start it to send each musician their mix.` | `Start engine` |
| starting | `Starting the engine…` | `Opening the audio input.` | |
| stopping | `Stopping the engine…` | `Closing the audio input.` | |
| error | `The engine is not running` | the error | `Try again` (calls `start`) |
| no musicians (and local mix not open) | `No phones yet` | `Musicians appear here when they open BandEar on their phone and join this computer. You can also add a phone by address in ` + link `Network and system` (`#/network`) | |

Non-action button on any notice calls `app.start()`; only `disconnected` calls `app.reconnect()` (`mix-screen.js:312-319`).

###### 1.8 Startup and first paint

- `shell.js` runs on module load: renders the sidebar, creates the console app once per window (`createConsoleApp` starts `connect()` immediately, `console-app.js:1596`), mounts chips, then routes (`shell.js:166-199`). The first paint does not move focus.
- `engine_status` is asked once for the first paint; afterwards the `status` event drives updates (`src-tauri/src/events.rs:36-37`, `console-app.js:563-609`).
- `engine_status` answers `{ state: "notRunning", run }` (not an error) or the full `EngineStatusDto` with `state: "running"` (`src-tauri/src/dto.rs:1147-1155`). The `status` event carries the DTO without `state`.
- Plain browser without Tauri globals: `createTauriBridge` throws `the Tauri globals are not available: open the console in the BandEar app` (`bridge.js:36-38`) and every screen shows heading `No engine to talk to` with text `This console runs inside the BandEar app. To preview it in a browser with a simulated engine, open index.html?fake.` (link `?fake`) (`shell.js:132-149`). `?fake` uses `fake-bridge.js` with `startDemo()` (`shell.js:153-158`). Native port: N/A except as an optional dev flag.
- Placeholder headings (`shell.js:37-44,67-77`) are used only for a route with no screen; all six routes have screens, so they never show in the app.

###### 1.9 Error and notice copy owned by the core (verbatim)

| source | text |
|---|---|
| `console-app.js:318` | `The engine started, but its status could not be read: <e>` |
| `console-app.js:554` | `Cannot hear the engine: <e>` |
| `console-app.js:364-370` | `The saved audio input could not be read, so nothing was started: <e>. Choose an input in Network and system to start without it.` (trailing `.` stripped when shown as a command error, `:484`) |
| `console-app.js:387-390` | `The audio input runs but was not saved for the next start: <e>.` (audioNotice `unsaved`) |
| `console-app.js:450-453` | `<device> was not found, and the system's default input did not start either: <e>.` |
| `engine-config.js:25-26` | `ASIO is chosen but no ASIO driver is picked: pick one under Network and system.` |
| `engine-config.js:117-129` | fallback notice: `The saved input <d> was not found, so the engine started on the system's default input. The saved input is kept and tried again at the next start.` / ASIO: `The saved ASIO driver <d> was not found, so the engine started on the system's own driver and its default input, never on another ASIO driver. The saved driver is kept and tried again at the next start.` |
| `recording.js:37-39` | `Stop the recording before switching the input device or its driver: a take never mixes two devices.` |
| `soundcheck.js:36-46` | `Go back to the live inputs before switching the input device or its driver: a soundcheck plays a take in place of the inputs.`; `Go back to the live inputs before recording: a take never records a soundcheck's playback.`; `Stop the recording before the soundcheck: a take records the live inputs.` |
| `mix-view.js:267-288` | command error: `The engine could not be stopped: <m>.` / `The engine could not be started: <m>.` / `The input device could not be switched: <m>.` / other `The engine refused to <action>: <m>.` |
| `mix-view.js:290-313` | `<label> was removed; showing <label>'s mix.`; `A change to <name>'s mix was not applied: <m>.` (local: `A change to the local mix was not applied: <m>.`) |
| `console-app.js:848` | `The groups were not changed: no engine is running, so this desk does not know them yet; start the engine first.` |

---

##### 2. Design tokens (`ui/app/tokens.css:26-128`)

`color-scheme: dark`. Single (dark) theme only.

| Token | Value | | Token | Value |
|---|---|---|---|---|
| `--ml-sidebar` | `#0b0f14` | | `--ml-track` | `#262f3a` |
| `--ml-ground` | `#0d1117` | | `--ml-fader-fill` | `#5b6b7c` |
| `--ml-surface-sunken` | `#12171e` | | `--ml-fader-fill-muted` | `#3a4452` |
| `--ml-surface` | `#161b22` | | `--ml-thumb` | `#e6edf3` |
| `--ml-surface-raised` | `#1f2630` | | `--ml-thumb-shadow` | `0 2px 6px rgb(0 0 0 / 50%)` |
| `--ml-border-subtle` | `#1f2630` | | `--ml-meter-ground` | `#0b0f14` |
| `--ml-border` | `#2b3440` | | `--ml-meter-safe` | `var(--ml-green)` |
| `--ml-text` | `#e6edf3` | | `--ml-meter-hot` | `var(--ml-accent)` |
| `--ml-text-secondary` | `#c5ced8` | | `--ml-meter-clip` | `var(--ml-red)` |
| `--ml-text-muted` | `#9ba8b5` | | `--ml-group-teal-bg` / `-text` | `#123a3a` / `#7fe0d0` |
| `--ml-text-on-bright` | `#0d1117` | | `--ml-group-blue-bg` / `-text` | `#1c2a44` / `#9cc3ff` |
| `--ml-accent` | `#f5a524` | | `--ml-group-violet-bg` / `-text` | `#2a2346` / `#c3b5ff` |
| `--ml-accent-hover` | `#ffc35c` | | `--ml-group-orange-bg` / `-text` | `#3a2412` / `#ffc58a` |
| `--ml-green` | `#3fb950` | | `--ml-group-none-bg` / `-text` | `#262f3a` / `#c5ced8` |
| `--ml-red` | `#f85149` | | `--ml-marker-bg` | `#10283a` |
| `--ml-blue` | `#4cc2ff` | | `--ml-marker-text` | `#9fdcff` |
| `--ml-focus` | `var(--ml-blue)` | | `--ml-accent-bg` | `#1a1610` |
| `--ml-ok-bg` / `--ml-ok-border` | `#10261a` / `#1f6f3a` | | `--ml-danger-bg` / `-border` | `#2a1414` / `#6b2a2a` |
| `--ml-warn-bg` / `--ml-warn-border` | `#2a1f0b` / `#6b4a12` | | `--ml-danger-text` | `#ffb4ad` |

Type: `--ml-font-sans` `"IBM Plex Sans", system-ui, "Segoe UI", sans-serif`; `--ml-font-mono` `"JetBrains Mono", ui-monospace, Consolas, monospace`. Sizes: `--ml-text-2xs` 10px, `-xs` 11px, `-sm` 12px, `-md` 13px, `-base` 14px, `-lg` 15px, `-xl` 20px, `-2xl` 26px. Weights: regular 400, medium 500, bold 600. `--ml-tracking-caps` 0.1em.

Spacing (4px grid): `--ml-space-1` 4px, `-2` 8px, `-3` 12px, `-4` 16px, `-5` 20px, `-6` 24px, `-8` 32px.
Radii: `--ml-radius-xs` 4px, `-sm` 6px, `-md` 10px, `-lg` 12px, `-xl` 16px, `-pill` 999px.
Hit targets: `--ml-target` 32px (every control), `--ml-target-primary` 40px (primary). `--ml-sidebar-width` 212px.

Fonts (bundled, `ui/fonts/`): `ibm_plex_sans_variable.ttf` (weight 100-700, stretch 75-100%) and `jetbrains_mono_variable.ttf` (weight 100-800), both `font-display: swap`, OFL licences beside them (`tokens.css:9-24`). Sans for all text; mono for numbers: top-bar device and format spans, chip times, strip readouts (`strip-styles.js:37-42`), `code/kbd/samp/.ml-mono` (`tokens.css:191-196`).

Base element styles (`tokens.css:136-218`): body 14px, line-height 1.4, background ground, text `--ml-text`, antialiased; h1-h3 weight 600 line-height 1.25, h1 26px, h2 15px; links accent, hover accent-hover; `.ml-visually-hidden` sr-only. Reduced motion: animations/transitions effectively off (`tokens.css:220-228`, components `support.js:28-35`).

Literal (non-token) sizes in this area: main padding `14px 22px 22px`, main gap 12px (`shell.css:143-149`); screen header flex-wrap baseline gap 12px, its `p` muted (`shell.css:155-164`); shared strip look: gap 7px, padding `10px 6px 8px`, 1px `--ml-border`, radius lg, surface bg; caption 10px bold caps-tracking muted; levels area min-height 200px gap 6px; buttons gap 4px (`components/strip-styles.js:6-47`).

Kind icons (`components/kind-icons.js`, 24 viewBox, stroke 1.8, default 16px; stereo 14px; unknown kind draws `other`): voice, guitar, bass, keys, drums, percussion, horns, strings, playback, click, other, plus stereo (two r5.5 rings at x 8.5 and 15.5). Path data at `kind-icons.js:4-18`.

---

##### 3. Command catalog (`src-tauri/src/lib.rs:39-675`, registered `:737-805`; 67 commands)

JS calls go `screen/lib -> app.<method>` (console-app) `-> api.<method>` (bridge.js) `-> invoke(name, args)`. JS arg names are camelCase; Rust params snake_case. All return `Result<T, String>`; the error string is the shown reason.

| command | args | returns | UI caller (console-app fn <- module) |
|---|---|---|---|
| start_engine | config: StartRequest `{deviceFilter, targets, controlPort, groups, host, bufferFrames}` | StartSummary `{run, deviceName, captureFormat}` | start <- mix-screen (Start/Try again), auto-start, network-screen (applyAudio, switchDevice restart) |
| engine_status | - | EngineStateDto (`notRunning {run}` or running status) | readStatus, after most commands |
| stop_engine | - | () | stop <- mix-screen |
| list_input_devices | host?: "auto"/"asio"/"wasapi" | `[{name, channels}]` | loadDevices <- network-screen; fallback check in start |
| switch_device | device | StartSummary | switchDevice <- network-screen (live path) |
| add_target / remove_target | address | String | addTarget/removeTarget <- network-screen |
| set_musician_name | address, name | () | rename <- network-screen |
| set_mix | address, mix: MixUpdateRequest | MixDto | edit <- mix-screen (coalesced) |
| set_local_mix | mix | MixDto | edit(LOCAL_MIX) <- mix-screen |
| output_devices | - | `[{name, channels}]` | loadOutputs <- network-screen |
| set_local_output | output?: `{device, firstChannel}` | `{output, localOutput, applied, saved, saveError}` | setLocalOutput <- lib/local-mix.js |
| set_groups | groups `[{id?, name, channels}]` | `{groups, applied, saved, saveError}` | editGroups <- lib/channels-view.js |
| set_channels | channels `[{name, kind, hidden, link}]` | `{channels, applied, saved, saveError}` | editChannels <- lib/channels-view.js |
| channel_table | - | `[SessionChannel]` | base of editChannels |
| set_engineer_controls | address, clientId?, controls `{limitPercent?, lockedChannels?}` | EngineerControlsSetDto | edit (engineer fields) <- mix-screen |
| audio_settings | - | `{host, device, bufferFrames, asioDriver}` or null | readChoice (connect, start) |
| set_audio_settings | settings | SessionAudio | keepChoice |
| presets | - | `[SessionPreset]` | loadPresets <- connect, mix-screen |
| save_preset / rename_preset / delete_preset | address,name / id,name / id | `{preset, presets}` | lib/mix-tools.js |
| apply_preset | id, musicians[] | `{mixes}` | lib/mix-tools.js |
| copy_mix | from, to[] | `{mixes}` | lib/mix-tools.js |
| set_mirror | address, clientId?, leader? `{address, clientId, name}` | MirrorSetDto | lib/mix-tools.js |
| session_notice | - | String or null | readSessionNotice (connect) |
| sessions | - | `[{id, name, savedAt, mixes, problem}]` | loadSessions <- lib/sessions.js |
| save_session / rename_session / delete_session | name / id,name / id | SessionSavedDto / SnapshotSummary / () | lib/sessions.js |
| load_session | id | SessionLoadedDto | loadSession <- lib/sessions.js |
| recording_settings | - | RecordingSettingsDto | loadRecordingSettings <- connect, record-screen |
| set_recording_settings | settings | RecordingSettingsDto | setRecordingSettings <- lib/record-controller.js |
| start_recording | takeName, utcOffsetMinutes? | RecordingStartedDto | startRecording <- record-screen |
| stop_recording | - | `{finishing, take, message}` | stopRecording <- lib/record-controller.js |
| add_marker | name | `{frame, seconds, name}` | addMarker <- record-screen |
| record_estimate | - | `{folder, bytesPerSecond, freeBytes, minutesLeft, problem}` | refreshEstimate <- connect, start, record-screen, record-controller |
| list_takes | - | `{folder, takes, unreadable, missing}` | refreshTakes <- record-screen, record-controller |
| open_take_folder | folder | () | openTakeFolder <- record-screen |
| start_soundcheck | takeFolder, startFrame?, looping? | SoundcheckDto | startSoundcheck <- record-controller |
| pause / resume / stop_soundcheck | - | SoundcheckDto | record-controller |
| seek_soundcheck | frame | SoundcheckDto | seekSoundcheck <- record-controller |
| set_soundcheck_loop | looping | SoundcheckDto | record-controller |
| log_location | - | `{directory, file, failedLines}` | **no UI caller** (not in bridge.js) |
| list_mix_backups | - | `[{stamp, savedAt, musicians, names, problem}]` | loadMixBackups <- connect, lib/sessions.js, network-screen |
| restore_mix_backup | stamp | MixRestoredDto | restoreMixBackup <- lib/sessions.js |
| link_history | musicianKey, sinceSeq? | LinkHistoryDto | linkHistory <- network-screen |
| set_musician_buffer | musicianKey, mode, ms? | BufferSetDto | network-screen |
| set_musician_redundancy | musicianKey, on, delayBlocks? | RedundancySetDto | network-screen |
| set_musician_limiter | musicianKey, settings `{enabled, ceilingDb, releaseMs}` | LimiterSetDto | editMixLimiter <- mix-screen |
| set_local_limiter | settings | LimiterSetDto | editMixLimiter(LOCAL_MIX) <- mix-screen |
| set_channel_send | channel, effect, level | ProcessingSetDto | editChannelSend <- processing-screen |
| set_effect | effect, settings (JSON) | EffectSetDto | editEffect <- effects-screen |
| set_musician_fx_return / set_local_fx_return | (musicianKey,) effect, level | FxReturnSetDto | editMixReturn <- mix-screen |
| processing | - | `[{channel, settings, active}]` | loadProcessing <- connect, lib/processing-editor.js |
| channel_processing | channel | `{channel, settings, active}` | **no UI caller** (wrapped in bridge.js:352, unused) |
| set_channel_processing | channel, settings | `{outcome, channels, message}` | editProcessing <- processing-editor |
| reset_channel_processing | channel | ProcessingSetDto | processing-editor |
| copy_channel_processing | from, to[] | ProcessingSetDto | processing-editor |
| strip_presets | - | `[{name, settings, factory}]` | loadStripPresets <- connect, processing-screen |
| save_strip_preset / delete_strip_preset | name(, settings) | `{outcome, preset, presets, message}` | processing-editor |
| apply_strip_preset | name, channel | ProcessingSetDto | processing-editor |
| eq_response | settings (eq), points? (128 default, 2-1024) | `[{freqHz, db}]` | loadEqResponse <- processing-editor |

###### Events (`src-tauri/src/events.rs`)

One pump thread per engine run (`events.rs:190-290`), every payload carries `run`:

| event | rate | payload |
|---|---|---|
| `meters` | every 33 ms (~30/s) (`:64`) | `{run, channels:[u8], processed:[u8], gateReductionDb:[f32], compressorReductionDb:[f32], musicians:{<address>:{left,right,limiterReductionDb}}, local:{left,right,limiterReductionDb}}` (`dto.rs:857-890`) |
| `status` | every 250 ms (~4/s) (`:67`), plus at once on GroupsChanged, ChannelsChanged, SoundcheckChanged (`:104-109`) | EngineStatusDto (`dto.rs:1158-1210`): run, deviceName, captureFormat `{channels, sampleRate, sampleFormat, bufferSize}`, controlPort, sampleRate, sourceChannels, groups, channels, stopped, samplesReceived, packetsSent, packetsDiscarded, musicians, localMix, localOutput, recording, lastTake, soundcheck, demo, load, localLimiter, localFxReturns, effects |
| `mix-changed` | at once | `{run, address, origin:"phone"/"desktop", mix}` or `{run, id:"local", origin:"desktop", mix}` (`dto.rs:712-760`) |

Both meters and status are due at pump start (`events.rs:126-131`); a stalled pump skips missed passes instead of bursting (`events.rs:153-164`).

---

##### 4. Client state model

###### 4.1 State shape (`ui/app/lib/console-state.js:135-167`, documented `:5-74`)

`engine {phase, error, status, run, endedRun}`, `musicians[]` (status row with mix nested as `mix`), `selected`, `departed {address,label}`, `pending {address:{editKey:value}}`, `mixError {address,message}`, `controlsNotice {address,outcome}`, `audio {host:"auto", device:null, bufferFrames:256, asioDriver:null}` (`engine-config.js:32-37`), `audioNotice {kind:"fallback"|"unsaved", message}`, `connection {status, error}` (starts `connecting`), `commandError {action,message}`, `devices {host,list,error}`, `presets`, `sessions {list,error}`, `sessionNotice`, `mixBackups {list,error}`, `localMix`, `outputs {list,error}`, `recordingSettings`, `takes {list,folder,unreadable,missing,error}`, `estimate`, `recordingOutcome {action,ok,message}`, `processing`, `processingPending {channel:settings}`, `processingError {channel,message}`, `stripPresets`, `eqCurves {channel:points}`, `limiterPending`, `sendsPending {channel:{reverb?,delay?}}`, `effectsPending`, `returnsPending {address:{reverb?,delay?}}`. Meters are never stored (`:2-3`). `LOCAL_MIX = "local"` is the local mix's key everywhere an address goes (`:114`).

###### 4.2 Store

- C1 One state value, pure reducer; a reducer returning the same object notifies nobody; subscribers get `(state, action)`; a dispatch from inside a subscriber is queued until every subscriber saw the current action; listeners are iterated over a copy; a throw empties the queue and leaves the store usable (`lib/store.js:13-39`).

###### 4.3 Runs and status merge

- C2 Phases that end a run: `idle`, `starting`, `error`. They set `endedRun = max(endedRun, run ?? 0)`, `run = null`, `status = null` and clear musicians, localMix, outputs, estimate, departed, pending, mixError, controlsNotice, commandError, audioNotice; `selected` survives so a restart reopens it. `running`/`stopping`/`stopped` only set phase and error and clear commandError (`console-state.js:130,526-550`). An unknown phase throws.
- C3 Live run: `Number.isInteger(run) && run > endedRun && (engine.run === null || run >= engine.run)` (`:362-365`). Status, mix-changed and meters of a non-live run are dropped (`:476, 796`, `console-app.js:526`).
- C4 Shown run: `run === engine.run` and live; required for `outputs`, `outputs-failed`, `soundcheck` answers (`:371-373, 817-822, 912-918`).
- C5 Status merge (`:475-524`): rows become musicians (mix nested via `mixFromDto`, every other field copied, `:376-384`); a known musician keeps its mix unless the row's `revision` is higher; local mix likewise (`:467-473`). Phase = `stopped` if `status.stopped` else `running`, but stays `stopping` while stopping; `error` cleared only when phase becomes `running`. `engine.status` = status without musicians, `engine.run = status.run`, `connection = ready`.
- C6 Selection: kept if its address is still listed (LOCAL_MIX counts while a local mix exists), else the first musician's address or null; if the open musician vanished, `departed = {address, label}`; departed clears when that address returns, on any `select`, or a run end (`:495-501, 788-794`). `select` of an unlisted address is a no-op.
- C7 A status drops pending edits, mixError and controlsNotice of addresses no longer listed (`:391-394, 515-520`).
- C8 Layout settle (`:435-458`): if the group id order changed, every `groupLevels.*` edit is dropped; if an input's role (`hidden`/`single`/`first`/`second` of a pair, `:406-421`) changed, its `channelGains`, `pans`, `channelMuted`, `channelSolo`, `lockedChannels` edits are dropped. Renames keep everything.
- C9 `mix-changed`: `id === "local"` maps to LOCAL_MIX (`:185-191`); taken only for a live run, a listed mix, and a strictly higher `revision` (`:573-588`). Revisions never go back from any source: status, event or command answer (`:461-463`).

###### 4.4 Optimistic edits, pending and echoes

- C10 `edit(address, edits)`: keys validated by `parseEditKey` (`field` or `field.index`; bad key throws, `mix.js:99-113`); for LOCAL_MIX the fields `maxLevelPercent`, `engineerLimitPercent`, `lockedChannels` are dropped; ignored if nothing left or the mix is not listed; merged into `pending[address]` (`:592-621`). The shown mix is `applyEdits(engineMix, pending)` (`viewMix`, `:979-986`).
- C11 `mix-applied(address, sent, mix)`: offer `mix` by revision (C9), then `settleEdits` removes each sent key whose pending value is still the sent value (a newer edit to the same key stays), clears `mixError` for that address (`:627-637`, `mix.js:280-288`).
- C12 `mix-failed`: settles the sent edits (fader falls back to the engine value) and sets `mixError {address, message}`; message = `Error.message` or `String(error)` (`:656-666`).
- C13 `engineer-applied`: settles as C11 using `outcome.mix`; `controlsNotice = {address, outcome}` unless `applied && saved`; a full success clears it for that address (`:641-654`).
- C14 `command-failed(action, message)`: phase reverts to `stopped`/`running` from the last status (or `idle` with none), run stays live, `commandError` set; `command-done(action)` clears only a matching action (`:553-564, 811-812`).
- C15 `session-loaded`: drop pending, mixError, controlsNotice and all of processingPending, processingError, limiterPending, sendsPending, effectsPending, returnsPending; presets from the outcome; audio restored if `audioLoaded` (`:669-689`).
- C16 Whole-value pendings (`processingPending`, `limiterPending`, `effectsPending`) settle only when the pending entry is the very object that was sent (`!==` identity), so a later edit stays (`:693-699`). Level pendings (`sendsPending`, `returnsPending`) settle when the pending level equals the answered level (`:702-711`).
- C17 After a limiter/effect/return answer, only that one value (that effect, that mix) is written into the kept status until the next status (`:716-764`).
- C18 `channelStrip(channel)`: pending edit, else the desk's strip, else a neutral default with `active:false`; `processingActive` is `null` while strips were never read (`:928-949`).

###### 4.5 Controller rules (`ui/app/lib/console-app.js`)

- C19 Connect (`:547-610`): order as in 1.7. Listeners register all-or-none; a partial set is unlistened and the error thrown (`:520-543`). Listening happens once per app; a later `reconnect` only re-reads.
- C20 One connection attempt at a time: a second `connect`/`reconnect` joins the running one (`:1588-1594`).
- C21 After `dispose`, events are dropped; meter listeners cleared; unlisten all (`:511-517, 1880-1886`). Meters bypass the store and reach `onMeters` listeners only for a live run (`:524-532, 1875-1878`).
- C22 Start (`:420-471`): `startRequest` null (ASIO without driver) never reaches the engine: `commandFailed("start", ASIO_NEEDS_DRIVER)` if a status exists, else phase `error`. Otherwise phase `starting`, `start_engine`; on failure with `orDefault`, list devices for the host and, if no device name contains the remembered one (case-insensitive substring), retry on the default input (never ASIO) and set audioNotice `fallback`. After success: `engine_status`; if unreadable, retry 3 times 500 ms apart while still `starting`, then `connectionFailed` (`:162-163, 311-333`); then refresh the disk estimate.
- C23 `startChoice` (Start, Try again, auto-start) reads the kept audio choice first; unread => nothing starts (C22 rule for running vs not); after a run that did not fall back, unsaved choices are saved via `set_audio_settings` (`:476-495`).
- C24 Stop (`:497-508`): `stopping`; success => `idle` (ends run); failure => `commandFailed("stop")` then re-read status (the run stays live).
- C25 Audio choices are applied and saved only after an engine runs with them; a failed save sets audioNotice `unsaved` (`:374-394`, `707-738`). A buffer change while on the fallback input is only kept for the next start (`:720-724`). Driver/buffer change and device switch are refused in words while recording or soundchecking (`:709-719, 744-751`).
- C26 Device switch: `restart` method => `applyAudio({}, {deviceFilter})`; live (WASAPI) => `switch_device`; failure => `commandFailed("switch")` + read status; success => `commandDone("switch")`, audio.device = name, clear fallback notice, save, read status (`:743-772`).
- C27 Coalescer (`lib/coalesce.js:11-69`): one lane per key, at most one send in flight; pushes while in flight replace the single waiting value (latest wins, intermediates never sent); a rejection or synchronous throw still releases the lane and is reported to `onError`; `busy(key)`, `idle(key)` promise.
- C28 Coalesced lanes (`console-app.js`): `set_mix`/`set_local_mix` keyed by address (`:208-223,245`); `set_engineer_controls` by address (`:226-246`); strip edits by channel (`:253-278`); limiter by mix address (`:1013-1042`); sends by `"<channel>:<effect>"` (`:1063-1083`); effect settings by effect (`:1098-1115`); returns by `"<address>|<effect>"` (`:1128-1156`). Mix sends read the pending edits from the store at send time, so the pushed value is `null` (`:1868-1872`). All sends skip while a session loads.
- C29 Serial desk lane `inTurn` (each waits for the previous): channel and group edits, session load, mix-backup restore, buffer, redundancy, processing reset/copy/preset-apply (`:790-800`). Processing commands also wait for the touched channels' (and linked partners') strip lanes to be idle (`:1311-1327`).
- C30 Channel edits are built on a fresh `channel_table`; group edits on the last `set_groups` answer, else the status's groups, else refused (`:805-849`). Each answers `{ok, message}` and re-reads status.
- C31 Session load holds every send while loading; on success the held edits are dropped and pending strip edits answer `CH <n>'s processing was not changed: a session was loaded meanwhile.`; then status and processing are re-read; on refusal the held edits are sent (`sendWaiting`) (`:1171-1251`). `unsavedEdits()` = pending mix edit keys + pending strips + unanswered channel/group edits (`:1255-1262`).
- C32 Local output choice lane: serial; a choice overtaken while waiting is not sent and answers `superseded: true`; the lane never stays rejected; status re-read after each (`:647-696`).
- C33 Recording settings: one at a time, each request built at send time on the last answered settings (`:1382-1402`). Stop recording retries 5 times 300 ms apart while `finishing` (`:166-167, 1453-1480`). Soundcheck seek: only the latest frame goes after the one in flight (`:1554-1572`). A soundcheck answer is written into the shown run's status at once (`:1521-1540`).
- C34 Every command answers `{ok, message}` in words; failures use `sentenceWithReason("<lead>", error)` (see F1); most commands re-read `engine_status` after an answer (e.g. `:776-784, 870-878`).
- C35 Window title follows `windowTitle(state)`: `BandEar Server — Demo source` while the demo runs, `BandEar Server — <deviceName>` with a status, else `BandEar Server`; set through `setTitle` only when it changes; failure is `console.warn` only (`console-app.js:1575-1584`, `lib/network-view.js:255-261`).

###### 4.6 Rates, scheduling and gestures

- C36 Backend rates: meters 33 ms, status 250 ms plus immediate on group/channel/soundcheck changes, mix-changed immediate (section 3).
- C37 `frameScheduler(draw)`: at most one draw per animation frame; `now()` draws immediately; after `dispose()` nothing draws, including a frame already requested (`lib/scheduling.js:14-51`). Used by processing and effects screens (`screens/processing-screen.js:238`, `effects-screen.js:92`).
- C38 `trailingThrottle(run, {intervalMs})`: first push runs at once; pushes within the interval fold into one trailing run at the interval's end; `cancel()` drops it (`scheduling.js:58-88`). Used at 500 ms for spoken reduction-meter values (`components/reduction-meter.js:15,53`).
- C39 Slider gesture (`lib/slider-gesture.js:15-116`), used by every fader/pan (`components/slider.js:29`):
  - press on track jumps (offset 0); press on thumb keeps the grab offset `levelFraction(value) - fraction`;
  - each move emits `input` only if the snapped, clamped value changed;
  - release emits one `change` with the dragged value only if it differs from the drag start; a value set from outside during the drag is held and becomes the shown value at release;
  - key step and reset each emit `input` + `change`, nothing if unchanged; unknown key returns null (not handled).
  - DOM wiring: pointerdown/move/up/cancel, `lostpointercapture`, `setPointerCapture`, keydown, dblclick reset, `role="slider"` with `aria-valuetext` (`components/slider.js:30-42,92,120`).

---

##### 5. Value and format rules

###### values.js (`ui/app/lib/values.js`)

| id | rule | examples (from `ui/tests/values.test.js`) |
|---|---|---|
| V1 | `FADER {min 0, max 100, step 1, page 10, reset 100}` (`:8`) | |
| V2 | `PAN {min 0, max 100, step 1, page 10, centre 50, detent 2, reset 50}` (`:11-19`) | |
| V3 | `METER_FLOOR_DB = -60`, `HOT_DB = -6`, `GROUP_TONES = teal, blue, violet, orange` (`:22-28`) | |
| V4 | `clampLevel(v,min=0,max=100)`: non-finite => min; else `min(max, max(min, Math.round(v)))` (`:31-37`). JS `Math.round` rounds .5 up (toward +inf). | 62.4->62, 62.5->63, -3->0, 140->100, NaN->0 |
| V5 | `levelFraction(p) = clampLevel(p)/100`; `levelFromFraction(f) = clampLevel(f*100)` (`:40-47`) | 62->0.62; 0.624->62; 1.2->100 |
| V6 | `pointerFraction(pos, start, len, {reverse})`: len<=0 => 0; `clamp01((pos-start)/len)`, reversed `1-f` for vertical (`:54-60`) | (300,100,200,rev)->0; (100,..)->1; (150,100,200)->0.25 |
| V7 | `keyValue(key, v, range)`: ArrowUp/Right +step, ArrowDown/Left -step, PageUp +page, PageDown -page, Home min, End max, other null; clamped; with `centre`, a move whose start and target lie on opposite sides (`(v-c)*(t-c) < 0`) stops on the centre (`:69-98`) | fader 62: Up 63, PgUp 72, PgDn 52, Home 0, End 100; 4 PgDn 0; pan 45 PgUp 50, 50 PgUp 60, 49 Right 50, 50 Left 49 |
| V8 | `snapToCentre(v)`: `abs(v-50) <= 2` => 50 (`:101-103`) | 52->50, 48->50, 53->53 |
| V9 | pan side/amount: `side = sign(v-50)`, `amount = abs(v-50)*2` (`:105-109`) | |
| V10 | `panLabel`: `C`, else `L<amount>`/`R<amount>` (`:112-118`) | 50 `C`, 40 `L20`, 65 `R30`, 0 `L100`, 51 `R2` |
| V11 | `panValueText`: `Centre`, else `<amount>% left`/`right` (`:121-127`) | 40 `20% left`, 65 `30% right` |
| V12 | `levelToDb(l)`: `v = min(100, Number(l)||0)`; v<=0 => -inf; `20*log10(v/100)` (`:130-136`) | 100->0, 50->-6.0206, 0 and -4 -> -inf |
| V13 | `meterFraction(l) = clamp01((db - (-60)) / 60)`, -inf => 0 (`:139-145`) | 50->0.8997, 1->1/3, 0.05->0, 150->1 |
| V14 | `meterZone(l)`: l>=100 `clip`; db >= -6 `hot`; else `safe` (`:148-153`) | 50 safe, 51 hot, 99 hot, 100 clip |
| V15 | `groupTone(i)`: non-negative integer => `GROUP_TONES[i % 4]`, else `none` (`:156-158`) | 4 `teal`, null/-1 `none` |

###### format.js (`ui/app/lib/format.js`; `MINUS = "−"` U+2212, `SEPARATOR = " · "`)

| id | rule | verbatim output |
|---|---|---|
| F1 | `sentenceWithReason(lead, reason)`: reason trimmed, trailing dots removed; empty/blank/null => `lead.` else `lead: reason.` (`:12-17`) | `... but not saved: the disk is full.` (also from `the disk is full..`) |
| F2 | `formatGroupsSet` (`:23-30`) | not applied: `Groups saved; they apply at the next start.`; applied+saved: `Groups applied to every phone.`; else `Groups applied to every phone, but not saved: <e>.` |
| F3 | `formatChannelsSet` (`:36-43`) | `Channel details saved; they apply at the next start.` / `Channel details applied to every phone.` / `Channel details applied to every phone, but not saved: <e>.` |
| F4 | `formatEngineerControlsSet(o, name)` subject `<name>'s limit and locks` or `Limit and locks` (`:50-58`) | `<subject> saved; they apply at the next start.` / `<subject> apply now and are saved.` / `<subject> apply now, but were not saved: <e>.` |
| F5 | `formatPercent(l) = clampLevel(l) + "%"` (`:61-63`) | 85 `85%`, 62.4 `62%` |
| F6 | `formatDb(db)`: -inf `−∞ dBFS`; `r = Math.round(db)`; r<0 `−<-r> dBFS` else `<abs r> dBFS` (`:66-72`) | -6.02 `−6 dBFS`, 0 and -0.3 `0 dBFS` |
| F7 | `formatSampleRate(hz) = parseFloat((hz/1000).toFixed(1)) + " kHz"` (`:75-77`) | `48 kHz`, `44.1 kHz`, `96 kHz` |
| F8 | `formatSampleFormat`: `/^([IUF])(\d+)$/`: I `<n>-bit`, U `<n>-bit unsigned`, F `<n>-bit float`; else unchanged (`:80-90`) | I24 `24-bit`, F32 `32-bit float`, U8 `8-bit unsigned`, DSD `DSD` |
| F9 | `formatBuffer`: `Fixed(<n>)` => `<n>`; `device default` => `default buffer`; else unchanged (`:93-99`) | `256`, `default buffer` |
| F10 | `formatCaptureFormat` = rate · format · buffer (`:102-108`) | `48 kHz · 24-bit · 256` |
| F11 | `linkState(m)` (`:121-154`): level from `linkHealth` in {good, warning, bad, unknown} else unknown. If any of underrunsPerSecond, lossPercent, receiveTimeoutsPerSecond is not a number: bad => `Link lost` / `Reports from this phone stopped`; not controlConnected => level unknown, `Not connected` / `The app on this phone is not connected`; else unknown `No reports` / `No reports from this phone yet`. With rates: bad and packetsPerSecond===0 => `No audio` / `This phone receives no audio`; else label Good/Warning/Poor/Unknown and detail `Loss <x.xx>% · Glitches <x.x>/s · Timeouts <x.x>/s` | `Loss 0.82% · Glitches 2.0/s · Timeouts 0.0/s` |
| F12 | `needsAttention`: linkState level warning or bad (`:157-160`) | |
| F13 | `musicianLabel`: name if truthy; else address without port; IPv6 `[x]:p` => `x` (`:163-173`) | `192.168.1.12`, `fe80::1` |

---

##### 6. Tests (`ui/tests`, node:test)

| file | tests | behaviours pinned |
|---|---|---|
| shell.test.js | 2 | brand row is the mark as decorative image (`alt=""`); mark file exists relative to index.html |
| router.test.js | 6 | section list/order/labels with Mix default; each hash opens its section; trailing slash, missing slash, case tolerated; empty/unknown opens Mix; `#/processing/<n>` channel 1-32 and canonical hash; title `X · BandEar` |
| store.test.js | 7 | initial state; dispatch notifies with (state, action); unchanged state notifies nobody; unsubscribe; unsubscribing mid-notify skips no one; nested dispatch ordering; throwing reducer leaves store usable |
| coalesce.test.js | 6 | first value sent at once; in-flight waits, only latest sent after; failure still releases and reports; synchronous throw counts as finished; lanes per key; `idle` resolution |
| scheduling.test.js | 4 | one draw per frame; disposed scheduler never draws (late promise); frame running at dispose does not draw; trailing throttle at once then once with latest, cancel |
| slider-gesture.test.js | 9 | track press jumps, drag follows, one change at end; thumb grab offset; no change when ending at start; move/release without press inert; outside value held during drag; clamping and same-value no-op; key step input+change; reset quiet when equal; pan detent and keyboard centre stop |
| values.test.js | 12 | every row of V4-V15 |
| format.test.js | 9 | F2, F3 (incl. dot and blank reasons), F5-F6, F7-F10, F11 good/warning/poor, lost/no audio, no reports/not connected/unknown health, F12, F13 |
| console-state.test.js | 36 | initial state; status fills musicians and selects first; selection fallback; unlisted select no-op; stopped status; phase follows start/stop with error; stopping stays stopping; idle forgets run data but not selection; late status of stopped or replaced run ignored; new run's status before start answer; late mix change ignored; meters live run rule; mix change from phone/desk; unlisted mix change ignored; pending over server mix; later phone change does not hide pending; answer settles carried edits only; older answer never rolls back; answer-before-event and event-before-answer; status revision never rolls back; refusal falls back + error; bad edit key throws; status drops departed musicians' edits; attention list; departed remembered; non-open leave silent; default audio auto/256, ASIO only when chosen; audio notice lasts its run; failed stop keeps run; command-done clears only same action; connection states; unknown action no-op; group-move and pair/hide layout settle |
| console-app.test.js | 56 | connect to running engine starts nothing; auto-start when stopped (never ASIO); no auto-start flag; failed start + late status ignored; late events after stop ignored; meters to listeners until unsubscribed; set_mix one at a time, latest wins; answer before event; phone change between request and answer; refusal falls back; failed stop keeps run; stop error that did stop; start ok but status unreadable keeps run; only notRunning triggers start; failed listen shown and listens once on reconnect; status never comes => try again; two reconnects listen once; device listing per driver; driver change restarts, buffer keeps device; ASIO rules (picked driver only, reuse, reconnect, no-driver message, failed ASIO keeps previous); live vs restart switch; refused switch said and forgotten; switch clears earlier refusal; remembered input reopened; read before auto-start; missing input falls back to default, said and kept; both failures said; ASIO never falls back to another ASIO; start failure with input present shows error; choices saved only after a run; unread choice rules (re-read on reconnect, nothing starts, choosing an input starts, running engine kept, later read does not overwrite, unsaved said and retried); add/remove/rename; label survives new IP; window title; select; dispose; channel/group edit serialisation, refusal, smaller-device details kept, offline keep, table unreadable, ids and 1-based members |
| bridge.test.js | 44 | 7 test bridge.js itself (invoke passthrough, payload-only listen + unlisten, setTitle, missing globals error, argument names for every command, audio settings argument names, the three events); 37 test `fake-bridge.js` (simulated engine: runs, revisions, refusals, groups/channels semantics, pairs, hidden channels, device switch, labels, meters). Port only if the Rust app keeps a fake engine |

---

##### 7. Hard for a native toolkit

1. Live regions: `ml-status-pill` host is `role="status"` (polite announcement on phase change); chips expose one accessible name per link with `aria-hidden` inner text; nav uses `aria-current="page"`; reduction meters speak through a 500 ms trailing throttle. GPUI accessibility support for these roles is unverified here.
2. Focus management: skip link to main; on navigation focus moves to the screen `h1` (visually hidden on Mix, `screens/mix-screen.js` LAYOUT `h1.ml-visually-hidden "Mix"`) but not on first paint; `:focus-visible`-only rings (keyboard vs pointer distinction).
3. Responsive layout: three sidebar layouts by width (full 212px, 64px icon rail with tooltips and hidden labels, horizontal row), sticky sidebar and sticky chip strip.
4. Custom drawing: SVG nav icons, kind icons, brand mark (arcs and quadratic/cubic paths), warning triangle; CSS triangle play marker; hollow/filled/square status dots; pulse animations (1s alternate, 1.2s, 1.6s) disabled under reduced motion.
5. Drag gestures: pointer capture, thumb grab offset vs track jump, detent snap, held external value during drag, double-click reset, arrow/Page/Home/End keys (C39, V7).
6. Hash routing and history: `#/processing/<n>` deep links, `replaceState` canonicalisation, per-section document title plus a separate native title that follows the device (C35, R4). GPUI has no URL bar; deep-link parity means an internal route enum with a channel parameter.
7. Async ordering the UI relies on: per-key coalescing, a serial desk lane, session-load hold-and-drop, run-number filtering and revision monotonicity (C3-C33). These are behaviour, not visuals, and the 56 + 36 tests pin them.
8. Number formatting: JS `Math.round` rounds .5 toward +inf (`formatDb(-6.5)` gives `−6`), Rust `f64::round` rounds away from zero (`-7`); `toFixed`/`parseFloat` trimming in F7; U+2212 minus and U+2026 ellipsis in copy.
9. Variable fonts with `font-stretch 75-100%` (IBM Plex Sans) and two bundled TTFs; the 212px sidebar width is sized to fit `Network and system` in that font (`tokens.css:126-127`).

#### B. Mix screen inventory (parity spec)

Source: `apps/desktop` at the worktree's HEAD. Paths below are relative to `apps/desktop/ui/app/`
unless they start with `ui/tests`, `ui/dev`, `src-tauri` or `apps/server`. "Verbatim" strings are
quoted exactly, including the true minus `−` (U+2212), the ellipsis `…` and the separator ` · `.

Data flow in one line: the Tauri events `status` (~4/s), `mix-changed` (at once) and `meters`
(~30/s) feed the store/meter listeners (`lib/bridge.js:16-29`); the screen renders view models
from `lib/mix-view.js` at most once per animation frame (`screens/mix-screen.js:226-234`); meters
bypass the store and go straight to the strips (`screens/mix-screen.js:236-248`).

##### 1. Layout

###### 1.1 Regions, top to bottom (`screens/mix-screen.js:52-78`)

1. Visually hidden `<h1 tabindex="-1">Mix</h1>`.
2. Top bar `<ml-top-bar>` (data `topBarView`, `lib/mix-view.js:38-53`): status pill (phase),
   "`<device> · <format>`" readout (hidden with no status; format e.g. "48 kHz · 24-bit · 256",
   `lib/format.js:102-108`), attention link to `#/network` with a warning-triangle icon and
   "`N phone needs attention`" / "`N phones need attention`" (`lib/mix-view.js:43`), engine button
   "Start engine" / "Stop engine", "Starting…" / "Stopping…" disabled while busy
   (`components/top-bar.js:11,141-147`). Action per phase: idle/error -> start, running/stopped
   -> stop, starting/stopping -> none (`lib/mix-view.js:25-32`).
3. Tabs row `.mix-tabs` (flex, wrap, bottom border): `<ml-musician-tabs>` (flex 1 1 320px) and,
   right-aligned, `<ml-mix-tools>` (hidden unless a musician, not the Local mix, is open:
   `screens/mix-screen.js:163`; CSS `screens/mix-screen.css:19-35`).
4. Message lines, each a bordered box, in this fixed order (`screens/mix-screen.js:59-65`):
   tools outcome (`role=status`), tools/limiter/return error (`role=alert`, danger colours),
   departed (status), mix error (alert), engineer-controls notice (status), command error (alert),
   audio notice (status). Each hidden when empty.
5. Notice card (title, text, optional action button) shown instead of the strips
   (`screens/mix-screen.js:66-70`, max-width 60ch, `screens/mix-screen.css:52-78`).
6. Body `role="tabpanel"`, `aria-label` "Local mix" or "`<name>'s mix`"
   (`screens/mix-screen.js:71-77,194`): left `section aria-label="Channels"` (channel strips, gap
   6px, scrolls sideways `overflow-x:auto`), right `section aria-label="Groups and master"` (group
   strips then the master strip, separated by a left border; `screens/mix-screen.css:80-114`).
   Body hidden when a notice shows or no mix is open (`screens/mix-screen.js:190-193`).

###### 1.2 Tabs

- One tab per musician in the engine's (status) order, then "Local mix" last while an engine runs
  (`lib/mix-view.js:59-72`; `lib/local-mix.js:178-188`, null when `state.localMix` is null).
- Tab content: bold name (`musicianLabel`: name, else IP without port, IPv6 without brackets,
  `lib/format.js:163-174`) + `<ml-link-state>` (mark + word, tooltip = detail)
  (`components/musician-tabs.js:104-112`). Musician link words from `linkState`
  (`lib/format.js:121-155`): "Good", "Warning", "Poor", "Link lost", "Not connected",
  "No reports", "No audio"; detail "`Loss 0.82% · Glitches 1.2/s · Timeouts 0.0/s`" form, or
  "Reports from this phone stopped", "The app on this phone is not connected", "No reports from
  this phone yet", "This phone receives no audio".
- Local tab link (`lib/local-mix.js:152-172`): running -> level `local`, "`This computer · <dev> ·
  outputs 3/4`", detail "The local mix plays on this computer"; failed -> level `warning`,
  "This computer · output stopped", detail = reason or "The output stopped"; else level `local`,
  "This computer · no output", detail "Choose where it plays in Network and system". Level
  `local` has no style of its own in `components/link-state.js` (falls to the default muted ring
  mark; marks: good dot, warning triangle, bad square, unknown ring, `components/link-state.js:6-7`).
- Selected tab: 2px accent border, raised background, inset 3px accent bar under it
  (`components/musician-tabs.js:43-47`).

###### 1.3 Channel strip `<ml-channel-strip>`, 84px wide, top to bottom (`components/channel-strip.js:125-138`)

1. Name row: instrument-kind icon (14px; kinds voice, guitar, bass, keys, drums, percussion,
   horns, strings, playback, click, other: `components/kind-icons.js:4-15`) + label (ellipsis,
   tooltip = label).
2. Group chip row (min-height 20px; chip hidden when no group).
3. "Follows `<leader>`" line (accent, hidden unless mirrored).
4. "STEREO" line with a two-circle icon (12px), `aria-hidden`; on a mono strip it keeps its space
   (`visibility:hidden`) so pans and faders line up (`components/channel-strip.js:69-72,129`).
5. Pan `<ml-pan>` (64x36).
6. Levels row: input meter (8px wide) then fader (40px wide), filling the strip height (min 200px,
   `components/strip-styles.js:30-36`).
7. Readout `<output>` ("85%" or "muted").
8. Buttons row: Mute "M", Solo "S".
9. Lock toggle, text "Locked"/"Unlocked" (min-width 64px, small type), hidden on the Local mix.
10. Processing link (`#/processing/<first side 1-based>`), text "Processing" or "Processing on"
    (accent border when on) (`components/channel-strip.js:211-216`; `lib/router.js:63-65`).

###### 1.4 Group strip `<ml-group-strip>`, 74px, sunken background (`components/group-strip.js:42-48`)

Caption "GROUP" (aria-hidden), group chip, "Follows X" line, fader (no meter), readout "NN%". No
mute: "Groups have no mute of their own in the engine" (`components/group-strip.js:7`).

###### 1.5 Master strip `<ml-master-strip>`, 188px, accent border + accent background (`components/master-strip.js:136-189`)

1. Caption "MASTER" (aria-hidden).
2. Levels row: meter "Output left", meter "Output right", then three fader columns each with a
   sub-caption under it: "LEVEL" (full width fader), "CAP" (compact 32px), "LIMIT" (compact 32px).
   CAP and LIMIT columns hidden on the Local mix (`components/master-strip.js:259-260`). Column
   tooltips: "The musician's own level cap; they can change it on their phone too", "Your limit for
   this musician; their phone cannot change it".
3. Readout (wraps, centred): "`85% · cap 90% · limit 100%`" / Local mix: "`85%`".
4. Ceiling note `<p id="ceiling">` (hidden on the Local mix).
5. Mute toggle "Mute" (wide 64px).
6. Limiter group (rule above): head = toggle "Limiter" + state word "On"/"Off"; "Ceiling" label +
   value + range slider; "Release" label + value + range slider; gain-reduction meter (bar + 56px
   value column); note line (`components/master-strip.js:158-175`).
7. Effect returns group (rule above): head "Effect returns"; "Reverb" + value + slider; "Delay" +
   value + slider (`components/master-strip.js:176-188`).

###### 1.6 Responsive (`screens/mix-screen.css:7-9,116-137`)

- Wide: screen fills `100vh`; channels and side sit in one row; channels scroll horizontally.
- `max-width: 860px`: page height auto; body becomes a column; channels and side each min-height
  320px; side gets a top border instead of a left one and scrolls horizontally itself.
- Tools panels are `min(440px, 100vw - 32px)` wide, absolutely positioned under the tools row,
  right-aligned (`components/mix-tools.js:89-103`). Tabs and tools row wrap.

##### 2. Controls

Commands are Tauri `invoke` names (`lib/bridge.js`), registered in `src-tauri/src/lib.rs:737-794`.
"Coalesced" = `createCoalescer` (`lib/coalesce.js:11-69`): per key one request in flight; pushes
while in flight replace the waiting value; the last push is always sent.

| Control | Verbatim label / aria-label | Range / step / keys | States (disabled + reason) | Command + args, timing | Source |
| --- | --- | --- | --- | --- | --- |
| Musician tab | visible name + link word; `role=tab`, `aria-selected` | Click; Left/Up prev, Right/Down next (wrap), Home, End: focus moves AND selects | Selected / not; one tab in tab order (roving tabindex) | none (local `select`) | `components/musician-tabs.js:68-73,90-101,114-135` |
| Channel pan (mono) | aria-label "`<label> pan`" | 0-100 (0 L, 50 C, 100 R), step 1, PgUp/PgDn 10 (stop at centre when crossing), Home 0, End 100, dbl-click 50, drag snaps to 50 within ±2 | disabled while mix follows a leader (no reason text on control; strip line "Follows X") | `set_mix {address, mix:{pans:[...]}}` on every move (`input`), coalesced per mix | `components/channel-strip.js:158,191`; `lib/values.js:11-19,69-103` |
| Channel balance (pair) | "`<label> balance`" | as pan; writes both pans `[max(0,2b-100), min(100,2b)]` | as pan | `set_mix` pans of both sides, on move | `lib/mix-view.js:155-161`; `lib/channels.js:83-92` |
| Channel fader | "`<label> level`" | 0-100 %, arrows 1, PgUp/PgDn 10, Home 0, End 100, dbl-click 100 | disabled while following; fill tone "muted" when muted | `set_mix {channelGains}` on move, coalesced; pair -> both sides | `components/channel-strip.js:157,194-196`; `components/fader.js:3-9` |
| Mute (channel) | text "M", aria-label "`Mute <label>`" | click/Space/Enter | pressed = muted; disabled while following | `set_mix {channelMuted}` on press | `components/channel-strip.js:134,159,199-200` |
| Solo (channel) | "S", "`Solo <label>`" | click | pressed; disabled while following | `set_mix {channelSolo}` on press | `components/channel-strip.js:160,201-202` |
| Lock | text "Locked"/"Unlocked", aria-label "`Lock <label> on the musician's phone`" | click | pressed = locked; stays enabled while following; hidden on Local mix; tooltip "Locked: the musician's phone cannot change this channel; you still can" / "Unlocked: the musician's phone can change this channel" | `set_engineer_controls {address, controls:{lockedChannels:[CH1..furthest]}, clientId}` on press, own coalescer per address | `components/channel-strip.js:136,161,203-209`; `lib/mix.js:195-209`; `lib/bridge.js:125-126` |
| Processing link | text "Processing"/"Processing on"; aria-label "`Processing of <label>, on`" / "`, off`" / "`, not read yet`"; title "Open this input's high-pass, equaliser, gate and compressor" | link | always live | navigation only | `lib/processing-view.js:416-428`; `components/channel-strip.js:211-216` |
| Group fader | "`<name> group level`" | as fader | disabled while following | `set_mix {groupLevels:[0..index]}` on move, coalesced | `components/group-strip.js:57-65,79-84` |
| Master level | "Master level" | as fader; tone "accent", "muted" when muted | never disabled (master stays the musician's own while following) | `set_mix {volumePercent}` / Local: `set_local_mix {mix}` on move | `components/master-strip.js:142,257`; `lib/master-controls.js:16` |
| Phone level cap | "Phone level cap", `aria-describedby=ceiling` | as fader (compact) | hidden on Local mix | `set_mix {maxLevelPercent}` on move | `components/master-strip.js:146`; `lib/master-controls.js:17` |
| Engineer limit | "Engineer limit", `aria-describedby=ceiling` | as fader | hidden on Local mix | `set_engineer_controls {controls:{limitPercent}}` ONLY on gesture end (`change`: release, each key press, reset); one per drag | `components/master-strip.js:150`; `lib/master-controls.js:18` |
| Master Mute | text "Mute", aria-label "Mute master" | click | pressed | `set_mix {muted}` / `set_local_mix` on press | `components/master-strip.js:157`; `lib/master-controls.js:19` |
| Limiter on | text "Limiter", aria-label "Limiter on" (static) | click | pressed = enabled | `set_musician_limiter {musicianKey, settings:{enabled,ceilingDb,releaseMs}}` / `set_local_limiter {settings}`, coalesced per mix, then `engine_status` read | `components/master-strip.js:160,326-329`; `lib/console-app.js:1013-1053`; `lib/bridge.js:263-269` |
| Limiter ceiling | label "Ceiling", aria-label "Limiter ceiling", `aria-describedby=limiter-note`, `aria-valuetext` = "0 dBFS" form | native range 0-1000 step 1 = -24..0 dBFS linear, value rounded to 0.5 dB; a key step that rounds to the same value moves 0.5 dB | thumb not moved by data while pointer-held | as Limiter on, on every `input` (drag) with `key` flag | `components/master-strip.js:166,330-338`; `lib/limiter.js:88-94,111-114,155-170` |
| Limiter release | "Release", "Limiter release", valuetext "50 ms" | range 0-1000, log 1..1000 ms, rounded 1 ms; key step min 1 ms | as ceiling | as ceiling | `components/master-strip.js:171`; `lib/limiter.js:97-103` |
| Reverb return | "Reverb", aria-label "Reverb return", valuetext "Off" / "40%" | range 0-100 step 1 % | thumb held while dragged | `set_musician_fx_return {musicianKey, effect:"reverb", level}` / `set_local_fx_return {effect, level}` on every `input`, coalesced per "address\|effect" | `components/master-strip.js:181,213-232`; `lib/console-app.js:1128-1168`; `lib/bridge.js:285-288` |
| Delay return | "Delay", "Delay return" | as reverb | as reverb | as reverb with `effect:"delay"` | `components/master-strip.js:186` |
| Mirror select | label "Mirror"; aria-label "`Mirror: whose mix <name>'s follows`"; options "Off", each other musician, "`<name> (not here)`" | select | shows pending choice until answered | `set_mirror {address, clientId, leader:{address, clientId, name}\|null}` on change, then `engine_status` | `components/mix-tools.js:179,243-246,338-366`; `lib/console-app.js:1685-1698`; `lib/bridge.js:176-177` |
| Copy mix to… | button "Copy mix to…", `aria-expanded`, `aria-controls=copy-panel` | toggles panel; Escape closes, focus back to button | `aria-expanded` true -> accent border | none | `components/mix-tools.js:180,247-249` |
| Copy targets | checkbox per other musician, text name + "(`follows X`)" | | disabled when that musician follows another | | `components/mix-tools.js:369-394`; `lib/mixes.js:185-202` |
| Copy | button "Copy" (primary) | | `aria-disabled` until engine runs and ≥1 ticked; hint "Start the engine to copy a mix." | `copy_mix {from, to:[addresses]}` on click, then `engine_status` | `components/mix-tools.js:188-189,332`; `lib/console-app.js:1672-1679` |
| Presets | button "Presets" | toggles panel; opening it with presets unread emits `presets-load` -> `presets` | | `presets` | `components/mix-tools.js:181`; `lib/mix-tools.js:147-158` |
| Save name | label "`Save <name>'s mix as`", text input maxlength 40; Enter saves | | | | `components/mix-tools.js:194-195,272-277` |
| Save preset | "Save preset" | | `aria-disabled` while no engine; hint "Start the engine to save or apply presets."; also inert with blank name | `save_preset {address, name}` | `components/mix-tools.js:196,404`; `lib/console-app.js:1640-1645` |
| Preset select | label "Preset" | select; change disarms delete | keeps choice when list changes, else first | | `components/mix-tools.js:203,279-282,411-429` |
| Apply targets | checkboxes "Apply to", open musician first with "(this mix)" / "(this mix, follows X)", ticked at start unless following | | followers disabled | | `lib/mix-tools.js:93-100`; `lib/mixes.js:190-193` |
| Apply | "Apply", aria-label "`Apply <preset>`" | | `aria-disabled` until engine + preset + ≥1 tick | `apply_preset {id, musicians}` | `components/mix-tools.js:206,432-433`; `lib/console-app.js:1663-1670` |
| Rename | label "Rename to", aria-label "`New name for <preset>`", maxlength 40, Enter renames; button "Rename" | | works with no engine | `rename_preset {id, name}` | `components/mix-tools.js:208-210,434`; `lib/mix-tools.js:208-213` |
| Delete preset | "Delete preset" (danger), aria-label "`Delete the preset <name>`" -> armed: "Keep" (focused, takes Delete's place), "`Delete <name>? No musician's mix changes.`", "Delete" | confirm works only ≥600 ms after arming; disarms after 10 s | confirming Delete `aria-disabled` until ready | `delete_preset {id}` | `components/mix-tools.js:463-514`; `lib/mix-tools.js:13,216-245`; `lib/network-view.js:44,250-252` |
| Read them again | button in "The presets could not be read." line | | | `presets` | `components/mix-tools.js:199,278` |
| Close (panels) | "Close" | Escape too | | | `components/mix-tools.js:189,214,258-260` |
| Notice action | "Try again" / "Start engine" | | | disconnected -> reconnect (listen + `engine_status`); else start (`start_engine {config}`) | `screens/mix-screen.js:313-319` |
| Top-bar engine button | "Start engine" / "Stop engine" | | disabled while starting/stopping | `start_engine` / `stop_engine` | `components/top-bar.js:120-125` |

##### 3. Displayed values

- Channel/group/master readout: "`NN%`" (`formatPercent`, whole 0-100, `lib/format.js:61-63`);
  channel or master muted -> "`muted`" with muted colour (`lib/mix-view.js:140,212`;
  `components/channel-strip.js:73-75`). Group readout never says muted.
- Master readout: "`<level> · cap NN% · limit NN%`" (`lib/mix-view.js:230-232`), e.g.
  "85% · cap 90% · limit 100%", "muted · cap 90% · limit 100%"; Local mix: level only.
- Ceiling note: "`The lower of the phone's cap and your limit wins: at most NN%.`" (NN = min(cap,
  limit); limit defaults to 100 when absent) (`lib/mix-view.js:223-233`).
- Pan readout (visible, under the track, mono font): "C", "L20", "R30" where the number is
  `|value-50|*2` (`lib/values.js:105-118`). aria-valuetext "Centre", "20% left", "30% right"
  (`lib/values.js:121-127`).
- Fader aria-valuetext "`NN%`" (`components/fader.js:129-131`); fader has no visible value of its own.
- Input/output meter: bar only (no number), drawn on a 60 dB scale: fraction = (dB+60)/60,
  dB = 20·log10(level/100), 0 -> empty (`lib/values.js:22,130-145`). Colours (`lib/values.js:148-153`;
  `app/tokens.css:72-75`): clip (red `#f85149`) at level ≥ 100; hot (accent `#f5a524`) from
  -6 dBFS (levels are whole numbers, so 51-99; 50 is -6.02 dB and safe); safe (green `#3fb950`) below; ground `#0b0f14`. **No peak hold, no
  ballistics/decay in the UI**: each `meters` value is drawn as is, once per frame
  (`components/meter.js:105-123`). Strip meter = max of a pair's sides (`lib/mix-view.js:173-175`).
- Master meters: open musician's `meters.musicians[address].left/right`, or `meters.local` on the
  Local mix tab, else 0 (`lib/mix-view.js:167-170`).
- Limiter values: ceiling "`0 dBFS`", "`−6 dBFS`", "`−6.5 dBFS`" (`lib/limiter.js:78-85`); release
  `formatMs`: "`0.5 ms`"-style below 10 (2 decimals trimmed), "`150 ms`", "`1 s`"/"`1.5 s`" from
  1000 (`lib/processing-view.js:65-67,105-111`). State word "On"/"Off"
  (`components/master-strip.js:303`).
- Limiter note (`lib/limiter.js:198-206`): on -> "`Peaks are held at <ceiling> or the mix's ceiling
  (NN%), whichever is lower. No added latency.`"; Local mix uses "`the mix's full scale`" instead of
  "the mix's ceiling (NN%)"; off -> "`Off: the mix is only clipped at its ceiling.`"
- Gain reduction: bar filling from the right (blue `#4cc2ff`), 12 dB full scale, plus text
  "`−4.0 dB`" / "`0.0 dB`" (values < 0.05 dB read 0) (`lib/limiter.js:25`;
  `lib/processing-view.js:360-366`; `components/reduction-meter.js:24-44`). Forced to 0 while the
  limiter is off (`components/master-strip.js:292-294,317-319`). Source: `meters.musicians[a]
  .limiterReductionDb` / `meters.local.limiterReductionDb`, non-finite or ≤0 -> 0 (`lib/limiter.js:71-75`).
- Return values: "Off" at 0, else "`NN%`" (`lib/effects.js:131-135`).
- Group chip: name, or "No group" without one; tone by group position cycling teal, blue, violet,
  orange (`lib/values.js:28,156-158`; colours `app/tokens.css:78-87`); invalid -> dashed border in
  text colour + bold "!" + visually hidden "`, input missing`" + tooltip "This group names an input
  the current device does not have" (`components/group-chip.js:57-73,86,99-106`).
- "Follows X" line and mirror note: see M10.

##### 4. Rules

**Mix model and sending**

- M1. A mix = `{revision, channelGains[32], pans[32], channelMuted[32], channelSolo[32],
  groupLevels[16], volumePercent, maxLevelPercent, muted, engineerLimitPercent, lockedChannels[32]}`;
  levels are whole 0-100 (`lib/mix.js:5-14,21-22`). Default: gains 100, pans even-index 0 / odd
  100, groups 100, volume 100, cap 100, limit 100, nothing muted/soloed/locked (`lib/mix.js:49-63`).
  Missing `engineerLimitPercent` reads 100, missing locks read none (`lib/mix.js:79-84`).
- M2. Edits are keyed "`field`" or "`field.index`"; unknown keys or out-of-range indexes throw
  (`lib/mix.js:88-115`). The shown mix = engine mix + pending edits (`lib/console-state.js:979-986`).
- M3. `set_mix` sends only edited fields; a table is sent from entry 0 up to the furthest edited
  index (earlier entries as shown); values clamped/rounded to 0-100 integers, flags as booleans
  (`lib/mix.js:157-188`). Engineer fields never go in `set_mix` (`lib/mix.js:36-40,148-155`).
- M4. Per mix, one `set_mix` in flight; the request is built at send time from all pending mix
  edits; engineer edits use a second coalescer (`lib/console-app.js:204-246,1861-1873`). An answer
  settles an edit only if its value is still the one sent (`lib/mix.js:280-288`); the answer's mix
  is taken only if its revision is newer (`lib/console-state.js:461-463,573-590,627-638`). Refusal:
  pending edits it carried are dropped and `mixError` set (`lib/console-state.js:656-666`). While a
  saved session loads, sends are held (`lib/console-app.js:211-214`).
- M5. Phone-side changes: `mix-changed` events (`origin: "phone" | "desktop"`) and the ~4/s
  status replace the mix when their revision is newer (`lib/console-state.js:795-796,480-484`);
  pending local edits stay on top until settled. During a pointer drag a value set from outside
  waits and appears on release (`lib/slider-gesture.js:9-11,38-50,68-80`), so a phone change never
  moves a held thumb. Only events of the live run count (`lib/console-app.js:523-530`).
- M6. Pending edits are dropped when the layout moves under them: group-level edits when group ids
  or their order change; channel edits for an input whose role (hidden / single / first / second
  side of a pair) changed. A rename keeps them (`lib/console-state.js:404-459`).

**Mute / solo / groups**

- M7. Mute and solo are per mix and per input (each musician has their own). Engine semantics
  (not in the UI, `apps/server/src/mix.rs:1133-1136,1380-1406`): a channel plays only if not
  hidden, not muted, and (no non-hidden channel is soloed or it is soloed); mute wins over solo; a
  hidden channel's solo counts for nothing; leaving solo restores levels. Master Mute silences the
  mix and its effect returns (`apps/server/src/mix.rs:1391`; `README.md` Effects table).
- M8. Groups: one group strip per configured group, in status order; a channel's chip is found by
  `groupId` (two groups may share a name); level `groupLevels[position]`; no group mute
  (`lib/mix-view.js:75-81,105-147,182-198`).

**Linked pairs**

- M9. A pair = an input with `link` whose next input exists and has the same hidden state; hidden
  units have no strip (`lib/channels.js:63-76`; `lib/mix-view.js:115-116`). The pair is one strip
  named after its first side, `index` = first side, marked "STEREO"; level, mute, solo shown from
  the first side; pan shown as balance = mean of the two pans; locked if either side is locked;
  every move writes both sides (balance -> two pans) (`lib/mix-view.js:120-161`;
  `lib/channels.js:83-92`). The engine keeps a pair's level/mute/solo/locks equal
  (`lib/channels.js:153-176`). The strip's processing link opens the first side.

**Mirror (follow), copy, presets (tools)**

- M10. While the open mix follows a leader (`mirror.name`), every channel strip and group strip
  shows "`Follows <leader>`" (tooltip "`This mix follows <leader>'s: move <leader>'s instead`" on
  channel strips) and its pan, fader, Mute, Solo / group fader are disabled; its accessible name
  becomes "`<label>, follows <leader>'s mix`" (pair: "`<label>, stereo pair, follows …`"; group:
  "`<name> group, follows …`"). Lock, processing link and the whole master stay live
  (`lib/mix-tools.js:20-26`; `components/channel-strip.js:173-182`;
  `components/group-strip.js:75-79`).
- M11. Mirror options: "Off", every other musician except one that would close a circle
  (directly or through others; self too), plus "`<name> (not here)`" (value "away") while the
  leader is absent; choosing "away" or the current value does nothing (`lib/mixes.js:116-177`;
  `lib/mix-tools.js:116-123`). The pending choice shows until any answer or until the desk's
  mirror changes (`lib/mix-tools.js:75-86,110-113,129-132`). Note under the row
  (`aria-describedby` on the select): "`<name> follows <leader>'s mix: its channels and groups move
  with it, its master stays its own.`" or "`<name> follows <leader>'s mix. <leader> is not here, so
  <name>'s mix stays as <leader> left it until they are back.`" (`lib/mixes.js:162-176`). A mirror
  can be set with no engine (kept for next start).
- M12. Copy carries channels, pans, mutes, solos and group levels; never master, cap, limit or
  locks (`lib/mixes.js:96-102`; hint text `components/mix-tools.js:187`). Targets exclude the open
  musician; followers are offered disabled with "(follows X)" (`lib/mixes.js:185-202`). A done
  copy unticks (`lib/mix-tools.js:133-135`).
- M13. Presets: name trimmed, 1-40 characters, unique case-insensitively (errors "a preset needs a
  name", "a preset name is at most 40 characters", `a preset named "X" already exists`,
  `lib/mixes.js:27-40`). A preset keeps each device input's gain/pan/mute/solo and group levels by
  group id; applying lands by group id, leaves master/cap/engineer controls, and makes pairs equal
  (`lib/mixes.js:46-90`). Save/apply/copy need a running engine (`canUse`); rename/delete do not
  (`lib/mixes.js:210-219`; `lib/mix-tools.js:185-213`). The open musician is pre-ticked in "Apply
  to" unless following (`lib/mix-tools.js:93-100`). Switching musician clears ticks, pending
  mirror and armed delete (`lib/mix-tools.js:76-81`). Done save/rename empties its field
  (`components/mix-tools.js:308-319`).
- M14. One panel open at a time; opening moves focus to the panel's first input/select/button;
  closing (button, Close, Escape) returns focus to its opener and disarms delete
  (`lib/mix-tools.js:147-166`; `components/mix-tools.js:444-461`).
- M15. Tool commands run only with a musician open, then read `engine_status`; every answer goes
  to the message line above the strips and back to the tools (`lib/mix-tools.js:261-290`;
  `lib/console-app.js:870-879`). Outcome sentences: "`Saved <name>'s mix as "<preset>".`",
  "`Renamed "<old>" to "<new>".`", "`Deleted the preset "<name>".`", "`Applied "<preset>" to Bass and
  Keys.`", "`Copied <from>'s mix to Bass, Keys and Drums: channels, pans, mutes, solos and group
  levels.`", mirror: "`<f> follows <l>'s mix now.`", "`<f> will follow <l>'s mix from the next
  start.`", "`<f> no longer follows another mix; their mix stays as it is.`", "`<f> will follow no
  other mix from the next start.`", with ", but this was not saved: <reason>." variants
  (`lib/console-app.js:1640-1698`; `lib/mixes.js:131-136,226-251`). Refusals: "`<lead>: <reason>.`"
  with leads "The preset was not saved", "The preset was not renamed", "The preset was not
  deleted", "The preset was not applied", "The mix was not copied", "The mirror was not changed"
  (`lib/format.js:12-17`). Unknown preset name in a sentence reads "the preset"
  (`lib/console-app.js:881-883`).

**Engineer locks, limit, level cap**

- M16. Lock: binds only the musician's phone; the desk still moves the strip (controls stay live)
  (`components/channel-strip.js:3-5`). The engine puts back a locked channel's level, pan, mute and
  solo after a phone change (`lib/channels.js:181-198`, mirrors the engine). Locks sent from CH 1 up to the furthest
  edited channel (`lib/mix.js:190-209`).
- M17. Level cap (`maxLevelPercent`): the phone's own; the musician can change it too; sent as it
  moves. Engineer limit (`engineerLimitPercent`): desk only; sent at gesture end. The engine plays
  at most `min(cap, limit)` (`lib/mix-view.js:200-234`; `apps/server/src/mix.rs:1381`).
- M18. A `set_engineer_controls` answer that did not both apply and save sets a status line
  (`lib/console-state.js:641-654`): "`<name>'s limit and locks saved; they apply at the next
  start.`" / "`… apply now, but were not saved: <reason>.`"; a full success clears it
  (`lib/format.js:50-58`).

**Limiter**

- M19. Every mix (musician or Local) has a limiter: off by default, ceiling -24..0 dBFS (default
  0), release 1..1000 ms (default 50); out-of-range/unreadable values clamp or default
  (`lib/limiter.js:19-43`). Ceiling in force = lower of the limiter's and the mix's own ceiling
  (min(cap, limit)), as the note says (`lib/limiter.js:177-206`; `README.md` Master limiter).
- M20. Shown settings = change on its way (`limiterPending`) else status's row `limiter` / status
  `localLimiter` else default (`lib/limiter.js:58-68`). Each control change computes the whole
  limiter from the shown one (`screens/mix-screen.js:287-294`). A drag takes the value at its
  position as is; a key step always moves at least one unit (0.5 dB / 1 ms); key detection =
  slider key held down on that slider (`lib/limiter.js:121-170`;
  `components/master-strip.js:331-342`). After each answer the status is re-read; a set that
  answered but whose status could not be read keeps the answered settings
  (`lib/console-app.js:1013-1040`). Only a refusal is said, in the tools error line
  (`screens/mix-screen.js:295-298`): "`<name>'s limiter was not set.`" / desk message /
  "`<Whose> limiter was not set: <reason>.`"; fallback "The limiter was not set."
  (`lib/limiter.js:217-220`; `lib/console-app.js:1053`). Success text (computed, not shown on the
  Mix screen): "`Ana's limiter is on: peaks are held at −6 dBFS, released over 50 ms.`"
  (`lib/limiter.js:221-229`).

**Effect returns**

- M21. Each mix returns the shared reverb and delay at 0-100 %, engineer only; shown = pending
  change else status (`fxReturns` per row, `localFxReturns`) (`lib/effects.js:208-231`). Sent on
  every move; only a refusal is said (`screens/mix-screen.js:301-311`), e.g. "`Ana's reverb return
  was not set: <reason>.`"; fallback "The return was not set." (`lib/console-app.js:1137-1166`).
  Engine: return level × master level, nothing under master Mute, before the limiter (`README.md`
  Effects).

**Local mix**

- M22. Local mix id `"local"` (`lib/console-state.js:114`). Tab exists only while
  `state.localMix` (an engine runs); it can open with no phones (no "No phones yet" notice)
  (`lib/mix-view.js:261-264`). Same strips but: no lock (`lockable:false`), no cap/limit columns,
  readout = level only, no ceiling note, limiter note uses "the mix's full scale", no mix tools
  (`lib/mix-view.js:114,213-222`; `screens/mix-screen.js:163`). Edits of cap/limit/locks are never
  kept for it (`lib/console-state.js:117,592-599`). Sent with `set_local_mix {mix}`, limiter with
  `set_local_limiter`, returns with `set_local_fx_return` (`lib/bridge.js:76,269,288`). Output
  device choice lives on Network and system, not here (`lib/local-mix.js:1-12`).

**Selection and screen lifecycle**

- M23. `select` only opens a listed mix; the selection survives a stop. When a status drops the
  open musician, the first musician opens and "`<old> was removed; showing <new>'s mix.`" shows
  until a pick or their return (`lib/console-state.js:494-500,511,788-794`;
  `lib/mix-view.js:293-295`).
- M24. Strip elements are keyed (channel strip by first input index, group strip by group id) and
  reordered in place, so a strip survives reorders/hiding even mid-drag
  (`screens/mix-screen.js:80-107,200-216`).
- M25. Leaving the screen drops late answers (no message drawn) and stops meters/subscription
  (`screens/mix-screen.js:160,324-330`).

##### 5. Empty / error / disabled states (verbatim)

Notices (`screens/mix-screen.js:32-50`; chosen by `lib/mix-view.js:243-265` in this priority):

| Kind | When | Title | Text | Action |
| --- | --- | --- | --- | --- |
| disconnected | connection failed | "The console cannot reach the engine" | the connection error | "Try again" (reconnect) |
| connecting | connecting and phase idle | "Connecting to the engine…" | (none) | none |
| stopped | phase idle or stopped | "The engine is stopped" | "Start it to send each musician their mix." | "Start engine" |
| starting | | "Starting the engine…" | "Opening the audio input." | none |
| stopping | | "Stopping the engine…" | "Closing the audio input." | none |
| error | phase error | "The engine is not running" | the engine error | "Try again" (start) |
| no-musicians | no musicians (and Local mix not open) | "No phones yet" | "Musicians appear here when they open BandEar on their phone and join this computer. You can also add a phone by address in " + link "Network and system" (`#/network`) + "." | none |

Message lines (`lib/mix-view.js:267-313`):
- Error (alert): "`A change to <name>'s mix was not applied: <reason>.`" / "`A change to the local
  mix was not applied: <reason>.`" (falls back to the address when unknown).
- Command (alert): "`The engine could not be stopped: <msg>.`", "`The engine could not be started:
  <msg>.`", "`The input device could not be switched: <msg>.`", else "`The engine refused to
  <action>: <msg>.`"
- Audio (status): the audio notice message, e.g. "The saved input Volt was not found, so the
  engine started on the default." (`ui/tests/mix-view.test.js:313`).
- Departed and controls: see M23, M18. Tools outcome/refusal: M15, M20, M21.

Tools panel states (`components/mix-tools.js:183-214,396-410`): Copy panel title "`Copy <name>'s
mix`", legend "Copy to", idle hint "Start the engine to copy a mix."; Presets: "Start the engine
to save or apply presets.", "The presets could not be read." + "Read them again", "No presets
yet.", legend "Apply to". Disabled buttons use `aria-disabled="true"` (opacity 0.55) and stay
focusable; clicks are ignored by the ready checks. Disabled strip controls: opacity 0.5, removed
from tab order (`tabindex=-1`, `aria-disabled`) (`components/slider.js:96-99`;
`components/toggle.js:54-57`).

##### 6. Accessibility

- Strips are `role="group"`; channel strip aria-label = label (or the follows form, M10),
  group strip "`<name> group`", master "Master", limiter "Limiter", returns "Effect returns".
  Captions "MASTER", "GROUP", "LEVEL", "CAP", "LIMIT", "STEREO" are `aria-hidden`.
- Sliders (fader, pan) are the host element: `role="slider"`, `aria-orientation`
  vertical/horizontal, `aria-valuemin 0`, `aria-valuemax 100`, `aria-valuenow`, `aria-valuetext`
  ("62%"; "Centre"/"20% left"), `aria-disabled`, `tabindex 0/-1`, focus ring 2px
  (`components/slider.js:39-49,89-99`). Keys ignored with Alt/Ctrl/Meta (`components/slider.js:137-144`).
- Meters: `role="meter"`, valuemin 0, valuemax 100, valuenow = level, valuetext "`−9 dBFS`" /
  "`0 dBFS`" / "`−∞ dBFS`"; spoken at most every 500 ms with a trailing update
  (`components/meter.js:15,72-78,125-141`; `lib/format.js:66-72`). Labels "`<label> input`",
  "Output left", "Output right".
- Reduction meter: `role="meter"`, valuemin 0, valuemax 12, valuenow one decimal, valuetext
  "`−4.0 dB`", throttled 500 ms with trailing (`components/reduction-meter.js:66-68,103-109`);
  label "Limiter gain reduction".
- Toggles: native button inside, `aria-pressed`, label via `aria-label`; on-state never colour
  alone (fill + lamp bar under the text) (`components/toggle.js:1-12,49-70`).
- Limiter/return range inputs carry `aria-valuetext` = the shown text (`components/master-strip.js:283,314`).
- Tabs: `role="tablist"` "Whose mix"; selection follows focus; roving tabindex (open tab, else the
  first) (`components/musician-tabs.js:65,94-95`). Body `role="tabpanel"` with mix name.
- Live regions: message lines `role="status"` / `role="alert"` (§1.1). The armed delete focuses
  "Keep" (`components/mix-tools.js:513`).
- Link state, group chip, mute/solo: words always present; colour is never the only cue.

##### 7. Tests (`ui/tests`, `node --test`)

| File | Tests | Behaviours pinned |
| --- | --- | --- |
| `mix.test.js` | 20 | default mix; status row -> mix; revision kept/oldest default, never sent; owned tables; edit keys valid/throw; edits overlay; empty -> no request; scalar only; table 0..index; furthest index; mixed tables; integer clamp; engine merge equals view; merge replaces from 0; clamp + absent kept; merge rejects unknown/too long; settleEdits keeps changed; all settled. |
| `mixes.test.js` | 9 | preset name trim/empty/40/unique; presetOf inputs + groups by id; withPreset by group id keeps master/cap/engineer; copy carries shared only; circle detection; Mirror options never a circle + note; leader away note; targets disable followers; answer sentences. |
| `mix-view.test.js` | 16 | top bar device/format/attention/Stop; busy no action, stopped/failed Start; tabs order + link words; channel strip fields; hidden input none, pair = stereo + balance; pair edits both sides, balance -> pans; strip meter = louder side; group by id; muted readout "muted"; pending edits shown; group strips tones/levels; master readout + ceiling note; unknown musician -> empty/null; notices per state; departed/error/command messages; audio fallback message. |
| `mix-tools-logic.test.js` | 11 | Mirror shows pending then desk; desk mirror change replaces pending; screen hands mirror answer to tools; failed/no-musician still frees Mirror; ticks per musician, follower loses tick; open musician pre-ticked unless following; no engine: no copy/save/apply, rename ok; Escape closes + focus back; delete arm/600 ms/10 s disarm/Keep/preset gone; save/rename clear, copy unticks; following strip disabled + words. |
| `mix-tools.test.js` | 11 | (fake bridge + console) preset save/rename/delete with name checks; apply by group id after reorder, never on a follower; copy scope; follower takes leader's mix and later changes, chains; no circle, stop leaves mix; mirror with no engine kept; fake checks mirror keys like desktop; address = IP:port; tools answer in words, strips say who they follow; screen wiring hands answers to tools; refused preset reason, re-read presets. |
| `master-controls.test.js` | 5 | limit sends once at gesture end; re-render mid-drag does not move limit thumb; key press on limit = finished gesture; level/cap on move, Mute on press; Local master shows level/mute, leaves cap/limit alone. |
| `limiter.test.js` | 16 | clamp/defaults; ceiling text with true minus; view state/notes/reduction only while on; change -> whole limiter, key moves ≥ 1 step; slow drag monotonic, ends at release value; reduction from open mix meters; outcome sentences incl. save failure; bridge command names/args; fake keeps limiters across starts; named session keeps/replaces; fake meters show reduction; strip shows pending first; refusal said and status's shown; set kept when status unreadable; settled when answer unreadable; only a held slider key is a key step. |
| `local-mix.test.js` | 33 | (mostly Network screen output select; Mix-relevant:) Local tab last + link words; local mix selectable, kept across statuses, forgotten at run end; edits wait under its id, no cap/limit/lock edits; `mix-changed` local by revision; local strips no lock, master no cap/limit; opens with no phones; `set_local_mix` one at a time; master meter = local peak on Local tab. Others: output options/labels/state words/alerts, fake output behaviour, sessions, API arg names, superseded choices. |

The gallery (`ui/dev/gallery.html:109-247`, driven by `ui/dev/gallery.js:21-48`) shows atoms only:
fader (100 "Unity", 62, 0 "Down", 88 tone accent "Their own channel", 55 tone muted "Muted channel",
40 disabled); meters (4 live inputs, live Output L/R, static levels 0/10/50/80/100 captioned with
their dBFS, horizontal live); pan (0, 40 "20% left", 50 "Centre", 65 "30% right", 100, 35
disabled); toggle (mute off/on, solo off/on, master wide "Mute", disabled); group chip (Drums
teal, Bass blue, Keys violet, Vocals orange, no name -> "No group", Horns invalid). No strip
organisms, reduction meter or mix tools are in the gallery.

##### 8. Custom drawing and gestures to reproduce

- Fader (`components/fader.js:19-135`): 40px wide (compact 32px), fills container height, min
  160px; 6px rail centred, inset by half the 20px thumb top and bottom; fill from the bottom to the
  thumb (`--ml-fader-fill #5b6b7c`, accent, or muted `#3a4452`); thumb = full width minus 6px, 20px
  tall, light `#e6edf3` with shadow and a 2px centre line. Value is linear: position = value/100.
  Pointer fraction = 1 - (y - (top+10)) / (height-20), clamped (`lib/values.js:54-60`). Cursor
  `ns-resize`; disabled opacity 0.5.
- Pan (`components/pan.js:16-126`): 64px wide; 22px track, 6px rail, 1px centre tick (the
  detent mark), 6x14px thumb at `(width-6)*value/100`; readout under it. Drag within ±2 of 50 snaps
  to 50 (`lib/values.js:101-103`); keys stop on the centre when crossing it (`lib/values.js:93-96`).
- Shared gesture (`lib/slider-gesture.js:15-116`): press on the track jumps there (input event);
  press on the thumb keeps the grab offset (no jump); each changed move = `input`; release = one
  `change` only if the value differs from the drag start; key/double-click = `input` + `change`;
  pointer capture on press; cancel/lost capture = release (`components/slider.js:30-36,114-135`).
  Only primary button starts a drag.
- Meter (`components/meter.js:17-123`): 8px wide, min 40px tall, 1px subtle border, radius 3;
  bar scales from the bottom (horizontal: from the left) by the dB fraction; colour band changes
  by zone; redraw coalesced to one per animation frame. No peak hold.
- Reduction meter (`components/reduction-meter.js:17-100`): 8px track radius 4, bar scales from
  the right, one draw per frame; text column 64px (56px in the master strip).
- Toggle (`components/toggle.js:16-70`): min 32x32px (wide 64px); on = filled (mute red, solo
  accent, other light) + 2px lamp bar 3px from the bottom.
- Limiter/return sliders are native range inputs (accent colour, min-height 24px); the strip does
  not reposition a thumb under the pointer and snaps it to the rounded value on release
  (`components/master-strip.js:220-232,309-316,339-353`). Page Up/Down step size on these native
  ranges is the platform default (not set in code; unverified).
- Rendering cadence: store changes -> one render per animation frame; meters ~30/s straight to
  `setLevel`/`setLevels`/`setLimiterReduction`, skipped while the body is hidden
  (`screens/mix-screen.js:226-248`).

#### C. Record screen and virtual soundcheck: parity inventory

Paths are relative to `apps/desktop/`. `rs.js` = `ui/app/screens/record-screen.js`, `rc.js` =
`ui/app/lib/record-controller.js`, `rv.js` = `ui/app/lib/record-view.js`, `rec.js` =
`ui/app/lib/recording.js`, `sc.js` = `ui/app/lib/soundcheck.js`, `scv.js` =
`ui/app/lib/soundcheck-view.js`, `ca.js` = `ui/app/lib/console-app.js`, `br.js` =
`ui/app/lib/bridge.js`, `rt.js` = `ui/app/components/record-track.js`, `chip.js` =
`ui/app/components/rec-chip.js`, `recording.rs` / `soundcheck.rs` =
`src-tauri/src/commands/{recording,soundcheck}.rs`.

Data flow: the screen renders pure view objects (`rv.js`, `scv.js`) from the store; the store is fed
by the `status` event, the `meters` event (~30/s, `br.js:14-20`) and command answers (`ca.js`).
Rendering is coalesced to one per animation frame (`rs.js:520-528`); waveform lanes are drawn at
most once per frame, only while a take records (`rs.js:531-554`).

##### 1. Layout (top to bottom, `rs.js:38-178`)

1. **Header**: `<h1 tabindex="-1">Record</h1>` + "Every input on its own track, written straight
   from the engine while the show runs." (`rs.js:39-42`). Nav description in the shell: "Record
   every input to its own track while the show runs." (`ui/app/shell.js:39`).
2. **Outcome lines** (hidden when empty): ok outcome (`role=status`), failed outcome
   (`role=alert`), take failure (`role=alert`) (`rs.js:43-45`).
3. **Transport card** `section[aria-label="Transport"]` (`rs.js:46-80`), one wrapping flex row:
   - buttons: Back to start (icon), Play/Pause (icon), Stop (icon), Record (dot + word) (`:47-58`);
   - position block: big clock `0:00` (`aria-label="Position"`) over state words (`role=status`)
     (`:59-62`);
   - "Take" name field (`:63-66`); "Add marker" button (`:67`); spacer;
   - summary `<dl>`: Armed / Format / Disk (free + " · about N min left") (`:69-73`);
   - full-width rows: disk warning (`role=alert`) (`:74`), disk problem hint (`:75`), markers box
     "Markers" / "Markers · {take}" + ordered list (`:76-79`).
4. **Locked hint** (between transport and tracks; hidden when empty) (`rs.js:81`).
5. **Tracks card** `aria-labelledby` "Tracks" (`rs.js:82-89`): heading, empty text, then one
   `<ml-record-track>` per row; the rows area has `min-width: 640px` and the card scrolls
   horizontally (`record-screen.css:273-284`).
6. **Lower area** (`rs.js:90-177`), flex-wrap: Takes card (`flex: 999 1 560px`) beside an aside
   (`flex: 1 1 300px`) that drops under the table in a narrow window (`record-screen.css:1-4,
   288-298, 403-409`).
   - **Takes card** (`rs.js:91-114`): head = "Takes" + recordings folder path (mono) + "Refresh";
     listing error (`role=alert`); unreadable note; table (Take | Started | Length | Ended |
     visually-hidden "Actions"); empty text below the table.
   - **Aside** `aria-label="Soundcheck and recording settings"` (`rs.js:115`):
     - **Virtual soundcheck card** (`rs.js:116-150`): heading + badge; "off" block (take select,
       Start soundcheck, hint) or "on" block (take name + `pos / len`, seek slider, Pause / Back to
       live inputs, clock line, dropouts line, failure alert); "Loop the take" checkbox (always
       shown); static explanatory hint.
     - **Settings card** (`rs.js:151-175`): folder form (Folder field, Use this folder, Default
       folder), folder note, notice (`role=status`), Files select, "Also record the local mix
       (stereo)" checkbox, static hint.

Track row (`rt.js:134-144`, CSS `:28-122`): head 260 px wide (round arm button "R" 44 px target,
name bold + source mono, horizontal meter 56 px) | lane (canvas waveform + centred note text). The
local-mix row has a sunken background (`rt.js:40-42`).

Shell status strip (every screen, `ui/index.html:22-24`, `ui/app/shell.js:170-178`): `<ml-rec-chip
data-chip="rec">`, `<ml-rec-chip data-chip="soundcheck">` (+ demo chip prepended, not this area).
The strip is hidden when no chip has data (`rc.js:374-394`).

##### 2. Controls

"busy" = `local.busy !== null`, the transport guard (`rc.js:51-66`). Every status-changing command
is followed by the noted reads. Command names are Tauri commands registered in
`src-tauri/src/lib.rs:315-427,776`; arg names as sent by `br.js:293-344`.

| Control | Verbatim label / aria-label | Range / keyboard | States, disabled + reason text | Tauri command (args), when | Source |
|---|---|---|---|---|---|
| Back to start | icon; `aria-label`/`title` "Back to start" | button | disabled when `!canRewind` (not by busy). Record mode: enabled when not recording/stopping/failed and position > 0 (last take's length until pressed). Soundcheck: enabled when not failed and `positionFrames > 0` | soundcheck: `seek_soundcheck {frame: 0}` via move guard; else none (sets `local.atStart`, position shows 0:00, last take's markers hidden) | `rs.js:48-50,363,564`; `rc.js:256-263`; `rv.js:409,453` |
| Play / Pause | icon swaps play/pause (`data-icon`); `aria-label` = `title` = see states | button | Labels: recording/stopping "Stop the recording before the soundcheck: a take records the live inputs."; playing "Pause the soundcheck"; paused "Play the soundcheck on"; failed "The soundcheck stopped on an error" (disabled); no playable take "Record a take to play it back" (disabled); else `Play "{name}" in place of the live inputs` (enabled only when engine runs and nothing records). Disabled while busy | start: `start_soundcheck {takeFolder, startFrame: 0, looping: local.loopNext}` then `engine_status`; pause: `pause_soundcheck`; resume: `resume_soundcheck` | `rs.js:51,354-361,562`; `rc.js:217-228`; `scv.js:131-153`; `rv.js:460-462` |
| Stop | icon; `aria-label`/`title` "Stop recording", or "Back to live inputs" during a soundcheck | button | disabled when busy or `!canStop`; record mode `canStop` = recording, stopping or failed; soundcheck mode always enabled | soundcheck: `stop_soundcheck` then `engine_status`. Else `stop_recording`, repeated up to 5 times 300 ms apart while answer `finishing` (`ca.js:166-167,1453-1460`), then `engine_status`, then `list_takes` on success; controller then `record_estimate` | `rs.js:52-54,351-353,561`; `rc.js:241-250`; `rv.js:389,407,451` |
| Record | dot + word "Record" / "Recording"; `aria-pressed` = recording | button | disabled when busy or `!canRecord` (`engineRuns && !recording/stopping`; never in soundcheck). Disabled `title`: soundcheck "Go back to the live inputs before recording: a take never records a soundcheck's playback."; else the state word ("Start the engine to record", "Finishing the take's files…"). No title when it can record or is recording. Red filled when recording (`data-recording`) | `start_recording {takeName: trim(field), utcOffsetMinutes: -getTimezoneOffset()}`, then `engine_status`, `list_takes`. Refused client-side (no command) during soundcheck | `rs.js:55-57,342-350,557-559`; `ca.js:1430-1449`; `rv.js:450` |
| Take name | `<label>` "Take"; placeholder "Take" | text, `maxlength=60`, no Enter handling | disabled while recording or stopping (enabled during soundcheck). Blank name sent as "" (desk names it "Take") | sent with Record | `rs.js:63-66,364`; `recording.rs:52-53,563-566` |
| Add marker | "Add marker" | button | disabled when busy or not `recording` (stopping, idle, soundcheck: disabled) | `add_marker {name: ""}` (desk picks next "Marker n", `ca.js:1482`), then `engine_status` | `rs.js:67,362,563`; `ca.js:1483-1501` |
| Arm (track) | text "R"; `aria-label` `Arm {name} ({source})`; `title` "Arm" / "Disarm"; `aria-pressed` | button | disabled while a take records or stops (`locked`), `title` "Arming stays as it is while a take records". Not blocked by busy or soundcheck | emits `track-arm {key}` → `set_recording_settings {settings: {folder, format, includeLocal, armed}}` (whole request, arm applied as a delta when sent; R5), then `record_estimate` | `rt.js:136,155-159,170-181`; `rs.js:590`; `rc.js:306-322`; `ca.js:1380-1402` |
| Arm (local mix) | "R"; `aria-label` "Record the local mix" | button | as above | `set_recording_settings` with `includeLocal: !armed` | `rt.js:174`; `rv.js:123-128` |
| Refresh | "Refresh" | button | always enabled | `list_takes`, then render | `rs.js:95,576-579` |
| Soundcheck (take row) | text "Soundcheck", or "Playing" for the take in the soundcheck (playing, paused or failed); `aria-label` = `title` | button | "Playing": disabled, title "This take plays in place of the live inputs now". Take being recorded: disabled, title "This take is being recorded". Else title `Play "{name}" in place of the live inputs`, disabled when engine not running, a take records, any soundcheck is on, or take has 0 frames. Not disabled by busy (guard drops it) | `start_soundcheck {takeFolder: row.folder, startFrame: 0, looping: local.loopNext}` then `engine_status` | `rs.js:247-254,585-588`; `rc.js:231-235`; `scv.js:159-182` |
| Open folder (take row) | "Open folder"; `aria-label` `Open the folder of {name}` | button | always enabled | `open_take_folder {folder}`; outcome line says result (no busy guard) | `rs.js:240-245,580-584`; `ca.js:1504-1515` |
| Soundcheck take select | `<label>` "Play back as the live inputs" | `<select>`; options `{name} · {m:ss}` | shown only with inputs live; disabled when no playable take; value = target (chosen, else last take, else newest playable) | none; sets `local.chosenTake` (also used by transport Play) | `rs.js:122-123,476-484,565`; `scv.js:30-55` |
| Start soundcheck | "Start soundcheck" | button | disabled when busy or `!canStart` (hint gives the reason, §5) | `start_soundcheck {takeFolder: select value, startFrame: 0, looping: local.loopNext}` | `rs.js:124,485,566-570` |
| Seek slider | `aria-label` "Position in the take"; `aria-valuetext` `{pos} of {len}` | range `min=0`, `max=lengthSeconds`, `step=0.1` s; arrows/drag | disabled when soundcheck failed. While dragged and focused, value not overwritten; `input` shows the dragged time | on `change`: `seek_soundcheck {frame: round(s*48000) clamped to [0, lengthFrames]}`; seeks coalesced (one in flight, latest wins) | `rs.js:132,489-494,573-574`; `rc.js:272-285`; `sc.js:59-62`; `ca.js:1554-1572` |
| Pause / Play / Play again (card) | "Pause" when playing; "Play again" when paused at the end; else "Play" | button | disabled when busy or neither playing nor paused (so disabled when failed) | `pause_soundcheck` / `resume_soundcheck` (same as transport Play) | `rs.js:134,495-496,571`; `scv.js:108-110` |
| Back to live inputs | "Back to live inputs" | button | disabled when busy or soundcheck off | `stop_soundcheck` then `engine_status` | `rs.js:135,497,572` |
| Loop the take | checkbox "Loop the take" | checkbox | checked = soundcheck's `looping` when on, else the choice kept for the next start; disabled only when failed | soundcheck on: `set_soundcheck_loop {looping}` (through guard; dropped while busy, re-render shows truth); off: none (kept as `loopNext`) | `rs.js:141-144,501-502,575`; `rc.js:292-299` |
| Folder field | `<label>` "Folder" | text, mono, no spellcheck; Enter submits the form | disabled until settings read or while a take records/stops. Shows the desk's folder unless focused; a refused path stays typed | (submit) | `rs.js:154-155,446-449`; `rc.js:355-362` |
| Use this folder | "Use this folder" (`type=submit`) | button / Enter in field | disabled as above | `set_recording_settings` with `folder: text` (blank → null = default); if ok `list_takes`; always `record_estimate` after an accepted change | `rs.js:157,591-595`; `rc.js:330-339`; `rec.js:106-127` |
| Default folder | "Default folder" | button | disabled as above or when already default | `set_recording_settings` with `folder: null`, then `list_takes` | `rs.js:158,444,596`; `rc.js:342-349` |
| Files | `<label>` "Files"; options "WAV 24-bit · one file per track" (`int24`, default), "WAV 32-bit float · one file per track" (`float32`) | `<select>` | disabled as folder | `set_recording_settings` with `format` | `rs.js:163-166,454-461,597-599`; `rv.js:39-42` |
| Also record the local mix (stereo) | checkbox label verbatim | checkbox | disabled as folder | `set_recording_settings` with `includeLocal` | `rs.js:167-170,600-602` |
| REC chip | link to `#/record`; `aria-label` = `title` = label (§3) | link | hidden when null | none | `chip.js:120-157`; `rv.js:236-269` |
| SOUNDCHECK chip | link to `#/record`; label (§3) | link | hidden when inputs live | none | `scv.js:188-223` |
| "Mix" link | "Mix" inside empty-tracks text | link `#/mix` | shown only when no track rows | none | `rs.js:84-87` |

On mount: render, `recording_settings` if the store has none, `list_takes`, `record_estimate`
(`rs.js:604-611`). Unmount cancels pending frames and unsubscribes (`rs.js:613-619`).

##### 3. Displayed values (verbatim formats)

- **Clock** `formatDuration`: `m:ss`, `h:mm:ss` from 1 h; floor, negatives → 0 ("0:07", "3:25",
  "1:02:03") (`rec.js:130-136`).
- **Transport position**: recording/stopping/failed → recorder's seconds; else last take's length
  until Back to start (then 0:00); soundcheck → `positionSeconds` (`rv.js:417-433,410`).
- **State words** (`data-kind` = recorder state or "soundcheck") (`rv.js:394-398,434-443`):
  "Ready to record" | `Recording · {take}` (take = folder name without `YYYY-MM-DD_HHMMSS_`,
  `rv.js:224-229`) | "Finishing the take's files…" | "The take stopped on an error" | "Start the
  engine to record" | `Soundcheck · "{name}" plays in place of the inputs` | `Soundcheck ·
  "{name}" is paused` | "The soundcheck stopped on an error" ("the take" when no name).
- **Armed**: "Nothing armed" | "Local mix only" | `{n} track(s)` | `{n} track(s) + local mix`
  (`rv.js:165-170`; counts armed input rows, a pair = 1).
- **Format**: `WAV · 48 kHz · 24-bit` / `WAV · 48 kHz · 32-bit float` (`rv.js:44,459`).
- **Disk** (`rv.js:315-327`): free = "Free space unknown" or `{size} free`, size in decimal units
  B/KB/MB/GB/TB, 1 decimal below 100 ("214 GB", "1.3 GB", "705 MB", `rv.js:272-282`); left =
  ` · about {N} min left` under 120 min, else ` · about {floor(N/60)} h left` (`rv.js:285-287`,
  `rs.js:389`); source: recorder's `freeBytes`/`minutesLeft` while recording/stopping, else the
  `record_estimate` answer. Problem (estimate only): backend text sentence-cased + "." (e.g. "The
  engine is not running: the tracks depend on the input device.", `recording.rs:641-645`).
- **Disk warning** (`rv.js:33-37,290-309`): ≥ 30 min none. 10-29: level "low", `About {n}
  minute(s) of recording space is/are left on this disk.` < 10: level "critical", `Only about {n}
  minute(s) of recording space is/are left: free some disk space or choose another folder before
  the take fills it.`; ≤ 0: `No recording space is left: free some disk space or choose another
  folder before the take fills it.` (verb "is" only when n = 1).
- **Markers** (`rs.js:370-385`, `rv.js:368-374`): title "Markers" or `Markers · {take}`; each item
  `{m:ss} {name}`; box hidden with no markers; recording's markers live, else last take's until
  Back to start; none during soundcheck.
- **Takes table** row (`rv.js:176-208`, `rs.js:215-264`), newest first as listed:
  - Take: name (red dot when `ended == "recording"`) + detail line joined by " · ": tracks summary
    (as Armed, from the take's tracks; local track counted separately), "No markers" / `{n}
    marker(s)`, and `{n} frame(s) missing` when `droppedFrames > 0`.
  - Started: `YYYY-MM-DD HH:MM` in the window's local offset, or "Unknown" (`rv.js:134-145`).
  - Length: clock (mono); the take being recorded uses the live status seconds.
  - Ended badge (`rv.js:147-162`): Stopped (ok), Engine stopped (warn), Disk error (error),
    Interrupted (warn), Recording (live, red), other "Ended" (ok); badge `title` = sentence:
    "stopped" | "ended when the engine stopped" | `stopped by a disk error: {reason}` |
    "recording now" | "interrupted before it was finished; its files hold what was written" |
    "ended" (`rec.js:161-176`).
  - Actions: Soundcheck/Playing, Open folder.
  - Head shows the listed folder path (`rs.js:412`). Unreadable note: `{n} take(s) could not be
    read and is/are not listed.` (`rec.js:190-196`).
- **Soundcheck card** (`scv.js:91-125`): badge Off / Playing / Paused / Failed / "At the end"
  (paused with position ≥ length); badge colour kind off→off, paused→warn, failed→error, else ok
  (`rs.js:473`). Time `{pos} / {len}`. Clock line: "Paced by the local output." or "Paced by a
  timer: no local output plays.". Dropouts: "No dropouts." or `{round(frames*1000/48000)} ms of
  silence where the disk fell behind.` Failure: `The soundcheck stopped playing: {reason}.`
- **Settings** (`rv.js:500-513`, `rs.js:450-453`): folder note "This is the default folder." or
  `The default folder is {defaultFolder}.`; notice = backend text, e.g. `Your Music folder could
  not be found, so takes are recorded in {folder} unless you choose another folder.`
  (`recording.rs:112-127`).
- **Track row** (`rv.js:73-115`, `rt.js:16-26`): key `in-{first input}` / `local`; name = channel
  name or `CH 3` / `CH 3-4`; source `IN 3` / `IN 3–4 · stereo` (en dash) + ` · hidden` when all
  sides hidden; local: "Local mix", "This computer's mix · stereo". Lane note: off "Not armed: this
  input is not being recorded" / local "Not recorded: arm it to record the local mix as a stereo
  file"; armed "Armed: records with the next take" / local "Armed: the local mix records with the
  next take"; recording: no note, canvas waveform. Meter label `{name} level`.
- **Waveform lane** (`rt.js:195-227`): mirrored bars, newest at right, one slot per capacity
  (600 peaks ≈ 20 s at 30 meters/s, `rc.js:36-37`), bar half-height = `meterFraction(level)` ×
  (mid-1), min 0.5 px, alpha 0.8; colour `--ml-group-teal-text` (inputs) / `--ml-accent` (local),
  fallback `#7fe0d0`; device-pixel-ratio aware. Lane state "off"/"armed"/"recording"; recording
  also once it kept peaks after the take (`rc.js:198-204`).
- **Track meter level**: input row = max of its sides' `channels` peaks; local row = max(local.left,
  local.right) (`rc.js:176-187`, `ui/app/lib/mix-view.js:173-175`).
- **REC chip** (`rv.js:236-269`): word "REC" / "Finishing" / "REC stopped", time clock, detail
  `{n} track(s)`; label `Recording, {spoken}, {n} track(s). Open Record` | `Finishing the take's
  files after {spoken}. Open Record` | `The recording stopped: {reason}. Open Record`; spoken =
  "1 hour 2 minutes 5 seconds" style, "0 seconds" (`rv.js:211-221`). Dot pulses when recording.
- **SOUNDCHECK chip** (`scv.js:188-223`): word "SOUNDCHECK" / "SOUNDCHECK stopped"; detail name, or
  `{name} · paused`; label `Soundcheck: "{name}" plays in place of the live inputs, at {n}
  second(s). Open Record` | `Soundcheck: "{name}" is paused in place of the live inputs, at {n}
  second(s). Open Record` | `The soundcheck stopped playing: {reason}. Open Record`.

##### 4. Rules

- **R1 Track rows** = recorded units of the running device: input linked to the next (`link`) is
  one stereo row `[i, i+1]`; hidden inputs record too; no rows while no engine runs
  (`rec.js:55-67`, `rv.js:61-97`). The local-mix row is appended only when settings are known and
  there is at least one input row (`rs.js:399-400`, `rv.js:103-115`).
- **R2 Linked pairs**: either side armed arms the pair (`rec.js:78-81`); disarming one side
  disarms both (`rec.js:87-97`); arming sends the row's first input (`rv.js:127`).
- **R3 Local mix track**: armed by settings `includeLocal`; its arm toggles `includeLocal`
  (`rv.js:123-126`).
- **R4 Settings request is whole**: `{folder (null = default), format, includeLocal, armed}`, built
  from the last answered settings; blank folder → null (`rec.js:106-127`).
- **R5 Settings lane**: changes go one at a time, each built when its turn comes on the settings the
  desk last answered; a refused change rides in no later request (`ca.js:1376-1402`). Arm presses
  are deltas, so quick presses on two rows both land; a second press on the same row before the
  first is answered toggles what the first asked (`rc.js:306-322`).
- **R6 Settings failures** never reject: said as outcome `The recording settings were not changed:
  {reason}.`; screen re-renders (`rc.js:116-126`, `ca.js:1389-1393`). Success: "The recording
  settings were saved." + `record_estimate` (`ca.js:1395-1397`).
- **R7 Lock while recording**: during recording or stopping, arm buttons, folder field/buttons,
  Files and local-mix checkbox are disabled; locked hint "Arming and the settings stay as they are
  while a take records; they apply from the next take." (`rv.js:47-48,488-493,500-513`,
  `rs.js:396-397,434-444`). Settings are NOT locked during a soundcheck; there the hint is the
  soundcheck-blocks-recording sentence (R18).
- **R8 Transport guard**: one transport command at a time (start, stop, mark, play, soundcheck,
  loop); its buttons are disabled until answered; another pressed meanwhile is not run
  (`rc.js:53-79`). Moves (Back to start, seek) share the guard as "move": they are dropped while
  another command is in flight, but do not block each other (`rc.js:84-102`).
- **R9 Start**: client refuses during soundcheck with "Go back to the live inputs before
  recording: a take never records a soundcheck's playback." (no command) (`ca.js:1431-1434`).
  Name trimmed; folder named in local time via `utcOffsetMinutes` (`ca.js:1435-1438`). Success
  outcome: `Recording "{name}" on {n} track(s) in {folder}.` + ` CH 9, CH 10 is/are armed but not
  on this device, so not recorded.` when inputs were skipped (`rec.js:150-158`); name "Take" when
  blank. Failure: `The recording did not start: {reason}.` Backend reasons (`recording.rs:522-566`):
  "the engine is not running: start it to record", "a take is already recording: stop it first",
  "nothing is armed: arm at least one input, or record the local mix", "nothing is armed on this
  device: {CH list} are past its {n} inputs; arm one of its inputs, or record the local mix",
  "the recordings folder {path} could not be created: {error}", and the engine's soundcheck
  refusal "a soundcheck is playing a take: go back to the live inputs before recording (a take
  never records playback)" (`soundcheck.rs:108-111`).
- **R10 Stop**: retries while `finishing` (5 × 300 ms); still finishing → failed outcome "The take's
  files are still being finished, so it has not stopped yet. Press Stop again in a moment." (state
  stays "stopping", Stop can be pressed again) (`ca.js:1453-1477`). Success: `"{name}" {end
  sentence}: {m:ss} on {n} track(s).` + ` {n} frame(s) the disk could not keep up with are
  missing.` when frames dropped (`rec.js:179-187`). Failure: `The recording was not stopped:
  {reason}.` (backend: "the engine is not running, so nothing is recording"). Stop is also enabled
  for a failed take (`rv.js:451`).
- **R11 Marker**: only while state is "recording"; blank name → desk's next "Marker n"; outcome
  `Marked "{name}" at {m:ss}.` / `The marker was not added: {reason}.` (`ca.js:1482-1501`).
- **R12 New take**: when the status shows a new take folder, all lane histories clear and Back to
  start resets (`rc.js:159-170`); lanes keep peaks only while state is "recording" and only for
  armed rows (`rc.js:147-149,176-187`).
- **R13 Take states**: `ended` ∈ stopped, engineStopped, diskError, recording, interrupted (§3
  labels). A take whose `take.json` never got its end is listed as interrupted, with the frames its
  files hold (`recording.rs:22-26`). Disk error: only the take stops; failed take shows "The take
  stopped on an error" + alert `The take stopped recording: {reason}.` (`rv.js:476-479`), REC chip
  "REC stopped".
- **R14 Device switch refusal**: while recording/stopping, a device switch or driver change is
  refused before any command with "Stop the recording before switching the input device or its
  driver: a take never mixes two devices." (`rec.js:37-39`, `ca.js:709-713,744-747`); during
  soundcheck: "Go back to the live inputs before switching the input device or its driver: a
  soundcheck plays a take in place of the inputs." (`sc.js:36-38`, `ca.js:714-719,748-751`). Shown
  as command error "switch" on the Network screen (not this screen).
- **R15 Playable takes**: listed takes with `ended != "recording"` and `frames > 0`
  (interrupted/disk-error takes included) (`scv.js:30-38`). Play's target: chosen in the card if
  playable, else the status's `lastTake`, else newest playable (`scv.js:44-55`).
- **R16 Soundcheck start**: refused client-side while a take records/stops with "Stop the
  recording before the soundcheck: a take records the live inputs." (`ca.js:1543-1546`); sends
  `startFrame` 0 and `looping` = the card's loop choice; then reads status. Outcome
  `Soundcheck: "{name}" plays in place of the live inputs from {m:ss}[, looping].`
  (`sc.js:69-73`). Backend refusals (`soundcheck.rs:96-160`, prefixed "The soundcheck did not
  start: "): engine stopping, recording, already playing, demo source, take unreadable, file
  unreadable, sample rate (only 48000 Hz), layout ("the take does not fit this device's inputs
  now: {reason}; play it on the device and with the linked pairs it was recorded with"), empty
  ("the take has no input audio to play"), unavailable; not a take folder of the recordings
  folder (R22).
- **R17 Soundcheck transport**: Play = start/pause/resume (`scv.js:131-153`); outcomes "Soundcheck
  paused at {m:ss}." / "Soundcheck playing from {m:ss}." / "Soundcheck moved to {m:ss}." / "The
  soundcheck starts the take again at its end." / "The soundcheck stops at the end of the take." /
  "Back to the live inputs." (`sc.js:65-89`); failures `{lead}: {reason}.` with leads "The
  soundcheck did not start" / "The soundcheck was not paused" / "The soundcheck did not play on" /
  "The soundcheck did not move" / "The soundcheck's loop was not changed" / "The soundcheck did
  not stop" (`sc.js:92-99`,
  `ca.js:1521-1540`). The command's answer is applied to the status at once for the shown run.
- **R18 During soundcheck**: Record disabled (title above), Add marker disabled, Stop becomes "Back
  to live inputs", Back to start seeks to 0, markers hidden, take name editable, locked hint "Go
  back to the live inputs before recording: a take never records a soundcheck's playback."
  (`rv.js:392-415,488-493`). Only one take plays; every row's Soundcheck disabled (`scv.js:169-174`).
  SOUNDCHECK chip on every screen.
- **R19 End of take**: without loop the engine pauses at the end ("At the end"); resume plays from
  0:00 ("Play again"). A seek from the end plays at once; a seek while paused stays paused; a seek
  to the end while paused stays paused (pinned by `ui/tests/soundcheck.test.js:206-239`; engine
  behaviour).
- **R20 Loop**: with a soundcheck on, toggles via `set_soundcheck_loop`; with inputs live, kept
  locally for the next start (Play, Start soundcheck, row Soundcheck) (`rc.js:223,233,292-299`).
- **R21 Failed soundcheck** (read error): badge Failed, failure alert, slider/Pause/Loop/Play/Back
  to start disabled; only Back to live inputs is left (`scv.js:94-119`, `rv.js:409`).
- **R22 Folder rules (backend)** (`recording.rs:425-450`): must be absolute ("the recordings
  folder must be a whole path, such as D:\Recordings, and `{text}` is not"), no `..` ("the
  recordings folder must not go up with `..`: `{text}`"), not inside the settings folder ("the
  recordings folder cannot be in BandEar's own settings folder ({config}): `{text}`"); typed
  default = default. `open_take_folder` and `start_soundcheck` accept only an absolute folder with
  a `take.json` directly inside the recordings folder: "{folder} is not a take folder in the
  recordings folder {recordings}" (`recording.rs:887-907`).
- **R23 Folder field**: shows the desk's folder unless the field has focus; refused text stays to
  be corrected; a kept folder is shown as the desk answers and its takes are listed
  (`rc.js:330-362`).
- **R24 Disk estimate refresh**: on connect, on engine start, after every accepted settings
  change, after Stop, on screen mount (`ca.js:469,603-608,1396`, `rc.js:247`, `rs.js:611`); a
  failed read forgets it; a run's end forgets it (`ui/app/lib/console-state.js:540`).
- **R25 Takes listing**: on mount, Refresh, after start, after a successful stop, after a folder
  change; a failed listing keeps the last list and says `The takes could not be listed:
  {reason}.` (`ca.js:1418-1426`, `rv.js:480-482`). Settings and takes survive a run's end.

##### 5. Empty / error / disabled states (verbatim)

- No tracks (no engine): "The tracks are the running device's inputs: start the engine from the
  Mix screen to see and arm them." ("Mix" is a link to `#/mix`) (`rs.js:84-87`).
- Takes: list unknown → "Listing the takes…" (nothing if the listing failed); folder missing → "The
  recordings folder does not exist yet: the first take makes it."; empty → "No takes in this
  folder yet." (`rs.js:416-424`).
- Takes listing error (alert): `The takes could not be listed: {reason}.`; backend reason e.g. "the
  recordings folder {path} could not be read: {error}" (`recording.rs:689-694`).
- Soundcheck start hint (`scv.js:19-23,58-66`), first match: "Start the engine to play a take in
  place of the inputs." | "Stop the recording before the soundcheck: a take records the live
  inputs." | "Record a take first: its tracks then play back here in place of the inputs, so every
  mix and the phones can be set with nobody playing."
- Soundcheck card static text: "Each track plays into the input it was recorded from, in place of
  the interface, so every mix, the local output and the phones can be set with nobody playing. The
  interface stays open: going back to the live inputs is instant." (`rs.js:145-149`).
- Settings static text: "Tracks are written before any processing, and a linked pair stays one
  stereo file. A file that passes 4 GB switches to RF64 on its own." (`rs.js:171-174`).
- Settings unknown (not read yet): all settings controls disabled; no local-mix row.
- Open folder outcome: `Opened {folder}.` / `The take folder was not opened: {reason}.`
  (`ca.js:1504-1515`); reasons: R22 refusal or "the file manager could not be started: {error}"
  (`recording.rs:205-212`).
- Outcome placement: ok → `role=status` line, not ok → `role=alert` line; only the last outcome is
  shown (store `recordingOutcome`, `ui/app/lib/console-state.js:55,274-277`).
- Disabled-reason surfaces: Record `title`, Play `aria-label`/`title`, row Soundcheck `title`,
  arm `title`, locked hint, soundcheck start hint (see §2).

##### 6. Accessibility

- Landmarks/labels: `section[aria-label="Transport"]`; Tracks/Takes/Virtual soundcheck/Settings
  sections `aria-labelledby` their h2; aside `aria-label="Soundcheck and recording settings"`
  (`rs.js:46,82,91,115,116,151`).
- `h1` "Record" has `tabindex="-1"`; the shell focuses it after navigation
  (`ui/app/shell.js:107-110`).
- Live regions: outcome `role=status`; outcome error, take failure, disk warning, takes error,
  soundcheck failure `role=alert`; transport state `role=status`; folder notice `role=status`
  (`rs.js:43-45,61,74,97,139,162`). Disk problem and locked hint are plain text.
- Names: icon buttons have `aria-label` (+ `title`); Record `aria-pressed`; arm `aria-pressed` and
  `aria-label` `Arm {name} ({source})` / "Record the local mix"; row buttons `aria-label`s; seek
  `aria-label` + `aria-valuetext`; position div `aria-label="Position"` (no role); table `th
  scope=col`, Actions header visually hidden; all SVG icons, dots and the canvas `aria-hidden`.
- Meter: `role=meter`, `aria-valuemin 0`, `aria-valuemax 100`, `aria-valuenow`, `aria-valuetext` in
  dB, accessible name `{name} level` (`ui/app/components/meter.js:73-75,139-140`, `rt.js:184`).
- Chips: one link whose `aria-label`/`title` say everything; inner spans `aria-hidden`
  (`chip.js:120-125,150-153`).
- Keyboard: no custom shortcuts in this area (no keydown handlers). Native: buttons
  Enter/Space; Enter in the folder field submits (form); slider arrows step 0.1 s (each step fires
  input+change → coalesced seek); selects/checkboxes native. Focus ring 2 px `--ml-focus`
  (`rt.js:75-78`, `chip.js:36-39`). Arm target size `--ml-target` (`rt.js:52-56`).

##### 7. Tests (74 total; `node --test`, all use `node:test`; fake engine `ui/app/lib/fake-bridge.js`)

- `ui/tests/record-controller.test.js` (13): transport guard drops a second command; lanes keep
  armed peaks only while recording and clear on a new take; local lane = louder local side; two
  quick arm presses on two rows land; second press on same row toggles back; failing
  settings/listing re-renders and says why; refused folder stays typed, kept one shown and listed;
  Play starts chosen take / pauses / resumes, Stop goes live; Back to start and slider seek, loop
  chosen before start loops; move/loop during Stop not sent (stop's words kept); moves hold other
  commands but not each other; REC chip strip shown for recording/stopping/failed only; SOUNDCHECK
  chip shown on every screen during soundcheck.
- `ui/tests/record-view.test.js` (24): unit rows (pair = stereo row); unnamed pair "CH a-b", hidden
  input records; no rows without engine; armChange pair-as-one and local; local row armed by
  settings; local-time start; take row fields; every end label + sentence; live take length from
  status; REC chip words/labels; spokenDuration; formatBytes; estimate free/minutes; recorder's
  disk while recording; low/critical thresholds; estimate problem, nothing guessed; peak history
  bounded/ordered (2 tests); takeNameOf; transport can record/stop/mark + state words; position =
  last take length until Back to start; float + local summary; recordMessages (outcome, failed
  take, listing error); settingsView.
- `ui/tests/recording.test.js` (19): engine api arg names for recording commands; pair arming
  rules; whole settings request, default folder null; arm delta on built request;
  duration/start/stop/unreadable words; store keeps settings/takes past run end, estimate not;
  settings read on connect and two quick changes land; change after a refused one builds on
  accepted; quick arm presses compose, refused one dropped; estimate read on connect and engine
  start; estimate without engine says why; fake refuses what the desk refuses; take records,
  grows, takes markers, stops into list; stop retries while finishing then gives up in words;
  start refused with no engine / nothing armed / inputs past device; recording blocks device switch
  and driver change; engine stop ends the take; takes newest first, unreadable said, only a listed
  take opens; estimate follows settings.
- `ui/tests/soundcheck.test.js` (11): engine api arg names for soundcheck commands; shapes, frames
  (`frameAt`), words; store applies status soundcheck and command answers for the shown run;
  start/advance/seek/pause/end-pauses/Play-again-from-0/loop/back-live with exact outcome strings;
  seek from end plays, seek while paused stays paused; start from frame looping, paced by local
  output; during soundcheck recording/switch/driver change refused (audio setup disabled with
  reason); recording blocks soundcheck start; fake refusals in desk words (wrong folder, layout,
  local-only take, 44.1 kHz…); read failure refuses transport but still goes back live; engine stop
  ends soundcheck, rapid seeks send first and last only.
- `ui/tests/soundcheck-view.test.js` (7): playable takes exclude the one recording, Play target;
  card offers takes and start hints; playing card position/clock/dropouts; paused/at-end/failed
  card words and allowed actions; SOUNDCHECK chip words/labels; transport Play/Stop/Back to start
  in soundcheck and labels; row Soundcheck enablement and "Playing".

Note: the record-controller, recording and soundcheck suites run against `createConsoleApp` over the fake
bridge with a movable clock; a Rust port needs an equivalent fake engine (the backend already has
`NotedOpener` and fake `RecordEngine` test doubles in `recording.rs` tests, e.g. `:996-1045,1762`).

##### 8. Native integrations needed

1. **Reveal take folder in the file manager**: no Tauri plugin. The `open_take_folder` command
   (`src-tauri/src/lib.rs:373-377`) validates the folder (R22) then calls `SystemFileManager`
   (`recording.rs:197-240`): `explorer.exe "<folder>"` (raw quoted arg) on Windows, `open <folder>`
   on macOS, `xdg-open <folder>` elsewhere, spawned directly, never through a shell. These modules
   import no Tauri types, so GPUI can call `commands::recording::open_take_folder` (or the
   `FolderOpener` trait) directly.
2. **Recordings folder choice**: there is NO folder-picker dialog. The folder is a typed absolute
   path in a text field, validated by `set_recording_settings` (R22), plus a "Default folder"
   button (`rs.js:153-160`). Capabilities grant only `core:default` and
   `core:window:allow-set-title` (`src-tauri/capabilities/default.json`), so no dialog/opener
   plugin exists. A GPUI port needs a text input with focus-aware value sync (R23); a native picker
   would be an addition, not parity.
3. **Confirmations**: none in this area (no confirm dialogs for stop, soundcheck or folder change).
4. **Local time offset**: `utcOffsetMinutes = -Date.getTimezoneOffset()` for `start_recording`
   and for the takes' "Started" column (`ca.js:1438`, `rs.js:415`); needs a local-offset source.
5. **Rendering**: per-frame render coalescing, a 600-slot ring buffer per armed track fed by
   `meters` (~30/s), and a DPR-aware waveform canvas (`rt.js:195-227`); the chip strip is global
   across screens; links `#/record` and `#/mix` are in-app navigation.
6. **Gallery**: `ui/dev/gallery.js`/`gallery.html` have NO entries for `ml-record-track` or
   `ml-rec-chip` (neither is in `ui/app/components/index.js`); there is no gallery spec to port
   for this area.

#### D. Channels and groups, Processing, Effects: parity inventory

Paths are relative to `apps/desktop/ui/app/` unless they start with `ui/`, `src-tauri/` or
`README.md` (all under `apps/desktop/`). Quoted strings are verbatim. Typography the formats rely
on: real minus `−` (U+2212) in every dB value, en dash `–` (U+2013) and middle dot `·` (U+00B7) in
strip choices ("CH 3–4 · Keys"), but an ASCII hyphen in group choices ("CH 2-3 OH L"). Channels are
1-based at the desk and in the Processing/Effects code; status channel indexes are 0-based.

##### 0. Shared plumbing (console-app.js and bridge.js)

| App method | Tauri command (args) | Timing / queueing | Source |
| --- | --- | --- | --- |
| `editChannels(change)` | `channel_table` (read whole table, inputs past the device included), then `set_channels {channels}`, then `engine_status` | One desk edit or session load at a time (`inTurn`); whole list replaced | lib/console-app.js:796-834, lib/bridge.js:107,114 |
| `editGroups(change)` | `set_groups {groups}` (1-based channels, no `id` for a new group), then `engine_status` | Same lane; base is the last `set_groups` answer (`keptGroups`, 1-based) else the status's groups (0-based, +1) else refused | lib/console-app.js:788,836-848 |
| `editProcessing(ch, strip)` | `set_channel_processing {channel, settings}` | Per-channel coalescer: one in flight, latest wins; edit shown at once (`processingPending`); held back during a session load | lib/console-app.js:253-276,1290-1304 |
| `resetChannelProcessing(ch)` | `reset_channel_processing {channel}` | `inTurn`, after pending strip edits of the channel and its pair partner | lib/console-app.js:1311-1328,1770-1775 |
| `copyChannelProcessing(from,to)` | `copy_channel_processing {from, to}` | As reset, touching `from` + `to` + partners | lib/console-app.js:1777-1782 |
| `applyStripPreset(name,ch)` | `apply_strip_preset {name, channel}` | As reset | lib/console-app.js:1795-1800 |
| `saveStripPreset(name,s)` / `deleteStripPreset(name)` | `save_strip_preset {name, settings}` / `delete_strip_preset {name}` | Direct; answer's `presets` replace the store's | lib/console-app.js:1331-1342,1786-1793 |
| `loadProcessing()` / `loadStripPresets()` | `processing` / `strip_presets` | On connect, after a session load, on "Read again" / on Processing mount if null | lib/console-app.js:1266-1283 |
| `loadEqResponse(ch, eq)` | `eq_response {settings: eq, points: null}` (128 points) | Debounced by the editor (120 ms) | lib/console-app.js:1345-1357, lib/bridge.js:398 |
| `editChannelSend(ch, effect, level)` | `set_channel_send {channel, effect, level}` | Coalescer per `"ch:effect"`; level clamped to whole 0-100 | lib/console-app.js:1063-1095 |
| `editEffect(effect, settings)` | `set_effect {effect, settings}` (whole effect) | Coalescer per effect; shown at once (`effectsPending`) | lib/console-app.js:1098-1125 |
| `onMeters(fn)` | event `meters` (~30/s): `{channels, processed, gateReductionDb, compressorReductionDb, ...}` | Bypasses the store | lib/bridge.js:14-22 |

All of these are registered in `src-tauri/src/lib.rs:737-804` (`engine_status` 739, `set_groups`
750, `set_channels` 751, `channel_table` 752, `set_channel_send` 791, `set_effect` 792,
`processing`…`eq_response` 795-804). Failure wording is `sentenceWithReason(lead, reason)` =
`"<lead>: <reason without trailing dots>."` or `"<lead>."` (lib/format.js:12-17).

---

##### 1. Channels and groups (screens/channels-screen.js, route `#/channels`)

###### 1.1 Layout (channels-screen.js:13-34, channels-screen.css)
1. Header: `h1` "Channels and groups" (tabindex -1, focused after navigation), paragraph "Names,
   instruments and groups reach every phone at once. A hidden input is shown nowhere and no one
   hears it."
2. Note (only while no status): see 1.5.
3. Messages: outcome line (`role="status"`) then failure line (`role="alert"`); empty lines take no
   space (css:41-50).
4. Body (hidden while no status), a wrapping flex row: channel table section (`aria-label
   "Channels"`, flex 999 1 640px) beside the groups aside (`aria-label "Groups"`, flex 1 1 280px),
   which drops below the table when narrow (css:52-67).
5. Table columns, in order (components/channel-table.js:217-227): "Input", "Name", "Instrument",
   "Level", "Stereo", "Group", "Shown" (title "A hidden input is shown nowhere and no one hears it"),
   "Processing". Header text uppercase, 0.12em tracking. Table scrolls horizontally in its frame.
6. Groups panel (components/group-cards.js:208-216): head row `h2` "Groups", note "up to 16",
   spacer, primary button "New group"; full hint; empty hint; then one card per group.
7. Card (group-cards.js:274-291): top row (chip, count, spacer, Edit); member tags; "No channels
   yet."; collapsible editor: Name field, fieldset of input checkboxes (max-height 260px, scrolls),
   hint, actions row (delete slot left, "Done" right).

###### 1.2 Controls

| Control | Verbatim label / aria-label | Range / keys | States | Command, when | Source |
| --- | --- | --- | --- | --- | --- |
| Name field (per row) | aria-label "Name of CH n"; placeholder "CH n"; title "Shown on every phone and on the Mix. Enter saves, Escape reverts" | text, maxlength 32; Enter commits (blurs); Escape restores saved name and blurs; blur commits | Never overwritten while focused; emits only when trimmed value differs from saved | `channel-rename {index,name}` → `channel_table` + `set_channels` + `engine_status`, on commit | channel-table.js:25,259,292-305,321-329 |
| Instrument select | aria-label "Instrument of <label>" (label = name or "CH n"); kind icon beside | 11 options (CH1) | Shows "Other" for an unknown kind | `channel-kind {index,kind}` → set_channels, on change | :260,306-308,331-341 |
| Stereo toggle | text "Stereo" + stereo icon; aria-label "Stereo pair CH a and CH b" or "Stereo pair CH n and the next input"; `aria-pressed` = linked | click | Unavailable: `aria-disabled="true"`, dashed, stays focusable, click ignored, `aria-describedby` = reason paragraph; title = reason, else "Undo the pair: two strips again" (linked) / "Pair with the next input: one strip whose pan is a balance" | `channel-link {index, linked: !linked}` → set_channels, on click | :264,309-313,345-359 |
| Shown checkbox | aria-label "Show CH n to musicians" or "Show CH n and CH m, its pair to musicians"; word "Shown"/"Hidden" | checkbox | title "Both inputs of a pair are shown or hidden together" (pair) / "A hidden input is shown nowhere and no one hears it"; held as clicked until answered (CH11) | `channel-shown {index, shown}` → set_channels, on change | :270,314-316,365-373 |
| Processing link | text "Processing"; aria-label "Processing of <label>, on" / "…, off" / "…, not read yet"; word "On"/"Off"/"" | link | title "Open this input's high-pass, equaliser, gate and compressor" | navigates to `#/processing/<first side>`; no command | :271,375-382; lib/processing-view.js:416-427 |
| New group | "New group" | click | `aria-disabled="true"` (dimmed 0.5, click ignored) at 16 groups | `group-add` → `set_groups` with "Group N" appended | group-cards.js:212,224-228,242 |
| Edit / Close (per card) | text "Edit"/"Close"; aria-label "Edit <name>"/"Close <name>"; `aria-expanded`, `aria-controls` = editor id | click | Opening focuses the Name field; open state survives re-render and other cards being added/removed (kept by group id) | none | :279,310-313,344-360,369 |
| Group name field | label "Name"; aria-label "Name of the <name> group" | text, no maxlength; Enter commits; Escape reverts (stops propagation); blur commits | Not overwritten while focused | `group-rename {id,name}` → set_groups, on commit if trimmed differs | :284,318-333,381-386 |
| Input choice (per unit) | text "CH 1 Kick" / "CH 2-3 OH L" / "CH 4"; note "in <Group>", "hidden" joined by ", "; fieldset legend "Inputs in <name>" | checkbox | Held as clicked until answered | `group-member {id,key,numbers,member}` → set_groups, on change | :392-437 |
| Delete group | "Delete group" (danger), aria-label "Delete the <name> group" | click arms | see CH20 | none until confirmed | :439-456 |
| Keep / Delete (armed) | "Keep", question "Delete <name>? Its inputs keep their own faders.", "Delete" (danger) | Escape anywhere in the card disarms | "Delete" `aria-disabled="true"` for 600 ms after arming; Keep takes the former button's place and gets focus | Confirmed Delete → `group-delete {id}` → set_groups without it | :457-509 |
| Done | "Done" | click | Closes editor, focuses Edit; closing disarms a pending delete | none | :290,314-317,344-353 |

###### 1.3 Displayed values and formats
- Input cell: "CH n", monospace; a hidden row gets a 3px muted inset bar on the left of that cell
  (channel-table.js:64-73).
- Level: horizontal `ml-meter` per row, accessible name "<label> input level", fed
  `meters.channels[index]` (0-100 linear peak); 60 dB scale, amber from −6 dBFS, red at 100
  (components/meter.js:1-9); spoken value "−12 dBFS", "−∞ dBFS" (lib/format.js:66-72), at most
  twice a second. Meters update only while the body is shown (channels-screen.js:90-94).
- Stereo text: "Linked with CH n" or "Mono"; reason paragraph under it (max-width 22em).
- Group chip: group name or "No group"; tone by group index (CH13); invalid adds "!" and hidden
  ", input missing", title "This group names an input the current device does not have"
  (components/group-chip.js:86,99-106).
- Card count: "1 channel" / "N channels" (lib/channels-view.js:302). Member tags: "Kick", "OH L
  (stereo)", "CH 9 (not on this device)" (:253-256,277-282).
- Outcome success (lib/format.js:23-43): "Channel details applied to every phone." / "Channel details
  saved; they apply at the next start." / "Channel details applied to every phone, but not saved:
  <err>."; groups: "Groups applied to every phone." / "Groups saved; they apply at the next start."
  / "Groups applied to every phone, but not saved: <err>."

###### 1.4 Rules
- CH1 Kinds, in picker order with labels: voice "Voice", guitar "Guitar", bass "Bass", keys "Keys",
  drums "Drums", percussion "Percussion", horns "Horns", strings "Strings", playback "Playback",
  click "Click", other "Other" (default) (lib/channels.js:12-39). A row shows an unknown kind as
  "other" (channels-view.js:97) but edits keep the unknown kind when it matches what the table read
  has for that input; choosing a kind replaces it (channels.js:100-111, channels-view.js:151-154).
- CH2 A channel's label is its trimmed name, else "CH n" (channels.js:53-56).
- CH3 Rows = `status.channels.slice(0, status.sourceChannels)` (channels-view.js:37-39); a row is
  `{index, number, input, name, label, kind, kindLabel, hidden, stereo, group}` (:91-113); group
  found by `channel.groupId`.
- CH4 Pair units: an input with `link` and a next input whose hidden flag equals its own forms
  `[i, i+1]`, else `[i]`; hidden inputs make units too (channels.js:63-76). The link is kept on the
  first side only.
- CH5 Pairing availability and reasons (channels-view.js:48-84): linked → available, text "Linked
  with CH <partner>"; otherwise text "Mono" and unavailable with reason:
  "CH n is the last input, so there is no next input to pair it with." /
  "CH n+1 is already paired with CH n+2." /
  "CH h is hidden and CH s is shown: show or hide both before pairing them."
- CH6 Link sets `link` on the input when it is single; unlink from either side clears the unit's
  first side (channels-view.js:187-196).
- CH7 Hide/show applies to both sides of a pair (channels-view.js:175-181).
- CH8 Names are trimmed; max 32 characters counted as code points (channels.js:42,113).
- CH9 Every channel edit copies the whole table, extends it to `max(table, sourceChannels, index+1)`
  entries with defaults `{name:"", kind:"other", hidden:false, link:false}`, keeps entries past the
  device, and is validated before sending (channels-view.js:144-154). Validation errors (verbatim,
  channels.js:100-132): "channel details are supported for at most 32 channels, received N";
  "CH n has the instrument kind `k`, which is none of voice, guitar, bass, keys, drums, percussion,
  horns, strings, playback, click, other"; "the name of CH n is longer than 32 characters";
  "CH n is linked to the next channel, but it is the last one"; "CH n+1 cannot be in two stereo
  pairs (CH n-n+1 and CH n+1-n+2)"; "CH n and CH n+1 are one stereo pair: hide or show both".
  A thrown error becomes the failure "The channel details were not changed: <error>."
  (console-app.js:805-834).
- CH10 Channel and group edits run strictly one at a time, each read status after its answer
  (console-app.js:796-825). The screen re-renders immediately on its own edit events and at most once
  per animation frame on store changes (channels-screen.js:52,75-89).
- CH11 Optimistic toggles (channels-view.js:325-373): Shown boxes (key `shown:<index>`, both rows
  of a pair follow a box held on either) and group member boxes (key `member:<id>:<key>`) show the
  clicked value until that edit is answered; a later click on the same key wins (token compare).
  Name fields are protected only while focused; the kind select is not held (uncertain whether it
  visibly flips back if a status arrives before the answer; code does not prevent it).
- CH12 Groups: at most 16 (lib/mix.js:22). New group name "Group N" with N starting at
  `groups.length + 1` and skipping names already taken (case-insensitive, trimmed); error "there can
  be at most 16 groups" (channels-view.js:199-209). Rename trims, refuses empty ("a group needs a
  name"); unknown id "that group is no longer there" (:215-229). Membership by 1-based numbers, both
  sides of a pair together; joining one group removes the inputs from every other; channels sorted
  ascending (:235-245). Delete by id (:248-251). Failures read "The groups were not changed:
  <error>."; with no status and no kept answer: "The groups were not changed: no engine is running,
  so this desk does not know them yet; start the engine first." (console-app.js:836-848).
- CH13 Group tone = `["teal","blue","violet","orange"][index % 4]` by position in the status list;
  no group "none" (lib/values.js:28,156-158).
- CH14 Group cards (channels-view.js:270-306): members = device units with any side in the group
  (pair once, "(stereo)"), plus "CH n (not on this device)" for group channels past the device;
  choices = every device unit `{key: first side 1-based, numbers, label, checked, owner, hidden}`;
  label "CH n" or "CH a-b" plus the first side's trimmed name; owner = another group holding any
  side.
- CH15 `invalid` comes from the status group; passed to the row chip and the card chip.
- CH16 Processing word per row = `processingActive(state, stereo.first + 1)`: null (strips never
  read) gives "" and "not read yet" (lib/console-state.js:944-949).
- CH17 Pair/lock rules mirrored from the engine (used by the fake bridge and Mix; pinned by
  engineer tests): a pair keeps one level, mute and solo; a side that moved carries the other, both
  moved → first side wins (`keepPairsEqual`, channels.js:159-179); a phone cannot change level, pan,
  mute or solo of a locked channel (`keepLocked`, :188-196); a pair locks as one (`lockPairs`,
  :202-212).
- CH18 Stereo balance (Mix): pans `[max(0, 2b−100), min(100, 2b)]`, balance = mean of pans
  (channels.js:83-91). `fitChannels` keeps details per device input, defaults past, last input never
  linked (:139-148).
- CH19 Readback: `channel_table` returns the engine's table when running, else the session's
  (bridge.js:109-114).
- CH20 Delete confirm (group-cards.js:20,439-516; lib/network-view.js:44,250): arming starts a
  600 ms (`CONFIRM_DELAY_MS`) guard before "Delete" works, re-rendering at 600 ms; auto-disarms after
  10 000 ms; arming another card disarms the first; removing the card disarms.

###### 1.5 Empty, error, disabled states
- No status: note "The engine is not running, so this desk does not know its inputs and groups yet.
  Start it from the Mix screen." ("Mix" links to `#/mix`); body hidden (channels-screen.js:18-21,56-57).
- No groups: "No groups yet. A group gives every musician one fader for its inputs."; at 16: "The
  engine takes at most 16 groups: delete one to make another." (group-cards.js:214-215,243-244).
- Empty card: "No channels yet." (:282,377).
- Outcome: ok message in the status line, refusal in the alert line; previous one replaced
  (channels-screen.js:58-59).
- Shell without an engine bridge: `h1` "No engine to talk to" (shell.js:132-150).

###### 1.6 Accessibility
Row header cell `th scope=row` "CH n"; every control has the aria-labels above; state always in
words ("Linked with CH 2", "Hidden", "On"); unavailable stereo stays focusable with its reason as
description; the chip's invalid note is visually hidden text; focus ring 2px `--ml-focus`
offset 2px; `role=status`/`role=alert` regions stay in the DOM (empty) so new outcomes are
announced; Escape-to-revert in both name fields; Delete confirm moves focus to Keep.

###### 1.7 Tests
- `ui/tests/channels.test.js` (6): kind list/order; label falls back to "CH n"; balance↔pans;
  `checkChannels` refusals (with kept unknown kinds); `fitChannels`; pair keeps level/mute/solo.
- `ui/tests/channels-view.test.js` (14): kind options; rows (name, kind, group, hidden);
  `linkChoice` availability and the three reason strings; edits keep the whole table and are pure;
  hide/show pairs; link/unlink from either side and refusals; status groups → 1-based definitions;
  new group naming and the 16 cap; rename/members/delete by id (move between groups, pair joins as
  one); card members/count/choices/owner/hidden; `pairUnits`; `groupDefinitions` from status (0) or
  answer (1); pending toggles hold/release/token; unknown kind survives other edits.
- `ui/tests/channels-screen.test.js` (3): a Shown box stays as clicked until answered then shows
  the desk's value; a refused Shown edit reverts to the desk's value; a group box stays as clicked
  until answered.
- `ui/tests/engineer.test.js` (25, related ones): lines 112-188 and 212 pin `keepLocked`,
  `lockPairs`, `keepPairsEqual` for locks; a lock waiting while an input joins a pair is dropped.

###### 1.8 Custom drawing
- Kind icons (16px) and the stereo icon (two overlapping rings, 14px) are inline SVG paths from
  components/kind-icons.js:25-32; reproduce as vector icons.
- Horizontal peak meter (shared `ml-meter`): fraction = `(20·log10(level/100) + 60)/60` clamped
  (lib/values.js:130-145); zones safe/hot (≥ −6 dB)/clip (100) (:148-153).
- Group chip pill: 4 tones + none, dashed border when invalid (group-chip.js:11-74).
- Gallery: only `ml-group-chip` appears (ui/dev/gallery.html:242-247: Drums teal, Bass blue, Keys
  violet, Vocals orange, none → "No group", Horns teal invalid); `ui/dev/gallery.js` has no entries
  for these components.

---

##### 2. Processing (screens/processing-screen.js, route `#/processing` or `#/processing/N`)

###### 2.1 Layout (processing-screen.js:47-157, processing-screen.css)
1. Header `h1` "Processing", paragraph "Each input's processing is heard in every mix. Edit one
   channel, then copy it where it fits."
2. Note block (hidden unless blocked): text + "Read again" button (2.5).
3. Messages: outcome (`role=status`) then failure (`role=alert`).
4. Body (hidden while blocked):
   a. Wrapping action bar of four groups: [‹ prev, "Channel" select, › next, state chip];
      ["Bypass (A/B)", "Reset", "Copy to…" popover]; ["Preset" select, "Apply", "Delete"];
      form ["Save as" field, "Save"].
   b. Levels section (`aria-label "Levels"`): "Input" meter, "Processed" meter (4-column grid, 2
      columns under 700px wide).
   c. Bypassed banner (warn colours, hidden unless bypassed).
   d. Fieldset "Processing controls" (legend visually hidden), grid `auto-fit minmax(320px,1fr)`:
      Equaliser card spanning all columns (title `h2` "Equaliser", note "High-pass, then a low shelf,
      two peaks and a high shelf. Drag a handle; wheel for Q.", the EQ graph, then five band panels
      `auto-fit minmax(140px,1fr)`: `h3` "High-pass", "Low shelf", "Low peak", "High peak", "High
      shelf", each an On switch then its sliders); Gate card (`h2` "Gate", On switch right-aligned in
      the head, 5 sliders, "Reduction" meter); Compressor card (`h2` "Compressor", On switch, 6
      sliders, "Reduction" meter, transfer-curve figure).
   e. Effect sends card (outside the fieldset): `h2` "Effect sends", note "To the shared reverb and
      delay, after the strip and before every mix's fader. Each mix returns them on its master
      strip.", link "Effects…" (`#/effects`), sliders "Reverb" and "Delay".
5. Slider row = label (82px) | range | value (72px); inside band panels label and value share a
   line with the slider below (css:318-353).

###### 2.2 Controls

| Control | Verbatim label / aria-label | Range / step / units / keys | States | Command, when | Source |
| --- | --- | --- | --- | --- | --- |
| Previous / Next | "‹" / "›", aria-label and title "Previous channel" / "Next channel" | click; steps choices, held at the ends | disabled at first / last choice | opens strip, asks curve; hash replaced | screen:63,66,356-357,507-514 |
| Channel select | label "Channel" | options = strip choices (P3) | | opens strip on change | :64-65,350-354,512 |
| Bypass (A/B) | "Bypass (A/B)", `aria-pressed`, title "Hear the channel with no processing; press again to put it back" | toggle | disabled when not bypassed and the strip is neutral (`canBypass`) | `set_channel_processing` with a neutral strip / with the kept strip (P10) | :70-71,365-366,518 |
| Reset | "Reset", title "Take every part of this strip off" | | disabled while bypassed | `reset_channel_processing {channel}`; asks curve | :72,367-369,519-522 |
| Copy to… | summary "Copy to…" opens a popover (`details`); fieldset legend "Copy <choice label> to"; one checkbox per other strip (text = its choice label, value = first side); button "Copy" (primary) | multi-select | "Copy" disabled while bypassed; on success popover closes and boxes clear | `copy_channel_processing {from, to}` on Copy; none picked → P13 message | :73-82,304-325,523-533 |
| Preset select | label "Preset"; options `name` or `name (built-in)` | | selection not forced by re-render | re-render only | :85-86,377-379,534 |
| Apply | "Apply" | | disabled while bypassed or nothing chosen | `apply_strip_preset {name, channel}`; asks curve | :87,380,535-538 |
| Delete (preset) | "Delete"; title "Built-in presets cannot be deleted" / "Delete this preset of yours" | no confirm | disabled when nothing chosen or built-in | `delete_strip_preset {name}` | :88,381-384,539 |
| Save as | label "Save as", placeholder "Preset name", maxlength 40; submit "Save" (Enter submits) | | | `save_strip_preset {name: trimmed, settings: shown strip}`; on success select shows the new name, field cleared | :90-94,540-550 |
| Part On switches (7) | visible "On"; aria-label "High-pass on", "Low shelf on", "Low peak on", "High peak on", "High shelf on", "Gate on", "Compressor on" | checkbox | disabled with the fieldset while bypassed | `set_channel_processing` whole strip, on change; EQ parts also ask curve | :177-185,250-282,472-477 |
| Strip sliders (25) | visible label (table P5); aria-label "<Part> <label lowercased>", e.g. "High-pass frequency", "Low peak gain", "High shelf q", "Gate threshold", "Compressor makeup"; output and `aria-valuetext` = formatted value | native range 0-1000 step 1 mapped linear/log (P6); native slider keys (browser default; exact PageUp/Home behaviour not set in code) | disabled while bypassed | `set_channel_processing`, live on every `input` event (coalesced per channel); EQ paths also ask curve (debounced) | :163-174,386-397,451-471 |
| EQ graph handles (5) | named per handle (2.6) | drag, wheel, keys (2.8) | `aria-disabled` and inert while bypassed | `set_channel_processing` live per gesture + curve request | :497-505 |
| Reverb / Delay send | labels "Reverb", "Delay"; aria-label "<choice label> reverb send" / "… delay send"; output and valuetext "Off" or "N%" | range 0-100 step 1 % | stay enabled while bypassed (outside the fieldset); the slider under the pointer is not redrawn | `set_channel_send {channel, effect, level}`, live on `input` (coalesced per channel+effect) | :145-154,419-430,479-496 |
| Effects… link | "Effects…" | | | navigates `#/effects` | :143 |
| Read again | "Read again" | | only when inputs known but strips unread | `processing` | :54,339,515 |

###### 2.3 Displayed values and formats (lib/processing-view.js)
- `formatHz` (:75-81): rounded < 1000 → "80 Hz"; else kHz with ≤1 decimal, zeros dropped: "2.5 kHz",
  "1 kHz", "12.5 kHz" (99.6 → "100 Hz").
- `formatGainDb` (:84-90): one decimal, signed: "+3.0 dB", "−2.0 dB", "0.0 dB" (never "−0.0").
- `formatDb` (:93-96): ≤1 decimal, zeros dropped: "−18 dB", "−2.5 dB", "6 dB", "0 dB" (gate and
  compressor threshold, range, knee, makeup).
- `formatQ` (:99-102): < 10 → 2 decimals "0.71"; ≥ 10 → 1 decimal "10.0".
- `formatMs` (:105-111): ≥ 1000 → seconds ≤2 decimals "1.5 s"; < 10 → ≤2 decimals "0.5 ms",
  "0.05 ms"; else whole "150 ms".
- `formatRatio` (:114-116): "3:1", "2.5:1".
- Reduction meters (:360-366): value < 0.05 dB or non-finite → "0.0 dB"; else "−X.X dB".
- Sends: `formatLevel` "Off" at 0, else "N%" (lib/effects.js:131-134).
- State chip (screen:358-363): "Bypassed" (warn) / "Processing on" (ok) / "No processing" (muted).
- Bypassed banner (screen:371-373): "You hear <choice label> with no processing. Its settings are
  kept below; press Bypass (A/B) again to put them back. Leaving this screen puts them back too."
- Strip choice labels (P3): "CH 1 · Kick", "CH 2", "CH 3–4 · Keys".
- Meters (screen:436-448): "Input" = max over the strip's sides of `meters.channels`; "Processed"
  = max of `meters.processed` (falls back to `channels`); accessible names "Input level",
  "Processed level"; gate reduction max of `gateReductionDb` on a 40 dB scale, compressor max of
  `compressorReductionDb` on 20 dB (processing-view.js:523-524), names "Gate gain reduction",
  "Compressor gain reduction", visible label "Reduction".
- Transfer figure caption "Output against input, −60 to 0 dB" (screen:135).

###### 2.4 Rules
- P1 Strip shape and defaults (lib/processing.js:52-78): high-pass off 80 Hz Q 0.7071; bands on, 0 dB:
  low shelf 100 Hz Q 0.7071, peak 400 Hz Q 1, peak 2500 Hz Q 1, high shelf 8000 Hz Q 0.7071; gate
  off −50 dB threshold, 40 dB range, 0.5 ms attack, 50 ms hold, 150 ms release; compressor off
  −18 dB, 3:1, knee 6 dB, attack 10 ms, release 150 ms, makeup 0; sends reverb 0, delay 0.
- P2 Ranges (processing.js:27-44): HPF 20-2000 Hz, Q 0.5-2; band 20-20000 Hz, ±18 dB, Q 0.3-10; gate
  threshold −80…0, range 0-80, attack 0.05-50, hold 0-2000, release 5-4000 ms; compressor threshold
  −60…0, ratio 1-20, knee 0-24, attack 0.1-200, release 10-2000 ms, makeup 0-24 dB. `clampStrip`
  fills missing fields with defaults and clamps; non-numbers take the fallback (:93-138). Sends are
  whole 0-100. Every edit is sent clamped (processing-editor.js:104).
- P3 Strip choices (processing-view.js:372-389): the first `min(sourceChannels, 32)` inputs, a pair
  once on its first side; label "CH n" or "CH a–b", plus " · <name>" when the first side has a name.
  Opening a number opens the choice holding it; a number not on the device keeps the current one
  (processing-editor.js:140-147). Prev/next are clamped (processing-view.js:392-398).
- P4 Initial channel: hash `#/processing/N` (N 1-32, routeChannel lib/router.js:53-60), else the
  channel this app's editor had open last (per app instance, processing-editor.js:32,46), else
  CH 1, normalised to an existing choice. The hash is kept on the open channel with
  `history.replaceState` (screen:346-348). Opening clears the outcome.
- P5 Sliders per part (processing-view.js:493-520): High-pass: Frequency (log), Q (log). Each band:
  Frequency (log), Gain (linear), Q (log). Gate: Threshold, Range (linear), Attack, Hold, Release
  (log). Compressor: Threshold (linear), Ratio (log), Knee (linear), Attack, Release (log), Makeup
  (linear).
- P6 Slider mapping (processing-view.js:283-314): 1000 steps. Linear: `pos = round((v−lo)/(hi−lo)·1000)`.
  Log: offset `o = lo ≤ 0 ? 1 − lo : 0`, `pos = round(ln((v+o)/(lo+o)) / ln((hi+o)/(lo+o)) · 1000)`,
  `v = (lo+o)·((hi+o)/(lo+o))^(pos/1000) − o`; values clamped; ends exact.
- P7 Neutral (processing.js:141-160): EQ neutral = HPF off and every band off or 0 dB; strip neutral
  also needs gate off or range 0, compressor off or (ratio 1 and makeup 0). "Active" = not neutral
  (sends ignored). A strip that only sends is still kept by the desk (`isUntouchedStrip`).
- P8 Shown strip = the pending edit, else the desk's, else a neutral default while unread
  (console-state.js:928-937). Strip edits announce nothing on success; only refusals become the
  outcome (processing-editor.js:99-106).
- P9 Linked pair = one strip: opened, edited, copied and metered on both sides; the desk's answer
  names both channels (processing.js:276-290).
- P10 Bypass A/B (processing-editor.js:161-192; processing-view.js:433-455): only when active; keeps
  a copy of the strip in the editor and sends the default strip; while bypassed the kept strip is
  shown, every editor change is refused ("CH n is bypassed: turn the bypass off to change its
  processing."), and the sections, graph, Reset, Copy and Apply are disabled. Bypass again sends the
  kept strip; if refused it stays bypassed and kept. If the desk shows the strip active with no edit
  of the editor's pending, the bypass ends silently (changed elsewhere). Leaving the screen sends
  back every kept strip (`dispose`, :244-253). Messages: "CH n is bypassed: you hear it with no
  processing.", "CH n's processing is back on.", "CH n has no processing to bypass."
- P11 Reset = default strip, keeps sends (README.md:211).
- P12 Copy to… copies the open strip onto the checked strips (first sides); desk dedups and drops
  `from` (src-tauri/src/commands/processing.rs:763-777); sends are kept per channel.
- P13 Messages from the editor: "Pick the channels to copy CH n's processing to." (no target),
  "Name the preset to save it." (blank name) (processing-editor.js:198-217).
- P14 Presets (bridge.js:371-392; commands/processing.rs:20-25,845-958): built-in first: "Vocal",
  "Kick", "Bass DI", "Acoustic guitar", "Gentle compression", listed as "Vocal (built-in)", never
  deleted or replaced; own presets in save order; names 1-40 characters after trim, matched in any
  case; saving an own name replaces it in place; a preset keeps no sends. Desk messages (shown
  verbatim): "Saved the strip preset \"X\".", "The strip preset \"X\" was replaced.", "Deleted the
  strip preset \"X\".", "\"X\" is a built-in preset and cannot be deleted.", "There is no strip
  preset called \"X\"; nothing was deleted.", "\"X\" is on CH 2 now.", "There is no strip preset
  called \"X\"; nothing changed.", "A strip preset needs a name; nothing was saved.", "A strip
  preset's name is at most 40 characters; nothing was saved.", "\"X\" is a built-in preset; save under
  another name. Nothing was saved."
- P15 Other desk messages: "CH 1 is set to the new processing." / "CH 3 and CH 4 are set to the new
  processing.", "CH 3 is back to no processing.", "CH 1's processing is on CH 3, CH 4 and CH 8 now.",
  "There is no channel to copy CH n's processing to; nothing changed.", "CH 5 is sending 20% to the
  reverb." (commands/processing.rs:648,718,742,776,796); lists via `channelList` "CH 1, CH 2 and
  CH 5" (processing.js:246-252). Outcome mapping: `set` ok; `invalid`/`notFound` not
  (processing.js:259-263); presets `saved|replaced|deleted` ok, `invalid|factory|notFound` not
  (:270-274); fallbacks "The processing was changed." / "The processing was not changed." / "The strip
  presets were changed." / "The strip presets were not changed."
- P16 Client refusal wording (console-app.js:262-273,1296-1303,1770-1806,1077,1094): "CH n's
  processing was not changed: <reason>.", "CH n's processing was not changed: it was not sent.",
  "CH n's processing was not reset: …", "CH n's processing was not copied: …", "\"name\" was not
  applied to CH n: …", "The strip preset was not saved: …", "The strip preset was not deleted: …",
  "CH n's reverb send was not changed: …", fallback "The send was not changed."
- P17 Failure line (processing-view.js:464-485): shows the latest failure among (a) a refused send
  of the channel shown, (b) the editor's last refused outcome or the store's processing error; a
  refused send is forgotten when another channel is shown. The store's error shows when its channel
  is null or a side of the open strip (processing-editor.js:133-136).
- P18 EQ curve (processing-editor.js:27,227-242; processing-view.js:275-280): drawn from a local
  computation (128 points) at once; 120 ms after the last EQ change (and on mount, open, reset,
  apply) the desk is asked (`eq_response`); its points replace the local curve only while the
  equaliser shown equals the one asked (JSON equality); stale answers are dropped by a counter; a
  failed ask is silent. Curves are stored per channel (`eqCurves`).
- P19 EQ math, the same as the desk (processing.js:164-243): RBJ cookbook biquads at 48 kHz,
  `w0 = 2π f/48000`, `α = sin w0 /(2Q)`, `A = 10^(G/40)`; high-pass (G=0), peak, low shelf, high
  shelf coefficient sets at :172-196, normalised by a0. Magnitude per biquad
  `10·log10(|N(e^jw)|²/|D(e^jw)|²)` (power floor 1e-30); total = HPF (if on) + every band that is on
  with gain ≠ 0. Response: `count = clamp(round(points), 2, 1024)` (default 128), frequencies
  `20 · 1000^(i/(count−1))`, both ends included. Signal order in the engine: high-pass, gate,
  equaliser, compressor (README.md:205).
- P20 Strip presets are reloaded on mount when not yet read (screen:557-560).
- P21 Processing edits made while a session loads are held, then dropped if the load lands (and
  said so) or sent if it was refused (console-app.js:1285-1303).

###### 2.5 Empty, error, disabled states
- No status / no inputs: note "The engine is not running, so this desk does not know its inputs yet.
  Start it from the Mix screen." (plain text, no link), no Read again (screen:336-342).
- Strips unread (`processing` null): note "The channel processing could not be read." + "Read
  again". After a loaded session whose strips failed: store error "The loaded session's channel
  processing could not be read: <reason>." (console-app.js:1222-1231).
- Bypassed: banner, state chip "Bypassed", sections fieldset disabled (dimmed 0.7), graph inert;
  Delete preset, Save and sends stay enabled.
- Prev/next disabled at the ends; Bypass disabled on a neutral strip; Apply/Delete per 2.2.

###### 2.6 Accessibility
- Every slider: `aria-label` "<Part> <field>", `aria-valuetext` = the formatted value, visible
  `<output>`. On switches named "<Part> on".
- EQ graph: `svg role=img aria-label "Equaliser response curve"` described by a visually hidden
  paragraph = `curveDescription` (processing-view.js:252-264), e.g. "Equaliser response. High-pass
  on at 80 Hz. Low shelf on: 100 Hz, 0.0 dB, Q 0.71. Low peak on: 400 Hz, +3.0 dB, Q 1.00. High peak
  on: 2.5 kHz, 0.0 dB, Q 1.00. High shelf off."; handles in a `role=group` "Equaliser handles"; each
  handle a focusable button named "High-pass, on|off, 80 Hz" or "Low peak, on, 400 Hz, +6.0 dB, Q
  1.00" (:119-150), title "Drag to move; wheel or Page Up and Page Down for Q; arrows move it (Shift
  for bigger steps), Home sets 0 dB, Enter turns it on or off" (eq-graph.js:30-32); grid labels
  `aria-hidden`.
- Transfer curve `svg role=img`, aria-label "Compressor curve: threshold −18 dB, ratio 3:1, knee 6
  dB, makeup 0 dB" (screen:412-417).
- Reduction meters: `role=meter`, `aria-valuemin 0`, `aria-valuemax` = scale, `aria-valuenow` =
  clamped dB with 1 decimal, `aria-valuetext` "−4.0 dB", updated at most every 500 ms plus a trailing
  update (components/reduction-meter.js:15,65-109). Input/processed meters as `ml-meter`.
- Live regions: outcome `role=status`, failure `role=alert`.
- Keyboard: full EQ map in 2.8; Enter submits Save as.

###### 2.7 Tests
- `ui/tests/processing.test.js` (16): defaults and clamping; EQ response equals the desk's
  (flat, +6 dB peak, −3 dB HPF corner, shelf, point count clamps 2/1024); channel lists and outcome
  mapping; bridge command names and argument names; fake bridge: strip edits in range and pairs
  whole; strip presets (built-ins untouched, case-insensitive replace/apply/delete); EQ response and
  processed meters; sessions carry strips; console reads strips and presets on connect; dragged edit
  shows at once and sends only the latest after the in-flight one; a loaded session replaces strips
  and drops waiting edits; edits during a load dropped or sent; an early answer does not invent other
  strips; an edit before the first read reads all; unreadable strips after a load are said; reset,
  copy and preset wait for a linked partner's pending edit.
- `ui/tests/processing-editor.test.js` (11): opens the asked channel (pair on its first side) and
  steps; slider change sent whole and clamped, shown at once, not announced; A/B bypass/restore;
  refused restore keeps bypass; per-app last channel; strip changed elsewhere ends bypass; curve
  local at once and asked once after the pause (120 ms); reset/copy/preset outcomes, built-in not
  deletable; table and Mix links open a pair on its first side and say On only when active; unread
  links say neither On nor Off ("Processing of Kick, not read yet"); unread strips and Read again.
- `ui/tests/processing-view.test.js` (19): log frequency scale and inverse; ±18 dB scale; every
  format (see 2.3 examples); handle positions and labels; drag (clamp, enable, HPF frequency only);
  wheel/keys; curve path string ("M0.0 100.0L600.0 0.0"); grid labels; curve description; desk vs
  local curve; slider linear/log mapping; `withSetting`; compressor static curve and transfer path
  ends ("M0.0 60.0" … "L60.0 15.0" for −20 dB 4:1); reduction meter; strip choices and stepping;
  preset list labels; A/B helpers; every slider covers its range; failure-line ordering.

###### 2.8 Custom drawing and gestures
- EQ graph (components/eq-graph.js): logical box 600×200 (screen:44); rendered `width: 100%`,
  height 200px, `preserveAspectRatio="none"` so x stretches; strokes non-scaling.
  - Scales (processing-view.js:37-62): `x = ln(f/20)/ln(1000) · W` (f clamped 20-20000);
    `f = 20·e^(x/W · ln 1000)`; `y = (18 − clamp(dB, ±18))/36 · H`; `dB = clamp(18 − y/H·36, ±18)`.
  - Grid (:237-249): vertical lines at 50, 100, 200, 500, 1k, 2k, 5k, 10k Hz labelled "50 Hz" …
    "10 kHz" (bottom, centred, 2xs mono); horizontal at +12, +6, 0, −6, −12 labelled "+12", "+6",
    "−6", "−12" (the "0" label omitted) at the left; separate stronger 0 dB line
    `M0 H/2 HW` (eq-graph.js:186-213).
  - Curve: polyline `M/L x y` with 1 decimal through the points (processing-view.js:225-235),
    accent stroke 2px.
  - Handles (eq-graph.js:92-129,223-237): 28×28px hit target centred on (x/W %, y/H %), visible
    18px circle (inset 5px) with 2px border and a 9px bold mono mark "H" or "1"-"4"; off → dashed
    muted border. HPF handle sits on the 0 dB line at its frequency; bands at frequency and gain.
  - Drag (eq-graph.js:244-274; processing-view.js:171-178): primary button only; focuses and
    captures the pointer; each move emits box-unit x,y from the SVG's client rect; band takes
    frequency (clamped to its range) and gain (±18), HPF frequency only (20-2000); dragging turns the
    part on; ends on pointerup, pointercancel or lost capture. No rounding of dragged values.
  - Wheel over a handle (eq-graph.js:275-285): deltaY < 0 → Q × 1.12, > 0 → Q ÷ 1.12, clamped
    (band 0.3-10, HPF 0.5-2); the page does not scroll.
  - Keyboard on a focused handle (processing-view.js:191-222), default prevented for these keys:

    | Key | Band | High-pass |
    | --- | --- | --- |
    | ArrowRight / ArrowLeft | frequency × / ÷ 2^(1/12) (semitone); Shift: × / ÷ 2 (octave); clamped 20-20000 | same, clamped 20-2000 |
    | ArrowUp / ArrowDown | gain ± 0.5 dB; Shift ± 3 dB; clamped ±18 | no effect |
    | PageUp / PageDown | Q × / ÷ 1.12; Shift × / ÷ 1.12^4; clamped 0.3-10 | same, 0.5-2 |
    | Home | gain to 0 dB | no effect |
    | Enter, Space | toggle on/off | toggle on/off |
    | Tab | next handle (native order H, 1, 2, 3, 4) | |

    Keys do not turn a part on (only drag does). Disabled graph: no drag, wheel or keys.
  - Each gesture computes the next EQ from the current one (including a gesture not yet drawn) and
    sends the whole strip, then requests the curve (screen:457-464).
- Compressor transfer curve (processing-view.js:333-357; screen:130-135): `over = in − T`; if
  `2·over < −K` → `out = in`; else if `K > 0` and `2|over| ≤ K` → `out = in + (1/R − 1)(over + K/2)²
  /(2K)`; else `out = T + over/R`; plus makeup. Path with 61 points, `x = i/60 · 60`,
  `in = −60·(1 − i/60)`, `y = clamp(out, −60, 0)/−60 · 60` in a 60×60 viewBox drawn at 96×96px, over
  a dashed unity diagonal `M0 60L60 0`.
- Reduction meters (components/reduction-meter.js): 8px track, bar fills from the right
  (`scaleX(fraction)`, origin right), `fraction = clamp(dB/scale, 0, 1)`; value text 64px column,
  right-aligned mono; drawn once per animation frame.

---

##### 3. Effects (screens/effects-screen.js, route `#/effects`)

###### 3.1 Layout (effects-screen.js:38-68, effects-screen.css)
1. No `h1` in this screen (unlike Channels and Processing; navigation focus falls back to `main`,
   shell.js:107-111); window title "Effects · BandEar" (router.js:75-77). Intro paragraph: "One
   reverb and one delay, shared by every mix. Send inputs to them from the Processing editor; each
   mix returns them on its master strip. They add no latency to the dry mix." ("Processing" links to
   `#/processing`).
2. Note (hidden while running): see 3.5.
3. Outcome line (`role=status aria-live=polite`), danger colour when the last outcome failed.
4. Fieldset "Effects" (legend visually hidden), grid `auto-fit minmax(320px, 1fr)`: Reverb card
   (`h2` "Reverb"; sliders Size, Damping, Pre-delay, Width) and Delay card (`h2` "Delay" with
   "Ping-pong" switch in the head; tempo form ["Tempo" text field, "Note" select, "Set tempo"]; sliders
   Time, Feedback, High cut; the "Repeats every …" line).

###### 3.2 Controls

| Control | Verbatim label / aria-label | Range / step / units | States | Command, when | Source |
| --- | --- | --- | --- | --- | --- |
| Reverb sliders | "Size", "Damping", "Pre-delay", "Width"; aria-label "Reverb size", "Reverb damping", "Reverb pre-delay", "Reverb width" | 0-100 %, 0-100 %, 0-200 ms, 0-100 %; step 1 (raw values) | dragged slider not redrawn under the pointer | `set_effect {effect:"reverb", settings: whole reverb}`, live on `input` (coalesced) | :14-19,28-36,104-119,144-155 |
| Delay sliders | "Time", "Feedback", "High cut"; aria-label "Delay time", "Delay feedback", "Delay high cut" | 1-2000 ms step 1; 0-90 % step 1; 1000-20000 Hz step 100 | Time disabled while a tempo is set, title "The tempo sets the time; clear the tempo to set it here" | `set_effect {effect:"delay", …}` live | :20-26,120-122 |
| Ping-pong | visible "Ping-pong"; aria-label "Delay ping-pong" | checkbox | | `set_effect` delay on change | :53,123,156 |
| Tempo field | label "Tempo"; placeholder "bpm"; `inputmode=decimal` | text; 30-300 bpm or empty | shows the tempo only when synced; not overwritten after typing until submitted, nor while focused | none until submit | :56-57,124-126,157-159 |
| Note select | label "Note"; options "1/2", "1/4", "Dotted 1/8", "1/4 triplet", "1/8", "1/8 triplet", "1/16" | | not overwritten while focused | `set_effect` delay with `note`, on change (also when no tempo) | :58-61,127-129,160 |
| Set tempo | "Set tempo" (submit; Enter in the field) | | invalid → outcome message, nothing sent | `set_effect` delay with `{tempoBpm, note}` | :62,161-171 |

###### 3.3 Displayed values and formats (lib/effects.js)
- Reverb outputs: "50%" (size), "40%" (damping), pre-delay `formatTime` "10 ms", "100%" (width);
  percentages rounded (effects.js:180-186).
- Delay outputs: Time `formatTime(timeMs)` "375 ms" / "1.25 s" (≥1000 ms: `round(ms/10)/100` s)
  (:137-140); Feedback "35%"; High cut `formatKhz` "6 kHz", "12.5 kHz" (`round(hz/100)/10`) (:143-145).
- Repeats line: "Repeats every <timeInForce>." where timeInForce is `formatTime(delayTimeMs)`, or
  when synced "<time> (<note label> at <bpm> bpm)", e.g. "Repeats every 250 ms (1/8 at 120 bpm)."
  (:188-195; screen:130). The Time slider's own output shows `timeMs` even while synced.
- Tempo field text: `String(tempoBpm)` (e.g. "128.5") when synced, else "".
- Outcomes (effects.js:234-243): desk message, e.g. "The reverb is set: size 50%, damping 40%,
  pre-delay 10 ms, width 100%." / "The delay is set: 375 ms, feedback 35%, ping-pong."
  (src-tauri/src/commands/effects.rs:208-224), plus " It was not saved for the next start: <err>."
  when the save failed; refused: desk message, else "The effect was not set."; transport failure
  "The reverb was not set: <reason>." (console-app.js:1101-1106). Every settled change is shown,
  successes included (unlike Processing).

###### 3.4 Rules
- E1 Two effects, desk order reverb, delay (effects.js:20); one shared instance each.
- E2 Defaults (:51-63): reverb size 50, damping 40, pre-delay 10 ms, width 100; delay 375 ms, tempo
  0 (none), note quarter, feedback 35, high cut 6000 Hz, ping-pong off.
- E3 Ranges (:28-37): size/damping/width 0-100 %, pre-delay 0-200 ms, time 1-2000 ms, tempo 30-300
  bpm, feedback 0-90 %, high cut 1000-20000 Hz. Unreadable → default; tempo under 30 → 0, above
  300 → 300; unknown note → quarter (:75-103).
- E4 Note values and beats (:40-48): half 2, quarter 1, dottedEighth 0.75, quarterTriplet 2/3, eighth
  0.5, eighthTriplet 1/3, sixteenth 0.25.
- E5 Delay time in force (:111-118): tempo > 0 → `60000/bpm · beats`, else `timeMs`; clamped 1-2000
  ms (e.g. 120 bpm 1/4 = 500 ms, dotted 1/8 = 375 ms, 30 bpm 1/2 = 2000 ms).
- E6 Tempo entry (:151-162): trimmed; empty → 0 (clears sync); comma accepted as decimal point;
  rounded to 0.1 bpm; outside 30-300 or not a number → "A tempo is 30 to 300 bpm, or empty for none."
- E7 Every change sends the whole effect: the current view's values with the change merged
  (screen:134-142); the console shows it at once (`effectsPending`) and coalesces per effect, latest
  wins; an older answer for one effect never undoes the other's newer one (effects tests).
- E8 Shown values = the pending change over the status's `effects` (:169-197).
- E9 Sends live on each strip (`settings.sends`, whole 0-100), shared by a pair, untouched by
  presets, Reset and Copy to… (README.md:258); `channelSends` = pending over the strip's
  (effects.js:200-205). Returns are per mix on the Mix master strip (`returnsView`, labels "Reverb
  return"/"Delay return", :211-231; area of the Mix inventory).
- E10 `running` = engine phase "running" and a status (:179); otherwise every control is disabled.

###### 3.5 Empty, error, disabled states
- Not running: note "The engine is not running. Start it from the Mix screen to change the effects."
  (no link); fieldset disabled, cards at 0.7 opacity (screen:42,100-101; css:36-38).
- Time slider disabled while synced (3.2). Invalid tempo → message in the outcome line.

###### 3.6 Accessibility
Sliders have `aria-label` "<Effect> <label lowercased>" and `aria-valuetext` = the shown text
(screen:33,118); one polite status region for all outcomes, its `data-ok` drives colour only; labels
`for` the tempo field and note select; the time slider's title explains why it is disabled.

###### 3.7 Tests
- `ui/tests/effects.test.js` (10): clamping with defaults (reverb, delay, tempo cap 300); delay time
  from note and tempo held to 2 s (222.22 ms for 90 bpm 1/8 triplet); tempo parsing incl. "128,5" and
  the refusal text; strips carry sends ("Off", "35%"); panel and master strip show pending over kept
  ("250 ms (1/8 at 120 bpm)"); `formatEffectSet`/`formatReturnSet` wording; fake keeps sends with
  strips and shares them in a pair; fake sets effects and returns and sessions carry them; console
  shows a send, an effect and a return at once and settles on the answer; older answers never undo
  newer ones across effects/returns.

###### 3.8 Custom drawing
None beyond standard sliders, a checkbox, a text field and a select.

---

##### 4. Uncertainties
- Native `<input type=range>` keyboard behaviour (arrow step = 1 of 1000 positions, PageUp/Down,
  Home/End) is browser-defined; the code adds no handlers.
- After deleting the selected preset, which option the Preset select shows is left to the browser
  (options are rebuilt, value not forced; screen:293-302,378).
- Whether the instrument select visibly reverts if a status arrives before `set_channels` answers
  (CH11) is not tested.

#### E — Network and system screen: parity inventory

Scope: `apps/desktop/ui/app/screens/network-screen.{js,css}` and the components and libs it uses.
Paths below are relative to `apps/desktop/` unless they start with `apps/server/`. Strings in
backticks or quotes are verbatim. "—" is U+2014, "−" is U+2212, "…" is U+2026, "×" is U+00D7.

##### 0. Screen overview

###### 0.1 Layout (network-screen.js:34-55, network-screen.css:7-43)
1. Header: `h1` "Network and system" (`tabindex="-1"`), then the paragraph "Each phone's link and name, the audio input the mixes come from, and the saved sessions."
2. No-engine note (shown only while `state.engine.status === null`, network-screen.js:97): "The engine is not running, so no phone is listed. Start it from the [Mix](#/mix) screen, or choose a driver below."
3. Command error line, `role="alert"` (error style): `mixMessages(state).command` (network-screen.js:98-100, mix-view.js:281-288).
4. Audio notice line, `role="status"`: `state.audioNotice?.message` (fallback or "not saved" notice).
5. Body (`.network-body`, flex-wrap, gap `--ml-space-4`):
   - Left `section aria-label="Musicians"` (flex `999 1 560px`): musicians table, then the link-health panel (hidden until a musician is picked).
   - Right `aside aria-label="Audio input and session"` (flex `1 1 300px`): Audio input card, then Session card. In a narrow window the aside drops below (css:1-5).

There is no engine Start/Stop button on this screen: Start/Stop and the status pill (`ml-status-pill`) live in the top bar of the Mix screen only (mix-screen.js:54,281-282; top-bar.js:101). `status-pill.js` is not used here. The only engine (re)starts this screen triggers are a driver change, a buffer change, a non-WASAPI device switch and "Start on this driver" (section E).

###### 0.2 Screen-level messages
- Command failure text: `${COMMAND_FAILURES[action]}: ${message}.` with `stop` → "The engine could not be stopped", `start` → "The engine could not be started", `switch` → "The input device could not be switched", otherwise `The engine refused to ${action}` (mix-view.js:267-286).
- The engine's own `engine.error` (phase "error", e.g. a failed start with no engine running) is not rendered on this screen (only `command` and `audio` are, network-screen.js:98-102).
- Audio notices (console-app.js:374-395, 460-462, engine-config.js:117-130):
  - fallback: "The saved input ${device} was not found, so the engine started on the system's default input. The saved input is kept and tried again at the next start."
  - fallback from ASIO: "The saved ASIO driver ${device} was not found, so the engine started on the system's own driver and its default input, never on another ASIO driver. The saved driver is kept and tried again at the next start."
  - unsaved: `sentenceWithReason("The audio input runs but was not saved for the next start", error)`.
- `sentenceWithReason(lead, reason)` = `${lead}: ${reason without trailing dots}.`, or `${lead}.` when the reason is blank (format.js:12-17). Used everywhere below.
- Window title (not on the screen, but follows its device): "BandEar Server — Demo source" while the demo plays, "BandEar Server — ${deviceName}" while an engine runs, else "BandEar Server" (network-view.js:255-261).

###### 0.3 Data flow and timing
- The screen renders from the store on every change, batched to one `requestAnimationFrame` (network-screen.js:128-136); after its own awaited commands it renders at once (`update`, :140-143).
- Screen-local state (:74-88): `draftHost`, `draftDevice`, `pendingSwitch`, `addError`, `tableError`, `outputSaid`, `bufferSaid {address, ok, message}`, `redundancySaid {address, ok, message}`. Each "said" stays until the next change of its kind.
- On mount: `loadDevices(audio.host)` (`list_input_devices`), `loadMixBackups()` (`list_mix_backups`), and the outputs listing once per engine run (`followOutputs`, local-mix.js:195-209 → `output_devices`) (network-screen.js:315-320). On every store change `followRun(state)` lists outputs again when a new run appears (:311-314).
- Unmount: unsubscribe, stop the link-history follower, cancel the frame (:322-327).

###### 0.4 Tauri commands used by this screen
Registered in `src-tauri/src/lib.rs` (`generate_handler!` at :737); JS wrappers in `ui/app/lib/bridge.js`.

| Command (lib.rs line) | JS args (bridge.js line) | Triggered by |
|---|---|---|
| `list_input_devices` (:60) | `{ host }` (:65) | mount, driver change, after ASIO start |
| `start_engine` (:40) | `{ config: { deviceFilter, targets: [], controlPort: 50001, groups: [], host, bufferFrames } }` (:62, engine-config.js:138-152) | driver change, buffer change, non-WASAPI device switch, Start on this driver |
| `switch_device` (:65) | `{ device }` (:66) | device switch under WASAPI only |
| `set_audio_settings` (:185) | `{ settings: audio }` (:138) | after any start/switch that ran with new choices (console-app.js:374-395) |
| `engine_status` (:50) | none (:57) | after nearly every command (`readStatus`) |
| `add_target` (:73) | `{ address }` (:67) | Add phone |
| `remove_target` (:78) | `{ address }` (:68) | confirmed Remove |
| `set_musician_name` (:121) | `{ address, name }` (:69) | name field commit |
| `link_history` (:461) | `{ musicianKey, sinceSeq }` (:231-232) | open panel, then every 1000 ms |
| `set_musician_buffer` (:475) | `{ musicianKey, mode, ms }` (:242-243) | Buffer mode / ms commit |
| `set_musician_redundancy` (:491) | `{ musicianKey, on, delayBlocks }` (:254-255) | Redundancy switch / Copy select |
| `output_devices` (:105) | none (:82) | once per engine run |
| `set_local_output` (:113) | `{ output: {device, firstChannel} \| null }` (:89) | Local mix out select |
| `sessions` (:271) | none (:187) | List them again, after every session command |
| `session_notice` (:264) | none (:182) | on connect (console-app.js:898-904) |
| `save_session` (:278) | `{ name }` (:192) | Save |
| `rename_session` (:287) | `{ id, name }` (:194) | Rename |
| `delete_session` (:297) | `{ id }` (:196) | confirmed Delete |
| `load_session` (:305) | `{ id }` (:206) | Load / Load anyway |
| `list_mix_backups` (:441) | none (:211) | mount, Refresh, after a restore |
| `restore_mix_backup` (:450) | `{ stamp }` (:218) | confirmed Restore |

Musician key format: `client:${clientId}` or `address:${address}` (link-health-view.js:50-52).

---

##### A. Musicians table (`<ml-musicians-table>`, components/musicians-table.js)

###### A1. Layout
Order inside the element (musicians-table.js:326-359): table error line (`role="alert"`, above the table) → bordered table (horizontal scroll when narrow) → empty line → add form → hint paragraph.

Columns (th, :331-340), uppercase small headers:

| # | Header | Header tooltip (title) | Cell content |
|---|---|---|---|
| 1 | Musician | — | name input (14em, bold, placeholder "unnamed") + badge "local label" (dashed, only when `nameIsLocal`); address below (mono, xs) |
| 2 | Control | — | dot (hollow muted; filled green when connected, `aria-hidden`) + "connected" / "offline" |
| 3 | Link | "What the musician's phone reports about the audio it receives" | `ml-link-state` (mark + word, tooltip = detail) + "History" button; detail text below (mono, xs) |
| 4 | Buffer | "The phone's playback buffer: Auto lets the phone adapt it; Fixed keeps the length set here. Under it, the cushion the phone reports it plays" | select Auto/Fixed + (Fixed only) number field + "ms"; detail line; said line |
| 5 | Redundancy | "Redundancy: every packet sent twice, the copy right after it or a few blocks later; it doubles this musician's bandwidth, and a later copy adds that much latency. Under it, the copies sent and the packets the phone reports a delayed copy recovered" | switch + "On"/"Off"; Copy select (only while on); cost line; hold note; counts; said line |
| 6 | Mix | — | e.g. "85% · live" |
| 7 | Sent | — | right-aligned mono, e.g. "1,200" |
| 8 | Discarded | — | right-aligned mono |
| 9 | Loss | "Packets this computer discarded for the phone" | right-aligned mono, accent colour when > 0 |
| 10 | (hidden "Actions") | — | Remove / confirm prompt, right-aligned |

Selected row (link panel open for it) gets `data-selected` and a raised background; every body row has `cursor: pointer` (:94-99).

###### A2. Controls

| Control | Label / aria-label | Range / keyboard | States | Command | Source |
|---|---|---|---|---|---|
| Name field | aria-label `Name of ${name \|\| address}`; title = one of the 3 titles in A3 | Enter → blur (commits); Escape → restore saved name, blur; commits on `change` if trimmed value ≠ saved | `disabled` (transparent border/bg) when the phone announced a name (`editable === false`); never overwritten while focused (unless it became read-only) | `set_musician_name {address, name: trimmed}` | :507-521, :569-579 |
| History button | text "History"; aria-label `Link history of ${label}`; `aria-pressed` true on the open row | click | accent border/text when pressed | toggles the panel (B) | :432, :522, :598-599 |
| Row click | — | click anywhere not inside `input, button, select, label, a` | — | same as History | :552-558 |
| Buffer mode select | options "Auto" / "Fixed"; aria-label `Playback buffer of ${label}` | change sends at once | hidden (whole buffer line) when `!buffer.supported`; shows a pending choice until settled (N12) | `set_musician_buffer {musicianKey, mode: "auto"\|"fixed", ms}` | :438-441, :523-527, :610-626 |
| Fixed ms field | aria-label `Fixed playback buffer of ${label}, in milliseconds`; unit "ms" | `type=number min=5 max=500 step=1 inputmode=numeric`; Enter → blur (commit on change); Escape → value = last reported ms or 40, blur | visible only while mode shows Fixed; not overwritten while focused or while a choice is pending | as above with `ms` = typed string, validated by the screen first | :443-446, :528-541, :621-623 |
| Redundancy switch | checkbox `role="switch"`; aria-label `Send ${label}'s audio twice (redundancy)`; word "On"/"Off" | change sends at once | shows a pending choice until settled (N16) | `set_musician_redundancy {musicianKey, on, delayBlocks: null}` | :453-454, :542-546, :641-645 |
| Copy select | aria-label `When ${label}'s copy is sent`; title "Immediate: the copy goes right after its packet, with no added latency. Later: a Wi-Fi burst must last that long to take both, and the phone plays that much later" | options 0-4 blocks; change sends at once | hidden while off; options 1-4 `disabled` when the phone cannot hold packets | `set_musician_redundancy {musicianKey, on: true, delayBlocks: n}` | :456, :547-551, :646-665 |
| Remove (idle) | text "Remove", danger; aria-label `Remove ${label}` | click arms | — | none yet | :685-694 |
| Remove (confirm) | prompt `Remove ${label}?` + danger "Remove" + "Keep" (Keep last = where Remove was; Keep focused) | confirm works only ≥600 ms after arming; Escape on the row disarms; auto-disarm after 10 000 ms | confirm `aria-disabled="true"` (50% opacity) until ready | `remove_target {address}` | :696-744 |

###### A3. Displayed values and formats
- Name field titles (:48-57):
  - phone-announced: "Announced by the musician in their own app; the name is set on their phone, not on this desk"
  - local, phone has client id: "Local label for this desk, kept for this phone even when its address changes, and shown only while the musician has not announced a name. Enter saves, Escape reverts"
  - local, no client id: "Local label for this desk, kept for this address, and shown only while the musician has not announced a name. Enter saves, Escape reverts"
- Link (format.js:110-154): words Good / Warning / Poor / Unknown for levels good / warning / bad / unknown, with detail "Loss 0.82% · Glitches 0.0/s · Timeouts 0.0/s" (`loss.toFixed(2)`, `glitches.toFixed(1)`, `timeouts.toFixed(1)`). Special cases: "Link lost" / "Reports from this phone stopped"; "Not connected" / "The app on this phone is not connected"; "No reports" / "No reports from this phone yet"; "No audio" / "This phone receives no audio".
- Mix: `${percent}% · muted` or `· live` (network-view.js:78; percent clamped 0-100, format.js:61-63).
- Sent / Discarded: `toLocaleString("en-US")` ("1,200") (network-view.js:46-48).
- Loss: `${(discarded/(sent+discarded)*100).toFixed(2)}%`, "0.00%" with no packets (network-view.js:64-67,81).
- Buffer detail (playback-buffer.js:28-41, musicians-table.js:627-630): supported → "plays 42.7 ms" (`cushionMs.toFixed(1)` from `linkLatest.cushionMs`, empty before a report), title "The cushion the phone reports it plays"; unsupported → "needs a newer app", title "This phone's app does not take its playback buffer from the desk; update BandEar on it".
- Redundancy cell (redundancy.js:83-122), block = `blockMsOf(captureFormat)` (`n*1000/sampleRate` for `Fixed(n)`, else 256/48 = 5.333 ms):
  - Copy options: "Copy: immediate", "Copy: 1 block later (+5.3 ms)", "Copy: 2 blocks later (+10.7 ms)", … up to 4; when the phone cannot hold packets each 1-4 label ends ", needs a newer app".
  - Cost line (always shown, title "Every packet is sent twice, so a lost one can still arrive as its copy"): off "Sends once; on doubles this musician's bandwidth"; on immediate "2× bandwidth, no added latency"; on delayed "2× bandwidth, +10.7 ms latency on this phone".
  - Hold note (only while on and unsupported): "Delayed copies: needs a newer app", title "This phone's app cannot hold packets back for a delayed copy; update BandEar on it".
  - Counts (only while on): `${duplicatesSent} copies sent · ${recovered}`; recovered = "Recovered packets not reported by this phone" (no `redundancySupported`) / "Recovered packets are counted for delayed copies only" (delay 0) / "1 packet recovered" / "1,234 packets recovered".
- Row outcome lines (role status, red when `ok` false): buffer — formatBufferSet (playback-buffer.js:82-110):
  - "Sent Ana's phone a fixed 40 ms buffer." / "Ana's phone is back on Auto." / "Kept for Ana: their phone gets a fixed 40 ms buffer when it is connected again." / "Kept for Ana: their phone gets Auto when it is connected again." / (error) "Bass's phone needs a newer app to take its buffer from the desk." / (error) desk's message or "The buffer was not set." / thrown: "The buffer was not set: X."
  - save failure appended: " It was not saved for the next start: disk full."
  - local validation errors (playback-buffer.js:48-70): "Type the buffer in whole milliseconds, from 5 to 500." / "A fixed buffer is from 5 to 500 ms, not 501 ms." / "The buffer is Auto or Fixed, not X."
- Redundancy outcomes (redundancy.js:130-155): on immediate "Ana's audio is now sent twice, each copy right after its original: their bandwidth doubles (×2), with no added latency."; on delayed "Ana's audio is now sent twice, each copy 2 blocks later: their bandwidth doubles (×2) and their phone plays 10.7 ms later."; off "Ana's audio is sent once again: their bandwidth is back to normal (×1)."; (error) "Bass's phone needs a newer app for a delayed copy; an immediate copy works with any app."; (error) desk message or "Redundancy was not set."; thrown "Redundancy was not set: X."; save failure appended as for the buffer. Name in both = musician name or IP (`nameForKey`, playback-buffer.js:137-140; `musicianLabel`, format.js:163-170).
- Table-level errors (network-screen.js:233,257): `Could not remove ${address}: ${message}`, `Could not save the name: ${message}` (no final full stop).

###### A4. Rules
- N1. Link level comes from the server: bad when the report received 0 packets, any receive timeout, underruns ≥ 2/s, or loss ≥ 2 %; warning when any underrun or loss ≥ 0.5 %; else good. A report older than 3 s is bad while the control channel is connected, unknown once it closed; no report ever is unknown (apps/server/src/health.rs:21-30, 87-97, 174-186). The UI only maps level → word (format.js:121-154).
- N2. Without all three rates (fresh report): bad → "Link lost"; not connected → "Not connected"; else "No reports". Bad with `packetsPerSecond === 0` → "No audio" (format.js:128-143).
- N3. Mark shape repeats the level (never colour alone): good filled dot (green), warning triangle (accent, 10×9 px), bad square (red, radius 1px), unknown ring (muted) (link-state.js:18-60).
- N4. Name ownership: `editable = !clientName`; a local label is offered only while the phone announced none; `labelFollows` "phone" with a client id, else "address" (network-view.js:61-76).
- N5. Rows are rebuilt only when the set of addresses changes, so a field being typed in keeps focus and text (musicians-table.js:392-411). A field turned read-only meanwhile commits nothing (:516-520).
- N6. Row click selects (toggle) unless the click hit an `input, button, select, label, a` (:552-558). Selecting the open musician again closes the panel (link-health-controller.js:80-87).
- N7. Remove: arm → prompt; confirm enabled after `CONFIRM_DELAY_MS = 600` ms (network-view.js:44, `confirmReady` :250-252); auto-disarm after 10 000 ms (musicians-table.js:58, 731-744); Keep or Escape (on the row) disarms; arming another row disarms the first; a row leaving the list disarms (:398-400).
- N8. Remove never restarts the engine; the hint explains the phone's control channel stays (B/C hint text, :354-359).
- N9. Buffer modes: Auto or Fixed whole ms 5-500, default Fixed length 40 (playback-buffer.js:15-18). Validation in the screen before sending (network-screen.js:238-246): `^\d+$` after trim, then range.
- N10. Buffer control shown only when `bufferSupported` (phone announced "playback.buffer"); otherwise only "needs a newer app" (playback-buffer.js:1-9, 28-41).
- N11. Buffer commits: mode select change sends immediately (Fixed sends the current field value); field sends on `change` (Enter or blur) only while Fixed; Escape restores the last reported ms (or 40) (musicians-table.js:523-541).
- N12. Pending buffer choice (playback-buffer.js:119-134): shown over the reported value until the row reports the same mode (and same ms by numeric value: "040" settles 40) or a new outcome for that row arrives.
- N13. Buffer outcomes `sent` and `notConnected` are ok; `unsupported` and anything else are errors (playback-buffer.js:82-110). The desktop queues buffer/redundancy changes behind desk edits and session loads (`inTurn`, console-app.js:796-800, 971-1001) and re-reads status after.
- N14. Redundancy: off by default; on doubles that musician's bandwidth only; delay 0-4 blocks, default 0 = immediate (redundancy.js:21-23; apps/server/src/redundancy.rs:39-46). Switching on sends `delayBlocks: null` (server applies default 0).
- N15. Delays 1-4 need `redundancyHoldSupported` ("redundancy.hold"); recovered counts need `redundancySupported` ("redundancy.recovered"); immediate copies are never counted as recovered (redundancy.js:1-15, 83-97).
- N16. Pending redundancy choice settles when the row reports the same on/off and (when on) the same delay, or a new outcome arrives (redundancy.js:164-176).
- N17. Copy options are rebuilt only when their labels change (block size or phone) (musicians-table.js:646-659).

###### A5. Empty / error / disabled
- Empty: "No musicians yet. A phone that finds this computer appears here on its own." (:345), hidden when rows exist.
- No engine: rows empty (the screen note in 0.1 shows).
- Errors: table error (`role="alert"`) for remove/rename; per-row said lines for buffer/redundancy.
- Disabled: name field (phone-announced name); confirming Remove (`aria-disabled`) for 600 ms; Copy options 1-4 (unsupported phone).

###### A6. Accessibility
- Table with `th scope="col"`; the actions header is a visually hidden "Actions".
- Labels listed in A2; dot and link mark are `aria-hidden`; `ml-link-state` exposes the word as text and the detail as `title`.
- Live regions: table error and add error `role="alert"`; per-row buffer/redundancy said `role="status"`.
- Focus: Keep gets focus when the confirm prompt appears (:728). `:focus-visible` 2px outline.

---

##### B. Link-health history panel (`<ml-link-health>`, components/link-health.js; logic link-health-view.js, link-health-controller.js)

###### B1. Layout (link-health.js:302-335)
Card, in order:
1. Head: `h2` title `${name} · ${range.long}` (e.g. "Bass · last 5 minutes"), flexible spacer, "Close" button.
2. Range segmented group (`role="group" aria-label="Range"`): "1 min", "5 min", "10 min" (mono, min-width 64 px; pressed = accent fill).
3. Summary line: `ml-link-state` (level + word) + summary text.
4. Error message (`role="alert"`, hidden when null).
5. Empty message (hidden when null; when shown, the chart is hidden).
6. Chart: SVG (focusable) with the data group and a crosshair line; floating readout box; hidden description paragraph.
7. Extras note; recovered note (each hidden when null).
8. Key text: "A line per measure, one point a second. Shaded: seconds with no report. A vertical line: the phone reconnected. A tick: an underrun or a timeout in that second."
9. Toggle button "Show as table" / "Hide the table".
10. Table (scrolls, max-height 320 px, sticky header). Columns (:265-276): Time, Level, Loss, Round trip, Jitter*, Cushion*, Underruns, Timeouts, Recovered*, Note (* only when the phone reports that extra). First two and last columns left-aligned, the rest right-aligned, mono tabular numbers.

###### B2. Controls

| Control | Label / aria-label | Keyboard | States | Effect | Source |
|---|---|---|---|---|---|
| Close | "Close"; aria-label `Close the link health of ${name}` | click | — | closes, stops polling | :306, :359, :405 |
| Range buttons | text = label; aria-label `Last ${label}` ("Last 1 min"); `aria-pressed` | click | pressed one = current range | `link-range {rangeMs}` → re-window; no new read | :434-452 |
| Table toggle | "Show as table"/"Hide the table"; `aria-expanded`, `aria-controls="table"` | click | — | `link-table {shown}`; table data built only when shown | :327, :360-362, :421-427 |
| Chart (SVG) | `tabindex=0 role="img"`, aria-label = title, `aria-describedby` = description | see B6 | `:focus-visible` outline offset 4 | hover/keys show crosshair + readout | :313, :363-374, :591-621 |

###### B3. Displayed values (link-health-view.js)
- Ranges (:21-25): 60 000 "1 min" "last minute"; 300 000 "5 min" "last 5 minutes"; 600 000 "10 min" "last 10 minutes". Default 600 000 (:28).
- Summary parts joined " · " (:508-552):
  - "No link reports yet" when the window is empty (alone).
  - `Loss ${x.toFixed(2)}% over ${range label}` (Σlost / Σ(lost+received) over non-gap samples) or "Loss — over 5 min".
  - `Round trip ${now} now, ${avg} on average` (ms `toFixed(1)` after rounding to 0.1, e.g. "30.0 ms"), or "Round trip not measured yet".
  - "No underruns in 5 min" or "3 underruns in 5 min" / "1 underrun in 5 min".
  - only if > 0: "1 timeout"/"N timeouts"; "N s without reports"; "1 reconnect"/"N reconnects".
  - only when samples carry `recovered`: "7 packets recovered by redundancy in 1 min" (if > 0) or, while redundancy is on, "No packets recovered in 1 min".
- Summary level/word = the table's link state (N1-N2), e.g. "Link lost".
- Description (screen-reader text): `Link health of ${name} over the ${long}: ${label}. ${summary text}.` (:639).
- Extras note (:624-628): "Jitter and cushion: not reported by this phone (it needs a newer app)." / "Jitter: not …" / "Cushion: not reported by this phone (it needs a newer app)."
- Recovered note (only while redundancy on and recovered not reported): "Recovered packets: not reported by this phone (it needs a newer app)." (:629-632).
- Error: `The link history could not be read: ${error}.` (:645).
- Empty: "No link reports from this phone yet: the chart starts with its first report." (:646-649).
- Readout (sampleTooltip, :558-588): head = level word ("Good"/"Warning"/"Poor"/"No report") + local time "21:30:05"; lines `[label, value]`: Loss "0.50%", Round trip "10.0 ms", Jitter*, Cushion*, Underruns "1", Timeouts "0", Recovered*; missing values "—". Gap sample: title "No report", only "Round trip" if present, note "No report arrived for this second". Reconnected sample note: "Reconnected: the time before it went unmeasured".
- Table rows (historyTable, :591-608), newest first: time, level (gap → "No report"), loss, rtt, jitter, cushion, underruns, timeouts, recovered (gap → "—" except rtt), note "Gap" / "Reconnected" / "".
- Clock: `Intl.DateTimeFormat("en-GB", {hour, minute, second: "2-digit", hour12: false, timeZone})` → "21:34:05" (:443-457).
- Axis labels: see section 8.

###### B4. Rules
- N18. Panel opens on History or row click; picking the open musician again closes it; picking another switches (link-health-controller.js:80-87).
- N19. Panel closes and polling stops when the musician leaves the list (removed, or engine stopped) and on unmount (:59-76, :96-98).
- N20. A phone listed by address that registers (`address:` → `client:` key) keeps the panel, followed at the same address; a row that already had a client id is never followed to another at its old address (link-health-view.js:60-69; controller :70-74).
- N21. Polling: first read `sinceSeq: null` (whole), then every 1000 ms only samples after the newest `seq` held, appended and capped to `maxSamples` (default 600) (link-health-view.js:31-34, 82-96, 105-181).
- N22. A new `historyId` answered to a cursor read → keep the old buffer and immediately read whole (`refetch`) (:82-86, :132-134).
- N23. A failed read keeps the held history, shows the error (only when it changed), retries next interval; the next good read clears it (:144-153). Answers for a musician no longer followed are dropped (generation counter, :116-157).
- N24. Range and table toggle belong to the panel: range persists across musicians; closing resets the table to hidden but keeps the range (controller :28-35, 48-51, 89-94).
- N25. Window = samples with `monoMs > endMs − range` up to the newest, on the monotonic clock; wall-clock `atMs` is only for labels (:187-199).
- N26. Panels: Loss (%) floor 1, Round trip (ms) floor 10, then Jitter (ms) floor 5 and Cushion (ms) floor 50 only when extras reported; each own scale `niceMax` (steps 1, 2, 2.5, 5, 10 × 10^k, never below floor) (:201-230).
- N27. Extras: jitter/cushion shown when the phone announced "stats.v2" or any sample has the value; recovered when "redundancy.recovered" announced or any sample has it (:484-493).
- N28. A line breaks at a missing value, a reconnected sample, a gap sample, or a spacing > 1.5 s; a run of one sample is a dot (:253-276, :311-321).
- N29. Lanes: Underruns and Timeouts, a tick for each sample whose count > 0 (:232-235, :339-347).
- N30. Hover: nearest sample by monotonic time, accepted within plot ±8 px (binary search, :416-438). Pointer leave hides unless the SVG has focus (link-health.js:363-371).

###### B5. Empty / error / disabled
- No samples: summary "No link reports yet", empty message, chart hidden, table (if open) empty.
- Read error: alert line with message; chart keeps the last history.
- Hidden entirely (`hidden`) when the view is null (no musician picked) (link-health.js:396-402).
- CSS `.refetching svg { opacity: 0.6 }` exists (:115) but no code sets the class (unused).

###### B6. Accessibility / keyboard map (link-health.js:591-621)
- Focus on SVG → show the readout at the newest second (if nothing shown); blur → hide.
- ArrowLeft −1 s, ArrowRight +1 s, PageUp −10 s, PageDown +10 s (clamped), Home → oldest, End → newest, Escape → hide. Keys are `preventDefault`ed.
- The summary is the chart's text alternative (`aria-describedby` → hidden description); the table holds every value without hover. Error line `role="alert"`.

---

##### C. Add / remove phone (musicians-table.js:347-359; network-view.js:217-247; network-screen.js:211-234)

###### C1. Layout
Below the table: inline form — label "Add a phone by address", text field (16em, mono, placeholder "192.168.1.50:50000", autocomplete off, spellcheck off), primary button "Add phone"; error line (`role="alert"`) wraps to its own row. Then the hint:
"For a fixed rack or for debugging. Adding or removing a phone never restarts the engine: the capture, the control server and the discovery beacon keep running. Removing stops that phone's audio at once; its control channel stays open and its next mix is answered with "no UDP target configured for client IP"."

###### C2. Controls

| Control | Label | Keyboard | States | Command | Source |
|---|---|---|---|---|---|
| Address field | label "Add a phone by address" | Enter submits (form, `novalidate`) | `aria-invalid="true"` while an add error shows; cleared after success | — | :348-350, :390 |
| Add phone | "Add phone" | click / Enter | never disabled | `add_target {address}` after local validation | :351, :369-372 |
| Remove | see A2 | | | `remove_target {address}` | A2 |

###### C3. Values / messages
- Validation (network-view.js:226-247), input trimmed:
  - empty → "Type the phone's address, for example 192.168.1.50."
  - whitespace inside → `An address has no spaces: ${address}`
  - only digits/dots/colons but not `a.b.c.d[:port]` → `${address} is not an IP address such as 192.168.1.50.`
  - IPv4 part > 255 → `${address} is not an IP address: each part goes up to 255.`
  - trailing `:port` outside 1-65535 (not checked for a bare unbracketed IPv6) → `The port must be from 1 to 65535, not ${port}.`
  - accepted: "192.168.1.50", "192.168.1.50:50000", "[fe80::1]:50000", "stage-rack.local" (names are the engine's to refuse).
- Engine refusal: `Could not add ${address}: ${message}` (network-screen.js:222).

###### C4. Rules
- N31. Validate first; on failure show the message and send nothing (network-screen.js:211-216).
- N32. On success clear the field (`clearAddress`) and the error; then the status is re-read so the table updates (`musicianCommand`, console-app.js:776-784).
- N33. Remove is the guarded two-step of N7; its failure shows in the table error line.

###### C5 / C6. See A5/A6 (alert lines; `aria-invalid` on the field).

---

##### D. Session panel with named sessions and Mix backups (`<ml-session-panel>`, components/session-panel.js; sessions.js; mix-backups-view.js)

###### D1. Layout (session-panel.js:211-274)
Card titled `h2` "Session":
1. Desk notice (`role="status"`, warn box): `When the console opened its saved session: ${notice}.` (sessions.js:107-109).
2. Save field: label "Save session as", text input (maxlength 40), button "Save".
3. Idle hint (only with no engine): "The engine is not running: the session is saved without the musicians' mixes."
4. Separator.
5. Unread line: "Listing the saved sessions…" or `The saved sessions could not be listed: ${error}.` + button "List them again".
6. None line: "No saved sessions yet."
7. (When rows exist) "Saved sessions" select + primary "Load"; detail line; load-confirm box; "Rename to" input (maxlength 40) + "Rename"; delete slot.
8. Hint: "Loading replaces the channels, groups, presets, labels, limits, locks and mirrors, and sends each saved mix to the musicians who are connected. A phone that is not connected keeps its own mix. The saved audio input is used from the next start."
9. Separator; Mix backups group (`role="group"`, `h3` "Mix backups" + "Refresh"): unread line, none line, list, restore-confirm box, idle hint, hint "The musicians' mixes are backed up a few seconds after they change, and the last 10 are kept. Restore sends each backed-up mix to the musicians who are connected now; a phone that is not connected keeps its own mix."
10. Outcome lines: success (`role="status"`) and failure (`role="alert"`, red).

###### D2. Controls

| Control | Label / aria-label | Keyboard | States | Command | Source |
|---|---|---|---|---|---|
| Save name | "Save session as", maxlength 40 | Enter = Save | cleared after an ok save | — | :215-216, :310-315 |
| Save | "Save" | click | never disabled; blank (trimmed) name does nothing | `save_session {name: trimmed}` | :217, :308-309; sessions.js:285-291 |
| List them again | "List them again" | click | only in the unread line | `sessions` | :221, :316 |
| Saved sessions select | label "Saved sessions"; option text = row label | change | choice sticks to its id while listed, else first row | — | :226, :317-320, :440-452 |
| Load | "Load", primary; aria-label `Load ${name}`; aria-describedby detail | click | `aria-disabled` when chosen row not loadable | `load_session {id}` at once, or ask first if `unsavedEdits() > 0` | :227, :321-327, :455-456 |
| Cancel (load ask) | "Cancel" | click / Escape in box → focus Load | focused when the ask opens | none | :233, :328-344 |
| Load anyway | "Load anyway", danger | click | — | `load_session {id}` | :234, :333-336 |
| Rename to | label "Rename to", aria-label `New name for ${name}`, maxlength 40 | Enter = Rename | cleared after an ok rename | — | :238-239, :376-381 |
| Rename | "Rename" | click | `aria-disabled` when not loadable; blank does nothing | `rename_session {id, name: trimmed}` | :240, :374-375, :466 |
| Delete (idle) | "Delete", danger; aria-label `Delete the saved session ${name}` | click arms | — | — | :528-541 |
| Delete (armed) | "Keep" (first, focused) + `Delete ${name}? The desk's session does not change.` + danger "Delete" | Escape on Keep = keep; confirm after 600 ms; auto-disarm 10 000 ms | confirm `aria-disabled` until ready | `delete_session {id}` | :547-578; sessions.js:337-371 |
| Refresh | "Refresh" | click | — | `list_mix_backups` | :253, :347 |
| Restore (row) | "Restore"; aria-label `Restore the mixes backed up at ${label}` | click | `aria-disabled` when damaged or no engine (click ignored) | asks first | :499-506, :348-358 |
| Cancel (restore ask) | "Cancel" | click / Escape in box | focused when ask opens | none | :261, :359-373 |
| Restore (confirm) | "Restore", primary | click | — | `restore_mix_backup {stamp}` | :262, :364-367 |

###### D3. Displayed values
- Session option label: name, or `${name} (cannot be loaded)`; detail: `Saved 16 Oct 2026, 19:05 · 3 musicians' mixes` (date part omitted when unknown; "no mixes" / "1 musician's mix" / "N musicians' mixes") or `It cannot be loaded: ${problem}.` (sessions.js:57-96). Date: day without padding, English 3-letter month, `HH:MM` 24 h local.
- Load ask text: `1 change is still on the way to the engine. Loading "${name}" drops them.` / `${n} changes are still on the way …` (session-panel.js:458-465).
- Outcomes (console-app.js:1705-1723, sessions.js:117-205):
  - save: `Saved the session as "X" without mixes: the engine is not running.` / `Saved the session as "X". No musician was listed, so no mix was kept.` / `Saved the session as "X", with 3 musicians' mixes.`; failure `The session was not saved: ${error}.`
  - rename: `Renamed the saved session "A" to "B".`; failure `The saved session was not renamed: X.`
  - delete: `Deleted the saved session "X".`; failure `The saved session was not deleted: X.`
  - load failure: `"${name}" was not loaded: X.` (name falls back to "the saved session").
  - load success, sentences in order: `Loaded "X": its channels, groups, presets, labels, limits, locks and mirrors apply now.` or `Loaded "X": it applies at the next start.`; `It runs, but was not saved: E.`; `Pushed the saved mix(es) to A, B and C.`; `A follows B's mix, so only their master volume and mute were set.`; `The saved mix was not pushed: the engine is not running.` / `The N saved mixes were not pushed: the engine is not running.`; `Not pushed to A: not connected, and a phone's own mix wins when it connects.`; `Not pushed to A: not here.`; `Not pushed to A: E.`; `Not applied: r1; r2.`; `Part of the saved session could not be read and was left out: d1; d2.`; `Its audio input is used from the next start.` or `Its audio input was not taken: E. The desk keeps its own.`; `The local mix is the saved one.`; the local output notice verbatim; extra `Its channel processing could not be read: E.` (console-app.js:1238-1240).
  - Name joining: "A", "A and B", "A, B and C" (mixes.js:131-136). Skipped name: kept label, else listed musician's label, else the address, else "a phone" (sessions.js:212-224).
- Desk-side name errors (src-tauri/src/sessions.rs:306-326, mirrored in sessions.js:26-39): "a saved session needs a name", "a session name is at most 40 characters", `a session named "X" is already saved` (case-insensitive).
- Backups row: when label `18 Oct 2026, 19:04:05` (to the second) or `Backup ${stamp}` when damaged/unknown time; detail "no musicians" / "1 musician: Ana" / "3 musicians: Ana, Bass and Rack" (or without names) or `It cannot be restored: ${problem}.` (mix-backups-view.js:40-77).
- Restore ask: `Restore the mixes backed up at ${label} (${detail})? Each connected musician's phone is sent their backed-up mix now.` (session-panel.js:515-519).
- Restore outcome (mix-backups-view.js:99-130): `Restored the mixes backed up at ${time} to A and B.` (" at …" omitted when unknown) or "No musician's mix was restored from the backup."; follower sentence as for load; `Not restored to A: not connected, and a phone's own mix wins when it connects.`; `Not restored to A: not here.`; `Not restored to A: E.`; failure `The mixes were not restored: E.` (console-app.js:961).
- Backups list outcome: ok → nothing said; failure → `The mix backups could not be listed: E.` in the outcome alert (sessions.js:404-407, console-app.js:929-941).

###### D4. Rules
- N34. Names: trimmed, 1-40 characters (counted as code points), unique case-insensitively (renaming may keep its own name) — enforced by the desk (sessions.rs) and the fake bridge; the inputs cap at 40 (`maxlength`).
- N35. Save keeps the whole desk session and, while an engine runs, every musician's mix (sessions.js:5-11).
- N36. Load asks first when `unsavedEdits() > 0` (mix edits pending + strip edits pending + channel/group edits not answered, console-app.js:1255-1262); "Load anyway" loads, Cancel/Escape drops the ask; choosing another session drops the ask and disarms delete (sessions.js:276-322).
- N37. Load runs in turn after earlier desk edits; edits held while it loads are dropped if it loaded, sent if it was refused (console-app.js:1186-1251). A loaded audio input applies at the next start; the engine is not restarted.
- N38. Unloadable (damaged) sessions: listed with "(cannot be loaded)", Load and Rename `aria-disabled` and inert (sessions.js:293, 329).
- N39. Delete: two-step, Keep takes Delete's place, confirm only after 600 ms, auto-disarm 10 s, Keep/Escape keeps (sessions.js:337-371; mix-tools.js:13).
- N40. After every session command the list is read again (`sessionCommand`, console-app.js:916-925); outcome is said; an ok save/rename clears its field; any answered load clears the ask (sessions.js:377-382, 396-426).
- N41. Backups: the desk backs up live mixes ~3 s after they go still (at most 20 s after the first change) and on engine stop/quit, keeping the newest 10 (mix-backups-view.js:5-22). Listed newest first.
- N42. Restore asks first (only for a restorable row); needs an engine (button `aria-disabled` otherwise); sends each backed-up mix to connected phones only, never queued; runs in turn with desk edits; re-reads status and backups after (mix-backups-view.js:136-173; console-app.js:946-966). A list refresh drops an ask whose row vanished.

###### D5. Empty / error / disabled
- Sessions listing: "Listing the saved sessions…"; failure `The saved sessions could not be listed: ${e}.` + "List them again"; empty "No saved sessions yet.".
- Backups: "Listing the mix backups…"; `The mix backups could not be listed: ${e}.`; empty "No backup yet: the musicians' mixes are backed up a few seconds after one changes."; idle (rows exist, no engine) "Start the engine to restore a backup.".
- Disabled (aria-disabled, inert): Load/Rename (unloadable), Delete confirm (600 ms), Restore (damaged or no engine).

###### D6. Accessibility
- Section `aria-labelledby` "Session"; backups `role="group"` labelled "Mix backups"; list `aria-label="Mix backups, the newest first"`.
- Confirm boxes (load ask, restore ask) are `role="alert"`; outcome lines: status (ok) / alert (error); desk notice `role="status"`.
- Focus moves: Cancel on ask open, Load after cancel, Keep on delete arm.

---

##### E. Audio setup (`<ml-audio-setup>`, components/audio-setup.js; network-view.js; engine-config.js; local-mix.js; demo-source.js)

###### E1. Layout (audio-setup.js:158-207)
Card `h2` "Audio input"; a two-column grid (110 px labels):
1. Recording lock caution (`role="status"`, full width, hidden unless locked).
2. "Driver" select.
3. ASIO caution (full width, while an ASIO choice waits): "**ASIO loads a kernel-mode driver**: an unstable one can restart the whole machine, so it is never chosen for you. Pick the driver deliberately, preferring your interface's own over a built-in one; nothing starts until you press Start on this driver."
4. "Input device" select; devices error line (`role="alert"`).
5. Switch warning box (`role="alert"`): text + "Switch anyway" + "Cancel".
6. Apply row (only while ASIO waits): primary "Start on this driver".
7. "Buffer" select.
8. "Local mix out" select (`aria-describedby="local-state local-hint"`), state line, alert line, notice line (caution), error line, said line (`role="status"`), hint line.
Then a definition list: Capture format, Sample rate, Control port, Samples received, Packets sent / discarded, Engine load. Then the hint: "The driver, the buffer and, outside WASAPI, the device restart the capture engine, the way a DAW does when the driver changes: the phones reconnect on their own. The buffer only takes effect on ASIO, where the driver honours it; WASAPI shared mode pins the period to the device's own (10 ms). The sample rate is 48 kHz by design: it is the format on the wire the phones receive, not a free choice."

###### E2. Controls

| Control | Label | Options / range | States (disabled + reason) | Command, when | Source |
|---|---|---|---|---|---|
| Driver | "Driver" | "Auto — the system's own (safe)", "WASAPI (shared)", "ASIO — kernel driver (risky)", "Demo (no interface)" (values auto, wasapi, asio, demo) | disabled while recording or soundchecking (lock text shown) | to non-ASIO different from applied: `start_engine` (new driver, its default input), then `set_audio_settings`, then `list_input_devices`; to ASIO (not applied): no start, only `list_input_devices {host:"asio"}` | network-view.js:28-33; network-screen.js:145-160 |
| Input device | "Input device" | `Name — N channels` / `— 1 channel` / `— channels unknown`; while ASIO waits first option "Choose an ASIO driver…" (value ""); empty list "no input devices" or "Listing the devices…" | disabled when recording, no devices, or (not ASIO-pending and no running engine / engine stopped) | ASIO pending: only sets draft; else `deviceSwitchPlan` → confirm / `switch_device` (WASAPI) / `start_engine` with `deviceFilter` (others) | network-view.js:90-98,112-131,173-215; network-screen.js:162-179 |
| Switch anyway | "Switch anyway" | — | in the warning box | `switch_device` or restart | audio-setup.js:175; network-screen.js:181-188 |
| Cancel | "Cancel" | — | — | drops the pending switch | :176; network-screen.js:273 |
| Start on this driver | "Start on this driver" (primary) | — | visible while ASIO waits; disabled until a driver is picked or while recording | `start_engine {host:"asio", deviceFilter: driver}`, then `set_audio_settings`, `list_input_devices` | :178-179; network-screen.js:190-197 |
| Buffer | "Buffer" | "128 frames — lowest", "256 frames — balanced", "512 frames", "1024 frames — safest" | disabled while recording/soundchecking | engine running and no ASIO draft: `start_engine` on the device in use (keepDevice), then `set_audio_settings`; else kept for the next start only (no command) | network-view.js:36-41; network-screen.js:199-209 |
| Local mix out | "Local mix out" | "None (lowest load)" + every whole stereo pair `${device} · outputs 1/2`, `3/4`, …; plus `${label} (not offered now)` for an in-use output not listed | disabled with no engine, while the outputs are being listed, or while the engine is stopped | `set_local_output {output: {device, firstChannel} \| null}` then `engine_status` | local-mix.js:58-66,107-148,216-224 |

###### E3. Displayed values
- Readouts (network-view.js:204-213), "—" with no engine: Capture format `${channels} ch · 48 kHz · 24-bit · 256` (formatCaptureFormat: "44.1 kHz", "24-bit"/"32-bit float"/"16-bit unsigned", `Fixed(256)` → "256", "device default" → "default buffer", format.js:75-108); Sample rate "48,000 Hz"; Control port "50001"; Samples received "1,234,567"; Packets sent / discarded "1,200 / 30".
- Engine load (network-view.js:143-157): `${round(avg)}% (peak ${round(peak)}%)` plus ` · 1 late block` / ` · 1,200 late blocks`; "—" with no engine or no `load`. Warning style (warn box, accent text) when either rounded value > 70.
- Switch warning (network-view.js:122-130): `Switching to ${name} (${channels} ch) makes ${n} group(s) unavailable: ${names joined ", "}. They are kept, not deleted, and become available again if you switch back.`
- Devices error: `Devices could not be listed: ${error}` (network-screen.js:118).
- Recording lock: "Stop the recording before switching the input device or its driver: a take never mixes two devices." (recording.js:37-39) or "Go back to the live inputs before switching the input device or its driver: a soundcheck plays a take in place of the inputs." (soundcheck.js:36-38).
- Local mix out state line (local-mix.js:75-148): no engine "Start the engine to choose where the local mix plays."; listing "Listing the outputs…"; running `Playing on Volt 4 · outputs 3/4: no gaps, nothing dropped.` / `…: 12 silent frames filled in, 3 frames dropped to hold the delay.` (singular "1 silent frame", "1 frame"); off "Off: the local mix plays nowhere."; failed "Stopped: the local mix plays nowhere." with alert `${label} stopped: ${reason}. Choose it again, or another output.`
- Local notice line: the status's `localOutput.notice` verbatim; error line `The outputs could not be listed: ${e}.`; hint "The local mix plays at 48 kHz on the pair you choose. Under ASIO only the input driver's own outputs are offered; WASAPI offers every output device."
- Local said line (formatLocalOutputSet, local-mix.js:252-261; console-app.js:647-696): applied `The local mix plays on Volt 4 · outputs 3/4.` / "The local mix plays nowhere now."; not applied (no engine) `Volt 4 · outputs 1/2 is kept: the local mix plays there from the next start.` / "No output is kept: the local mix plays nowhere from the next start."; plus ` It was not saved for the next start: E.` when not saved; refused `The local mix output was not changed: E.`; unreadable answer `The local mix output was changed, but the desk's answer could not be read: E.`; lane failure `The local mix output may not have changed: E.`; status unreadable appended ` Where it plays now could not be read: E.` Red when `ok` false.

###### E4. Rules
- N43. Driver order: safe first, demo last (network-view.js:24-33). Defaults: host "auto", device null, 256 frames, no ASIO driver (engine-config.js:32-37).
- N44. ASIO is never chosen by default or applied by a driver change alone: choosing ASIO lists ASIO drivers and waits; "Start on this driver" starts on the picked driver, which becomes `asioDriver` (network-screen.js:145-153,190-197; console-app.js:726-735).
- N45. ASIO refusals: `startRequest` returns null for ASIO without a driver, and the console says "ASIO is chosen but no ASIO driver is picked: pick one under Network and system." (as engine error, or as a "start" command failure while an engine runs) without touching the engine (engine-config.js:25-26,138-143; console-app.js:425-435). A kept session audio that names ASIO without a driver restores as "auto" (engine-config.js:95-104). A missing input never falls back to ASIO (fallback to "auto"/same driver default input, engine-config.js:111-114; console-app.js:400-413,437-462). The engine itself refuses a live switch under ASIO, so non-WASAPI switches restart (network-view.js:6-7).
- N46. Only WASAPI switches the device live (`switch_device`); auto/asio/demo restart via `start_engine` with `deviceFilter` (network-view.js:159-162; console-app.js:743-772). A switch that would leave configured groups without inputs (any group channel index ≥ new device channel count) asks first; unknown channel count (0) never asks; picking the current or an unknown device does nothing (network-view.js:100-131).
- N47. Devices listed for another driver are never offered: until the shown driver's list arrives the device select shows "Listing the devices…" (network-view.js:178-180).
- N48. Device shown: running device, else the remembered device for the driver (ASIO → `asioDriver`), else ""; while ASIO waits, the draft driver (network-view.js:182-185; engine-config.js:83-87).
- N49. While a take records, plays as soundcheck, or its files are being finished: driver, device and buffer disabled and the lock text shown; console refuses switches/applies with the same text as a "switch" failure (network-view.js:174-177,193-202; console-app.js:707-719,743-751).
- N50. Choices are applied and saved (`set_audio_settings`) only after an engine started/switched with them; a failed start keeps the previous choices (console-app.js:698-738). While running on the fallback input, a buffer change is only kept for the next start (:720-724).
- N51. Buffer with no engine (or during an ASIO draft): kept for the next start, no restart (network-screen.js:199-205; console-app.js:1620-1624).
- N52. Demo source: driver "demo" with one device "Demo (no interface)" of 8 channels (Kick, Snare, Hi-hat, Bass, Keys, Pad, Vocal, Room); chosen only by the engineer, never by "auto"; restarts like any non-WASAPI driver; title says "Demo source"; Mix-screen top bar shows a "Demo source" chip linking to `#/network` (demo-source.js:11-61; network-view.js:9-10).
- N53. Local mix out: off by default ("None", lowest load), always 48 kHz, only outputs the running driver offers (ASIO: the driver's own; WASAPI: all) as whole stereo pairs; value encoding `${firstChannel}|${device}`, firstChannel 0-based (local-mix.js:6-66). Outputs listed once per engine run; an answer for an ended run is ignored (local-mix.js:195-209; console-app.js:624-633; console-state.js:817-822).
- N54. Local mix out choices go out one at a time; a choice overtaken while waiting is not sent; only the last choice's outcome is said (`superseded`) (console-app.js:635-680; local-mix.js:216-224).
- N55. Engine load warns when the rounded average or peak exceeds 70 % (`LOAD_WARNING_PERCENT`), decided on the rounded numbers so 70.4 does not warn and 70.5 does (network-view.js:133-157).
- N56. Selects are rebuilt only when their option list changes, so an open list is not torn down by status updates (audio-setup.js:316-336).

###### E5. Empty / error / disabled
- Device list: "no input devices", "Listing the devices…", `Devices could not be listed: E`.
- ASIO pending: "Choose an ASIO driver…" option; Start disabled until a driver is picked.
- No engine: readouts and load "—"; device select disabled (unless ASIO pending); Local mix out disabled with "Start the engine to choose where the local mix plays.".
- Recording/soundcheck: driver, device, buffer, Start-on-driver disabled with lock text.

###### E6. Accessibility
- Every select has a `<label for>`; Local mix out is described by its state and hint lines.
- Live regions: lock caution `role="status"`; devices error, switch warning, local alert, local notice, local error `role="alert"`; local said `role="status"`.

---

##### 7. Tests (ui/tests, `node:test`; counts are `test(` calls)

| File | Tests | Behaviours pinned |
|---|---|---|
| network-view.test.js | 18 | row fields (name ownership, link word/detail, mix "85% · live", "1,200", loss "2.44%", buffer and redundancy cells); label follows phone vs address; muted mix; host order + safe/risky labels, buffer values; device labels with channel counts; switch-plan confirm text; WASAPI-only live switch; audio-setup readouts; load "8% (peak 15%)", 70 % rounded thresholds, late blocks "1,200"; dash without engine/load; ASIO waits for a driver; other driver's devices never offered; no engine: no switch, blank numbers; recording lock; remembered vs running device; address validation cases; confirm after 600 ms; window title. |
| link-health-controller.test.js | 10 | select opens+follows; select again closes+stops; select another switches; closes when musician leaves / engine stops; read error shown then cleared; range/table panel-owned, close hides table keeps range; dispose stops and ignores late answers; address→client registration keeps panel; address key follows only a registered row at the same address. |
| link-health-view.test.js | 25 | musician key; ranges 1/5/10 min default 10; append after cursor bounded to ring; new historyId refetch; follower polls only new, every interval, stops; failed read said and retried; switching follow drops old + late answer; window on monotonic clock; niceMax; lines break at gaps/reconnects (3 runs, gap band, x positions, plot right = width−12); isolated dots, missing value breaks; panel order and extras; lanes; time ticks ("−10 min … −2 min", "21:40:00"; "−60 s … −15 s"), last anchor end; reconnect label leftwards and de-duplicated; nearestIndex; summary parts; level = table's; quiet window; tooltip lines/notes; table newest first; extras detection; panel view (title, ranges, notes, error, empty); clock in zone; recovered counts and notes. |
| link-history.test.js | 8 | bridge `link_history` args; fake history shape (600 s, oldest first); deterministic generation; attention link reconnects / lost link ends in gaps; never-reporting phone empty, `sinceSeq` filter; per-musician historyId, restart renews; bad key / stopped engine refused; status rows carry capabilities, rtt, newest sample but no history. |
| playback-buffer.test.js | 9 | bufferView ("plays 42.7 ms", unsupported note, address key); bufferRequest 5-500 whole ms + messages; formatBufferSet sentences incl. save failure; rows carry Buffer cell; bridge `set_musician_buffer` args; fake keeps buffer, cushion follows, older phone refuses; console sets, re-reads status, says outcome; pending choice settles by value ("040") or by new answer. |
| redundancy.test.js | 10 | blockMsOf; Redundancy cell (labels, cost, notes, counts); outcome sentences with bandwidth/latency; pending choice settling incl. delay; rows priced in engine block; bridge `set_musician_redundancy` args; fake keeps across starts, only delayed copies recovered; named session keeps/replaces redundancy; generated history recovered counts; console sets, re-reads, says. |
| sessions.test.js | 11 | name rule (trim, 40, unique any case); rows with saved time and mixes or problem; panel notice and unread; save sentences; load sentences (applied, pushed, skipped reasons); load with no engine; skipped name fallback; load at once vs ask with pending edits; unloadable session neither loaded nor renamed; delete two-step with pause and auto-disarm; screen answers panel after every command, also failures. |
| session-console.test.js | 11 | console reads sessions + notice on connect; save keeps desk session and mixes by key; load pushes mixes, names unreached; next group edit builds on loaded groups; load drops waiting edits; refused load keeps and sends them; pending channel/group edit counts and load waits; loaded audio opened at next start, never ASIO without driver; stopped-engine load applies next start; newer-build channel kind survives edits; rename/delete and unreadable session said. |
| mix-backups.test.js | 11 | backup time to the second; rows with count/names or damaged reason; view unread/engineRunning; restore sentences; Restore asks first, Cancel or new list drops it; console lists on connect, fake backs up when still; only newest kept, unchanged not retaken; restore to connected; not-connected skipped, no-engine/bad stamp refused; failed list keeps known list and says why; panel backup events run console commands. |
| engine-config.test.js | 10 | default start (auto, 256, no targets, port 50001); ASIO only with picked driver, never built without; buffer normalisation; deviceFilter; remembered input per driver, named wins; changing one choice keeps others; new driver → default input unless named; restoredAudio (ASIO without driver → auto); fallback never ASIO; fallback notice text. |
| demo-source.test.js | 8 | demo is its own last driver, never default; start on demo and restore kept demo; badge/title only while demo plays; top-bar badge beside REC chip; audio setup lists the demo device; fake runs demo with no interface; back on interface shows no demo format; console starts demo and keeps choice. |
| local-mix.test.js (Local mix out) | 33 | of these, the audio-setup ones: options None + pairs; value round-trip; select shows listed + in-use; state words/counters, failure alert; kept-not-opened notice, "(not offered now)"; disabled/waiting with no engine or before listing; change outcome sentences; outputs listed once per run; ended-run answer ignored; one-at-a-time last-wins; only last choice's outcome said; status/answer unreadable said and next choice still goes. The rest cover the Mix screen's Local mix tab. |
| engineer.test.js | 25 | Not this screen: the Mix screen's engineer limit and channel locks (`set_engineer_controls`). Listed only because it was named. |

No DOM tests exist for the components; component behaviour (focus moves, Escape handling, row rebuild rules, chart drawing) is untested and must be ported from the component sources.

Gallery (`ui/dev/gallery.js:50-99`, `gallery.html:263-266`): only `ml-link-state` examples — the fake engine's musicians plus "Heavy loss" (bad), "Hears nothing" (bad, 0 packets/s → "No audio"), "Just added" (unknown, connected → "No reports"), "App closed" (unknown, offline → "Not connected"). `ml-status-pill` states are in gallery.html:254-259 (not used on this screen). No gallery entries for the table, link-health panel, audio setup or session panel.

---

##### 8. Custom drawing to reproduce (history chart)

Geometry: `healthLayout` (link-health-view.js:293-413); rendering: `#drawChart` / `#showSeq` (link-health.js:481-577, 623-675); styles link-health.js:108-220. All units CSS px; coordinates rounded to 0.1.

- Width = chart container width (ResizeObserver, rounded; redraw on change) (link-health.js:375-381). Height = computed; SVG `viewBox="0 0 w h"`.
- Anatomy constants (:212-223): PAD_LEFT 72, PAD_RIGHT 12, MARKER_ROW 16 (top strip for reconnect labels), TITLE_ROW 18, PANEL_HEIGHT 56, PANEL_GAP 6, LANE_HEIGHT 18, AXIS_HEIGHT 22, LABEL_ROOM 90, END_LABEL_ROOM 64. Plot: left 72, width `max(1, w−84)`, top 16, bottom = after last lane, axisY = bottom + 15. Height = bottom + 22 (2 panels: 234; 4 panels: 394).
- x mapping: `x = 72 + (monoMs − (endMs − range)) / range × plotWidth` (domain is always the full chosen range, ending at the newest sample).
- Panels stacked: for each, `top = y + 18`, title text at (plot.left, top − 6) class `title` (secondary colour, medium weight); `y` advances by 18 + 56 + 6. y mapping: `top + 56 − min(v, max)/max × 56`.
- Ticks per panel: two horizontal lines across the plot: baseline at 0 (`--ml-border`) and grid at max (`--ml-border-subtle`); labels "0" and max (integer, or 1 decimal) right-aligned at x = left − 8, y + 4, mono muted.
- Shaded bands (gaps): consecutive gap samples (seq contiguous) form one band; `x0 = max(72, x(first.monoMs − 1000))`, `width = max(1, x(last.monoMs) − x0)`; drawn as a rect in every panel (panel.top, height 56), fill `--ml-surface-raised`, drawn before the line.
- Lines: one SVG path per panel, `M x y L x y …` per run (runs ≥ 2), stroke `--ml-blue` 2 px, round joins/caps, no fill; stroke 1.25 px when samples > plotWidth/2 ("dense"). Single-sample runs: circle r 4, fill blue, 2 px stroke in surface colour.
- Event lanes: below panels, Underruns then Timeouts, 18 px each; label at (left − 8, middle + 4) right-aligned (sans, muted); a grid line across at the lane middle; each event a 2×12 rect (rx 1) centred on x at middle (fill secondary text colour).
- Reconnect markers: vertical dashed line (`stroke-dasharray 4 3`, 1 px, muted) from plot.top − 2 to plot.bottom; label "Reconnected" at y = plot.top − 4, x ± 4, anchor "end" when within 90 px of the right edge else "start"; a label is omitted when less than 90 px from the previous labelled one.
- Time axis (y = axisY): ticks counting back from the newest second: step 15 s for ranges ≤ 60 s (labels "−60 s", "−45 s", …), 60 s for ≤ 300 s ("−5 min" … "−1 min"), 120 s otherwise ("−10 min", "−8 min", …); a tick closer than 64 px to the right edge is skipped; anchor "start" at the left edge, else "middle"; the last label is the newest sample's local time ("21:40:00") anchored "end" at the right edge.
- Crosshair: vertical 1 px line (`--ml-text`, no pointer events) at the sample's x from plot.top to plot.bottom; hidden by default.
- Readout box: absolutely positioned, top = plot.top, left = x + 12, or x − 12 − boxWidth if it would overflow the chart width (clamped ≥ 0); min-width 150, padding 8/10, sunken surface, shadow `0 4px 12px rgb(0 0 0 / 40%)`, no pointer events. Content: head row (bold title left, mono muted time right), label/value rows (value mono bold), optional muted note.
- Draw order: per panel (title, ticks, gap bands, path, dots), then lanes, then reconnects, then axis labels; crosshair above data.

##### 9. Notes and discrepancies found
- Stale doc comments say a null redundancy delay means 2 blocks ("1-4, 2 when null": bridge.js:245; "2 when null": console-app.js:1745). Code and server say default 0 (immediate), range 0-4 (redundancy.js:21-23; apps/server/src/redundancy.rs:39-46; src-tauri/src/commands/redundancy.rs:49-57). Port the code behaviour.
- `set_musician_redundancy` can answer `unsupported` (commands/redundancy.rs:95-100, redundancy.js:132-137) although bridge.js's doc lists only `set`/`invalid`.
- Two different "unknown" words: the table/summary says "Unknown" (format.js:111), the history readout/table says "No report" (link-health-view.js:42-47).
- Confirm prompts differ in order: musicians table `Remove X?` · Remove · Keep (Keep last, musicians-table.js:726); session delete Keep · question · Delete (Keep first, session-panel.js:576). Both comments say Keep sits where the original button was.

## Tasks

| ID | Specs | Route | Status | Evidence |
|---|---|---|---|---|
| T1 Pin Rust 1.100.0 in `rust-toolchain.toml` | S9 | inline | done | `f76aa65`; `rustc` in the repo is 1.100.0; assess medium, under budget |
| T2 Extract `bandear-desktop-core`; Tauri crate uses it | S3, S6 | delegated (context backstop) | done | `001ca95` (moves only) + `f653d76`; core 363 tests pass, Tauri 1 test passes and builds; review granted, running |
| T3 Console model foundation + GPUI shell, atoms, adapter | S1, S2 (A), S4, S5 | delegated (context backstop) | done | `73da7f1`..`68999a6` (console values/state/commands/executor/lifecycle/lanes/harness/tests, kit, screen stubs, app shell), `027808f` FakeDesk scripts, `71fbc17` preview; console 164 tests, kit/app/screens 15; app opens, no panic; high commits reviewed: `68a1c58`, `0132ea9`, `32729f0`, `cd48506` (one correction each for `68a1c58` and `cd48506`), `68999a6`, all approved and acknowledged |
| T4 Mix screen | S2 (B) | delegated (parallel unit) | pending | |
| T5 Record screen, soundcheck, Effects | S2 (C, D §3) | delegated (parallel unit) | pending | |
| T6 Channels and groups, Processing | S2 (D §1-2) | delegated (parallel unit) | pending | |
| T7 Network and system | S2 (E) | delegated (parallel unit) | pending | |
| T8 Parity verification on macOS + independent verifier | S1, S7 | inline + verifier | pending | |
| T9 Remove Tauri and `apps/desktop/ui`; README; members | S6, S7 | inline | pending | |
| T10 Release workflow builds the GPUI app's installers (was `tauri-action`) | S5, S6 | inline | pending | |

Risk: item 5 (new dependencies and a workspace toolchain pin), so T8 includes one independent verifier
at the feature's end.

## Log

- L1 (2026-11-12, the user's request, verbatim): "Replace the BandEar desktop app's Tauri + web UI
  (`apps/desktop`) with a native GPUI app (GPUI = Zed's GPU-rendered Rust UI framework), keeping feature
  parity. What exists (verified 2026-11-12): `apps/desktop/src-tauri` is a Tauri 2 app. It depends on
  `personal-monitoring` = the `apps/server` library (path `../../server`). The Tauri commands in
  `apps/desktop/src-tauri/src/commands/` (for example `soundcheck.rs`) bridge the web UI to the engine;
  reuse their logic instead of rewriting it. `apps/desktop/ui` is the web frontend (`package.json`; no
  React/Svelte/Vue dependency found). Root `Cargo.toml` workspace members: `apps/server`,
  `apps/desktop/src-tauri`. cpal is vendored and patched (`vendor/cpal`). `apps/server` has default
  feature `asio` (Windows ASIO), so the desktop app must keep working on Windows. Dev machine is macOS,
  cargo 1.93. Desktop history and decisions: `apps/desktop/README.md`, `odd/tasks/desktop-dsp.md`. GPUI
  status is uncertain — verify it before writing code: which source to depend on (a web search found a
  third-party library claiming `gpui` 0.2 on crates.io, while an older GPUI README says it is only a git
  dependency on `zed-industries/zed`), Windows backend maturity, MSRV vs cargo 1.93, and what widgets
  exist (text input, sliders, lists, menus) natively or via component libraries. If GPUI cannot support
  something the current UI must do (Windows, a required control, accessibility), stop and report one
  "Needs your decision" with the evidence before rewriting anything. Steps: 1. Inventory every screen,
  control, state and command of the current UI; that inventory is the spec (feature parity). 2. Follow
  this repo's process from CLAUDE.md (ODD): this is a large task, so create `odd/tasks/desktop-gpui.md`
  plus its Engram mirror before the first source write, then work-unit commits on this worktree's
  branch. 3. Build the GPUI app, keep app-state/view-model logic testable without a window, and test it.
  4. Remove Tauri and `apps/desktop/ui` only after parity is verified; update the README and the
  workspace members. Done = the GPUI desktop app builds and runs on macOS with the same screens and
  behavior, the workspace `cargo test` passes, Tauri and the web UI are gone, docs updated, all
  committed on this branch. Do not push, open a PR or merge; that is the user's call."
- L2 (2026-11-12) GPUI verification, then the user's decision. Evidence: official `gpui` stopped at 0.2.2
  (2025-10-22), no AccessKit, builds on rustc 1.93 with `runtime_shaders`. Current GPUI ships as
  `gpui-pre` 0.3.x weekly snapshots of Zed main (third-party publisher), with AccessKit adapters for macOS
  and Windows and `gpui-component` 0.9.1 widgets (input, slider, select, switch, tabs, table, dialog);
  every gpui-pre uses `std::hint::cold_path`, so it needs rustc newer than 1.93 (builds and opens a
  window on 1.100.0; CI uses stable). Decision, relayed by the starting session: "The user picked "A:
  gpui-pre + Rust 1.100 (Recommended)" (gpui-pre =0.3.12 + gpui-component =0.9.1, Rust >= 1.100 for the
  whole workspace)."
- L3 (2026-11-12) Coordination from the shared-engine session, verbatim: "Toolchain pin: please put
  `rust-toolchain.toml` at the repo root in its own small commit, with `channel = "1.100.0"` (the exact
  version already installed) and `targets = ["aarch64-apple-ios", "aarch64-apple-ios-sim",
  "aarch64-linux-android", "armv7-linux-androideabi", "x86_64-linux-android"]` so the mobile engine keeps
  building under the pin and we don't conflict on that file later." / "Disk: the data volume has 8.9 GiB
  free (96% used) and three shared-engine writers are building in `~/projects/bandear-worktrees/`.
  Please keep one GPUI build at a time, delete probe target dirs as soon as you're done with them, and
  stop and report instead of retrying if a build fails for space." / "Downloads: ... please surface any
  further toolchain or SDK installs (for example the Metal Toolchain) to the user first; crates fetched
  by cargo are fine."
- L4 (2026-11-12) Not verified so far: AccessKit nodes on screen (the computer-use reader saw only the
  window chrome of a probe, and `osascript` lacks the accessibility permission); a Windows build (needs a
  Windows machine or CI). A GPUI debug build of the probe took about 6 GB of `target`.
- L5 (2026-11-12) `.github/workflows/release.yml` builds the desktop installers with `tauri-apps/tauri-action`
  (Windows NSIS, macOS app+dmg for both architectures, Linux AppImage+deb) and reads the version from
  `src-tauri/tauri.conf.json`; removing Tauri needs a replacement (T10). Builds in this feature run with
  `CARGO_PROFILE_DEV_DEBUG=line-tables-only` to keep `target` small (S10); RDD is on (default), so each
  work-unit commit gets `gentle-ai review assess` from the branch point `2c45064`.
- L6 (2026-11-12) The user confirmed the choice in this session, verbatim: "A: gpui-pre + Rust 1.100".
- L7 (2026-11-12) T2 landed. The first native review of the whole range stopped with
  `lens_context_budget_exceeded` (57,488 changed lines, nearly all moved files), so the T2 commit was
  split into `001ca95` (pure renames, does not build alone) and `f653d76` (the real changes, 793
  lines), and `f653d76` is reviewed alone (risk + reliability lenses; the user granted both
  consents). `gentle-ai sync` ran first, as the review's `managed_assets_outdated` stop asks. The
  Tauri crate is now `bandear-desktop-tauri` (installers would ship that binary name; moot once T9
  removes Tauri). The 1.100.0 toolchain has no rustfmt; the stable 1.93 rustfmt checks formatting.
- L8 (2026-11-12) `f653d76`'s review was approved and acknowledged (authority burned), with four
  informational advisories: the installed binary name, config resolved before the log, the hardcoded
  missing-log text, and `R3-weak-drop-on-pump-thread` (a shell that drops its last `Arc<DesktopState>`
  while the pump reads makes the pump thread drop the state and join itself; the GPUI app must keep one
  strong `Arc` for the whole process; passed to T3). The stop hook then offered the whole branch from
  `main` (57,507 lines, renames included): granted by the user, it stopped again with
  `lens_context_budget_exceeded`. The user chose, verbatim: "Seguir con RDD (Recomendado)": keep RDD on,
  review each new commit alone, and when the hook offers the whole branch again, relay it, record the
  size stop and continue.
- L9 (2026-11-13) The user, verbatim: "continue mi llave, con confianza". T3 resumed after the session
  restart and landed (console, kit, six screen stubs, app shell). Disk fell to 2.2-4.9 GiB free while
  other sessions built; `target/debug/incremental` (1.4 GB) and the Tauri crate's build output were
  removed and builds run with `CARGO_INCREMENTAL=0`. The new cargo layout keeps build output under
  `target/debug/build/<package>/<hash>/`.
- L10 (2026-11-13) Native review of T3. The range was 27,267 lines (high), so each commit was assessed
  alone in a detached worktree under `~/projects/bandear-worktrees/`; two oversized high commits were
  split with git plumbing (final tree byte-identical). The reviewer's blocker on the first split
  (`ec078b0` declared modules whose files came later) led to rebuilding the console series so every
  commit compiles and passes its own tests (107, 107, 114, 114, 160), corrected in `68a1c58` (4 lines).
  The lanes review found a real defect, fixed test-first in `cd48506` (53 lines): `Lanes::on_error`
  silently dropped its reporter when the lanes were shared or a send was in flight, so a refused
  fader value would go unreported. Every high commit was granted by the user and ended approved and
  acknowledged; the medium slices were under budget. Open advisories (WARNING, non-blocking, for T8):
  exec slot overwrite and missing exec tests, lane stuck if its flow is dropped, desk-lane eager claim
  and leak on cancel, connect with no timeout, stale engine state after loads, stale fallback notice,
  silent startup failure on Windows (`app/src/lib.rs:69-75`, `eprintln!` in a GUI-subsystem build).
- L11 (2026-11-13) Decisions for the screen units (D9): the T3 writer reported that `FakeDesk` answers
  only lifecycle commands and screen writers cannot edit the console, so `FakeDesk::script(name,
  closure)` was added (`027808f`, test-first). A new dev-only crate `bandear-desktop-preview` opens one
  screen alone over the simulated desk inside the shell's main frame (`<screen>_preview` example per
  screen crate, `71fbc17`), because building the whole app while four writers edit their screens would
  break and running it would open an input and go on the network. Assumption (T3 writer): the app's
  `BANDEAR_NO_AUTO_START` switch opens the window without starting an engine, for looks that touch no
  input or network; it is documented with T9.
- L12 Next: T4-T7 in parallel (one writer per screen crate set), then T8.
