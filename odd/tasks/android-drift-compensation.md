# Android clock-drift compensation

Objective: keep the Android client's heard latency from growing over a long session when the desk's
audio clock runs faster than the phone's DAC. Branch `feat/android-drift-compensation` from main
aa12e08; continued (S7, S8, T5-T7) on `feat/android-wifi-lock` from main 6936584. Delivery: single
PR. Tests: JVM unit tests (`gradle -p apps/android :app:testDebugUnitTest`).

## Specs

- S1 — Field report (L1): "al principio se escuchaba muy bien y sincronizado con poca latencia pero
  al pasar el tiempo la latencia fue aumentando". Android phone, desk on the Windows PC, capture from
  the Behringer X32's X-USB card. After a long session the heard latency must stay near the cushion
  target instead of growing without bound.
- S2 — Root cause (code evidence, 2026-10-26): the desk sends one packet per captured block, so at
  the X32's clock (`apps/server/src/pipeline.rs:243-270, 449-476`). The phone writes to `AudioTrack`
  with `WRITE_BLOCKING` (`PmonPlayer.kt:274`) into a track sized to the cushion (`:462-471`), so
  when the desk is faster the surplus waits unseen in the kernel UDP socket buffer. Nothing in the
  client drops, resamples or catches up (no drift handling anywhere in `apps/android`).
- S3 — The client must measure the whole queue a packet waits in (anything queued before the
  `AudioTrack` plus the track's own backlog), and when that queue stays above the cushion target
  for a sustained period (not on a single burst), it removes the surplus a little at a time so it
  is inaudible: short splices with a crossfade, never a hard cut, never more than a small fraction
  of the audio per second. Amended 2026-11-18 (L6, reworked in L8, margin in L9): a surplus that
  stands, after a stall or since the track opened, is removed at once instead. When the smallest
  surplus over a 0.5 s window of audio played (`CATCH_UP_WINDOW_MS`), measured as the slow trim
  measures it, is above max(cushion, 30 ms) (`CATCH_UP_FLOOR_MS`), the oldest whole queued packets
  are dropped at once, at most that minimum less max(one packet, cushion), so the queue keeps
  max(one packet, cushion) at the window's tightest point, with one crossfade at the junction
  (`PcmSplice`'s 64 frames). Jitter, coarse write steps
  and reorders come back down within the window and are never cut. The dropped frames count as
  discarded and come out of the backlog (S5). A smaller surplus, such as a sustained drift, is
  still trimmed slowly as above. Amended 2026-11-20 (L13, S8): both the catch-up and the slow trim
  keep the depth of the Wi-Fi's recurring bunches standing over that margin (`BunchReserve`): the
  longest of the last 120 s (`BUNCH_MEMORY_SECONDS`) of each second's second-longest drought, a
  drought being the frames played while nothing arrived. The catch-up then cuts only a minimum
  over max(cushion, 30 ms) plus that depth, down to max(one packet, cushion) plus that depth.
  Datagrams that arrive one at a time keep no more than a jitter's packet.
- S4 — The existing behaviour stays: cushion growth on underruns and shrink after clean seconds
  (`PlaybackLatency.kt`), `ReorderHold` reordering and gap handling, duplicate/late drops, the
  256-packets-behind restart, mute/volume processing, and the wire protocol; and, as main did, a
  short or failed `AudioTrack` write counts what the track did not take as discarded (L6).
- S5 — The `backlog_ms` the phone already reports to the desk must include the queue that S3
  measures, so the desk sees the real latency. The stats message shape does not change.
- S6 — If the phone's clock is the faster one, nothing new is needed: underruns already grow the
  cushion (S4).
- S7 — Low-latency Wi-Fi lock (L11): "While a receive session runs, hold
  `WifiManager.WIFI_MODE_FULL_LOW_LATENCY` (API 29+; fall back to `WIFI_MODE_FULL_HIGH_PERF` below
  29), and release it when the session stops." "Add the `WAKE_LOCK` permission it needs." "Test
  acquire and release through the session's seams, and update the docs." "Measure on the user's
  Galaxy S23, release builds, interleaved runs, with and without the lock: underruns per minute,
  backlog, the Auto cushion, and discarded counts." "Include one run with the screen locked."
  The fallback below 29 is not built: the app's minSdk is 29 (L12).
- S8 — Retune the stall catch-up with that data (L11): "Goal: in steady state, no more underruns
  than the build without a catch-up; after a real Wi-Fi stall, latency back near the cushion within
  about 1–2 s." On the S23's Wi-Fi, near the cushion is the cushion plus the depth of its recurring
  bunches, about 100 ms (L12, L13).

## Tasks

- T1 — S1-S6: drift compensation in the Android playback path, pure policy with unit tests, honest
  backlog. Route: delegated writer (reason: long-session context backstop in the parent). Commit: 1111baf
- T2 — S3 (catch-up after a stall), S4 (write accounting), S5: on the shared engine
  (`feat/shared-engine-rebase`), in `DriftTrimmedOutput`. Route: delegated writer. Commit: 1cc1842
- T3 — S3 rework (L8): catch up only a surplus that stood through a 0.5 s window, on
  `feat/shared-engine-stack`. Route: delegated writer. Commit: 51bb2c6
- T4 — S3 margin (L9): a cut keeps max(one packet, cushion) at the window's tightest point.
  Route: delegated writer. Commit: f270897
- T5 — S7: hold the low-latency Wi-Fi lock per receive session, through a seam of `ReceiveSession`
  (`LowLatencyWifiLock` in the app); measure it on the S23. Route: inline (no writer trigger: one
  understood change). Commit: a086d9e
- T6 — S3: port the catch-up (squashed e978296, based on a60ed34) onto main. Route: inline
  (cherry-pick, no conflicts). Commit: 91a127e
- T7 — S3, S8: the bunch reserve (`BunchReserve`) for the catch-up and the slow trim; measure it on
  the S23. Route: inline. Commit: pending

## Log

- L1 (2026-10-26, user): "ayer hice una prueba en la iglesia con un amigo, me dijo que al principio
  se escuchaba muy bien y sincronizado con poca latencia pero al pasar el tiempo la latencia fue
  aumentando, esto puede deberse a que no hay perdida de paquetes?" Then: "Android y el servidor
  estaba en este computador", "estaba usando x usb que viene en la mixer x32 de behringer", and
  "si" to starting with Android.
- L2: read-only mapping confirmed no drift compensation on either client; iOS accumulates in its
  500 ms ring (separate follow-up), the desk UI shows only `cushionMs`, never `backlogMs`
  (follow-up).
- L3 (T1 evidence): RED first: `DriftTrimTest` + the queued-frames backlog test failed 13 of 42 against
  stubs; GREEN: full `:app:testDebugUnitTest` 570 tests, 0 failures. Design: the receive loop drains
  every waiting datagram through a non-blocking `DatagramChannel` into `PlayoutQueue` (measurable,
  bounded at 24 000 frames), `DriftTrimmer` (pure; 2 s persistence window on the minimum surplus, 8 frames
  per splice, 500 ppm budget, never below target) decides, `PcmSplice` removes the frames with a 64-frame
  crossfade. `backlog_ms` now adds the queued frames (`reportExtras(queuedFrames)`). Not verified on a device.
- L4 (verification): Risk item 4 (concurrency: reworked receive/write loop). Independent verifier:
  S2-S6 met on reading, S1 needs a device, no blockers; advisories: drained packets skew jitter,
  trimmed frames are not counted as discarded, queue overflow drops are not counted as lost.
- L5 (native review review-0e848b63f58b4f8e, lenses resilience + reliability, user granted): one
  CRITICAL finding, the write loop refilled itself while a surplus lasted and starved the receive
  loop's link report, hold re-read and stop check. Bounded correction `faf72e7` (plan 60 lines):
  `reportIfDue()` shared by both loops, the write loop re-reads the hold and reports per packet and
  loops only `while (running.get())`. Full `:app:testDebugUnitTest`: 570 tests, 0 failures. Targeted
  validation approved; acknowledged.
- L6 (2026-11-18, user): asked what Android should do after a stall, chose "Volver rápido
  (Recommended)": "Si la cola supera bastante el colchón (por ejemplo, el doble), se descartan de
  una vez los paquetes más viejos hasta el objetivo: un corte breve y la latencia vuelve enseguida.
  Para deriva chica se mantiene el recorte lento e inaudible. Lleva tests y queda registrado en la
  spec de deriva." Why: on the shared engine every datagram of a stall's burst plays, so the
  stall's delay stayed and the slow trim gave it back at about 0.5 ms a second (independent
  verifier: a 200 ms stall left about 175 ms of extra latency for minutes, a 700 ms stall filled
  the queue at about 505 ms); `main` lost most of such a burst instead (a defect there), which
  brought latency back at once.
- L7 (T2 evidence): rule as implemented in `DriftTrimmedOutput`: catch up when the queue measured
  behind the next packet exceeds cushion + max(cushion, 30 ms); drop the oldest whole packets to
  the cushion, the first one kept aside so its first frames fade into the first packet kept
  (`PcmSplice`), one splice per burst. Floor: field jitter is about 1 ms (1.1 ms on the S23), a
  one-packet Auto cushion is 2.7 ms, below 30 ms the slow trim gives it back within a minute, and
  it is about what `main` played of a burst (six 5 ms datagrams). Also restored: `AudioTrack.write`'s
  answer (short count or error code) counts what was not taken as discarded. RED first (stubs with
  the new signatures): 8 of the 9 new tests failed; with a fixed 25 ms buffer, the backlog reported
  after a 200 ms stall was 200, 195, 195 ms (25 ms before it), after a 700 ms stall 495, 495, 490 ms,
  and the queue reached 24 000 frames. GREEN: both stalls report 25.0 ms from the first report after
  them, the 35 (175 ms) and 135 (675 ms) oldest packets counted as discarded, the queue never above
  2 880 frames; a 50 ms stall (11 datagrams at once) plays every packet, only the slow trim acts.
  The queue-bound test now runs at the desk's 500 ms buffer, where the catch-up cannot act. Full
  `:app:testDebugUnitTest assembleDebug`: 444 tests, 0 failures; `assembleRelease` passes. Not
  verified on a device.
- L8 (2026-11-18, device): the instant catch-up cut audio continuously on the Galaxy S23 (release
  builds, Auto cushion, no stall, 60 s steady). Parent 82bd252 (no catch-up): 2 underruns in 60 s,
  no discards, backlog about 190 ms (it started about 250 ms after connecting and drained at
  0.5 ms/s), cushion 5.33 ms. Catch-up 9fbbef4: 320 underruns in 60 s (about 5 a second), 8.6% of
  the audio discarded (1,007 whole 256-frame packets), backlog about 40 ms (often exactly 26.33),
  cushion 21.33 ms. Why the instant trigger starved the track: the blocking `AudioTrack.write`
  returns in coarse steps, so the datagrams that pile up in the socket meanwhile land in the queue
  together; `framesWritten - presentedFrame` includes the HAL and DAC pipeline and a stale
  timestamp, so the audio on the track was almost always capped at the cushion and the queue
  behind the next packet alone regularly exceeded max(cushion, 30 ms); each cut dropped the queue
  to one packet, the track starved and ran dry, Auto grew the cushion, and it repeated. Separately,
  the parent carried a standing ~250 ms backlog from connecting (audio piles up while the track
  opens) and gave it back only at 0.5 ms/s. Rework (T3): the trigger is the smallest surplus over
  each 0.5 s window of audio played (`CatchUpWindow`); at the window's end, over max(cushion,
  30 ms), at most that minimum less one packet goes at once; the instant trigger on write and the
  junction head kept across calls are gone. RED against the instant code (13 failures): a
  simulated track whose writes return only every 40 ms (it plays continuously; 4 to 8 packets pile
  up before it each time, arrivals 1 to 3 ms late) lost 3,072 samples to catch-ups in steady state
  (its 20 ms variant piles about 4 packets and passed); the queue-bound case (510 ms at once, Auto)
  played 3 packets instead of 100. The simulation reproduces the steady cuts, not the underruns:
  it plays frame by frame, while the device's mixer pulls in bursts. GREEN: 60 s at 20 and 40 ms
  steps, 0 frames cut, no underrun, every packet whole; with a fixed 25 ms buffer, a 200 ms stall
  is cut at once 475 ms after it (34 packets, 170 ms) and reports 30, 25, 25 ms (25 before); a
  700 ms stall, after the bound took 41 packets, has 93 more cut 470 ms after it and reports 30,
  30, 25 ms; a 250 ms startup backlog is cut at 720 ms (44 packets) and reports 30, then 25 ms; a
  50 ms stall plays every packet and only the slow trim acts; under 40 ms steps both stalls are cut
  and nothing runs dry after them, the backlog within two packets of before (the packet left
  standing, and room the track had before the stall that the capped measure cannot see). A window
  that straddles the stall's end still holds its tightest pre-stall moment, so the cut can come up
  to about 1 s after the stall. The queue-bound test is back to its original form; `DriftTrimTest`
  unchanged. Full `:app:testDebugUnitTest assembleDebug`: 452 tests, 0 failures; `assembleRelease`
  passes. Not verified on a device.
- L9 (2026-11-18, device): the windowed catch-up (9b38e19) still ran the track dry too often. S23,
  release builds, Auto cushion, 60 s steady, four interleaved runs: baseline 82bd252, 0 cuts a
  minute, 1 and 9 underruns a minute, backlog mean 236 and 163 ms (left over from connecting,
  draining slowly), cushion 5.33 ms; windowed 9b38e19, 23 packets cut a minute (2 to 3 cuts), 34
  and 24 underruns a minute, mostly 2 at a time in the report where the backlog drops, backlog mean
  47 and 56 ms, cushion moving between 5.33 and 21.33 ms. After 200 and 700 ms stalls the backlog
  was back to about 45 to 52 ms in 1.1 to 1.6 s, and the ~250 ms startup backlog was gone within
  about 2 s. audio_flinger: the track is a FAST track on a 4 ms FastMixer (HAL 192 frames) with its
  buffer cut to the cushion (256 or 512 frames), so blocking writes return in small steps and the
  pile-ups come from arrival or scheduling bunching, not from coarse 20 to 40 ms writes. Why: a cut
  left one packet at the window's tightest point; with a 256 to 512-frame track buffer, the next
  arrival dip rarer than the 0.5 s window emptied the track; Auto grew the cushion, and the next cut
  again left one packet. The user's choice said "hasta el objetivo": down to the cushion, not below
  it. Change (T4): a cut gives up at most the window's minimum less max(one packet, cushion), so the
  queue keeps the margin Auto sized for the measured jitter, and more as Auto grows the cushion.
  Choosing the test: in a model of the session (FAST track, 256-frame buffer, 4 ms steps, 24 ms
  HAL and DAC lag, 1 to 3 ms jitter, a late bunch every 1 to 3 s after a 250 ms stall's cut),
  leaving one packet ran the track dry from 17 ms bunches on whatever the cushion, and leaving the
  cushion from about 17 ms plus the cushion less a packet on (about 21, 27 and 33 ms for 512, 768
  and 1 024-frame cushions); so the test uses a 21.33 ms cushion and 21 ms bunches, within it.
  RED against the one-packet margin (10 failures): the new test ran dry at 3 756 ms, the end of the
  first bunch; the counts and backlogs the other tests now expect failed. GREEN: four seeds, 40
  packets cut each, never dry after the cut (only during the stall, and twice at the start, where a
  256-frame track starts with one packet ahead); with a fixed 25 ms buffer the backlog after a cut
  reads 50 then 45 ms (the buffer and the cushion left standing, given back slowly by the slow
  trim) instead of 30 then 25 ms; a 200 ms stall now cuts 30 packets, a 700 ms stall 41 at the
  bound and 89, a 250 ms startup backlog 40. Full `:app:testDebugUnitTest assembleDebug`: 453 tests,
  0 failures; `assembleRelease` passes. Not verified on a device.
- L10 (2026-11-18, device and user): third S23 run, the cushion-margin build (18977c6) against the
  baseline 82bd252 (release builds, Auto cushion, 60 s steady, four interleaved runs, then nine
  stalls): margin 23 and 18 packets cut a minute, 31 and 19 underruns a minute, backlog mean 61 and
  52 ms; baseline 0 cuts, 2 and 12 underruns a minute, backlog mean 260 and 94 ms. After 200 ms
  stalls the backlog was back in 1.2 to 1.6 s; two of three 700 ms stalls held 130 to 150 ms for 2
  to 5 s. The baseline's underruns rose as its backlog fell (2 a minute at 260 ms, 12 at 94 ms), and
  the backlog jumped from 33 to 134 ms with no gap at the sender: the phone's Wi-Fi delivers late
  bunches of about 100 ms every few seconds, which only a large backlog absorbs. The app holds no
  Wi-Fi lock. The user chose, verbatim: "Separate it (Recommended)" (the shared-engine chain ships
  at parity with `main`, without the catch-up; the catch-up moves to its own feature, which starts
  by testing the low-latency Wi-Fi lock on the S23 and then retunes the catch-up with that data).
  The write accounting of L7 stayed in the shared engine, as its own commit there. The catch-up is
  squashed onto the engine chain's tip as `feat/android-stall-catch-up`; the commits T2 to T4 cite,
  and the independent recheck of T2 (six probes, no blockers; its output probes became
  `CatchUpAccountingTest`), are on `feat/android-stall-catch-up-history`. Next: hold
  `WifiManager.WIFI_MODE_FULL_LOW_LATENCY` while receiving, measure the S23 steady state with and
  without it, then retune the catch-up with that data.
- L11 (2026-11-20, task from the session that started this one, after the user's "Separate it
  (Recommended)" and their choice to start with the Wi-Fi lock test): "Finish the two things left
  open on the Android receive path". "1. Low-latency Wi-Fi lock. The Android app holds no
  `WifiLock`." (S7). "2. Retune the stall catch-up with that data. The catch-up lives only on the
  local branches `feat/android-stall-catch-up` [...] so first rebase or port it onto current main."
  "Goal: in steady state, no more underruns than the build without a catch-up; after a real Wi-Fi
  stall, latency back near the cushion within about 1–2 s." "If the Wi-Fi lock removes the late
  bunches, the windowed catch-up may already meet this." "Done when: tests pass, the S23 numbers are
  recorded in the spec, and a PR is open for review."
- L12 (T5 evidence): `ReceiveSession` takes the lock (`holdWifi`) once the core's state is made and
  gives it back on close, after the output and the core, even when the output fails to close; a lock
  the phone refuses leaves the session playing. `LowLatencyWifiLock` makes one non-reference-counted
  `WIFI_MODE_FULL_LOW_LATENCY` lock per session; `WAKE_LOCK` in the manifest. No
  `WIFI_MODE_FULL_HIGH_PERF` fallback: minSdk is 29, and Android's reference says the older mode is
  deprecated in API 34 and asks for the low-latency one there. RED first: 2 of the 3 new session
  tests failed against a parameter the session ignored (the refusal test passed against it); GREEN:
  full `:app:testDebugUnitTest` 440 tests, 0 failures. `assembleRelease` builds with
  `-x lintVitalAnalyzeRelease`: lint fails on this machine's only JDK (26.0.2.1), a version it cannot
  read. Device (Galaxy S23, Android 17, home router at 11ac, RSSI about -53; the desk a demo engine on
  the wired Mac; release builds, arm64, Auto cushion, 10 s warm-up then 120 s,
  interleaved): `dumpsys wifi` showed `WifiLock{BandEar:receive type=4 ...}`, "Current operation
  mode: 4" and `isFg=true` through every lock run, mode 0 without it. Without the lock: 4.0, 3.0 and
  2.5 underruns a minute, backlog mean 132, 114 and 98 ms; with it: 14.4, 4.5 and 2.5 a minute, 152,
  105 and 131 ms; a second set: 2.0 and 4.0 (lock), and 9.5 and 4.5 (lock) beside the reserve runs
  of L13. Discarded 0 in every run; the Auto cushion 5.33 ms in 80 to 95% of the reports, up to
  21.33 ms after underruns. So the lock changes nothing measurable here. Why, from a diagnostics
  build that logged to logcat once a second (scratch, never committed): for 10 to 15 s every 30 to 60
  s the phone received the datagrams in bunches of 18 to 20 about every 100 ms (one beacon interval;
  the largest drain a second went from 2 or 3 datagrams to 18 to 20), and the queue before the track
  swung between 0 to 20 ms and about 117 ms at each; the backlog the desk reads is taken just before
  a bunch is drained, so these stretches read as a drop of about 95 ms and their end as a jump, the
  "late bunches" of L10, with the end-to-end latency unchanged. Not the sender: the desk's loopback
  copy (127.0.0.1:50000) arrived evenly in those seconds (largest gap a second, median 9.5 ms, as in
  the others); not the path as a whole: the control channel's round trip stayed about 9 ms in both.
  Underruns cluster at those stretches' gaps, more as the slow trim brings the standing backlog
  down: a queue under about 100 ms runs dry at a gap.
- L13 (T6, T7 evidence): the squashed catch-up (e978296) cherry-picked onto the lock with no
  conflict (91a127e). The windowed catch-up with the lock (S23, as L12, interleaved with the lock
  alone): steady 31.9, 28.9, 31.3 and 28.0 underruns a minute (lock alone: 2.0, 4.0, 9.5 and 4.5),
  backlog mean 52 to 57 ms, the cushion 10.67 to 21.33 ms in two thirds of the reports, 40 to 79
  packets cut in 2 minutes; with three 300 ms stalls (`SIGSTOP` of the desk) it was back within 1 to
  2 s each time (27.7 a minute), while the lock alone kept each stall's 75 to 100 ms for good (117
  ms before the first, 349 ms after the third). Why it underruns: each regular stretch's cut leaves
  max(one packet, cushion), and the next bunched stretch's first gaps run it dry. Change (T7, S8):
  `BunchReserve` measures each drought (packets played while nothing arrived), keeps each second's
  second-longest (a single drought, a stall's, is no pattern) and answers the longest of the last
  120 s; the catch-up leaves it standing over its margin, and the slow trim targets the cushion plus
  it. A first version measured bursts (the frames arriving between two plays) instead: steady on the
  S23 it gave 2.0 and 2.5 underruns a minute, no discards, backlog mean 102 and 116 ms, and a 300 ms
  stall came back to 134 ms within a second; but a desk stall's datagrams come back in two or more
  drains, so its second-largest burst read as a pattern, and two 700 ms stalls left the backlog at
  about 240 ms for the rest of the run. A drought ends once the queue is empty, so a stall's is never
  longer than what stood. RED first: the six new tests failed against a reserve that kept nothing
  (the session model with two bunched stretches and a stall ran dry at 30 016 ms, the second
  stretch's start); then the two stall-in-two-drains tests failed against the burst version. GREEN:
  full `:app:testDebugUnitTest` 465 tests, 0 failures (1 skipped: the opt-in benchmark). One
  expectation changed: a track whose writes return only every 40 ms lets its 7 to 8 packets play
  back to back with nothing arriving, so the reserve keeps about a step there (37 ms), latency that
  track does not need; the cut and nothing running dry after it are unchanged.
- Pending: device test on the S23 with the X32 for about 20 minutes, watching that latency stays flat
  with no cuts in steady state, no underruns clustered after a cut, a startup backlog gone within
  about a second, and a Wi-Fi stall's catch-up heard as one short cut.
