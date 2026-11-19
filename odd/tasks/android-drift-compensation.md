# Android clock-drift compensation

Objective: keep the Android client's heard latency from growing over a long session when the desk's
audio clock runs faster than the phone's DAC. Branch `feat/android-drift-compensation` from main
aa12e08. Delivery: single PR. Tests: JVM unit tests (`gradle -p apps/android :app:testDebugUnitTest`).

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
  still trimmed slowly as above.
- S4 — The existing behaviour stays: cushion growth on underruns and shrink after clean seconds
  (`PlaybackLatency.kt`), `ReorderHold` reordering and gap handling, duplicate/late drops, the
  256-packets-behind restart, mute/volume processing, and the wire protocol; and, as main did, a
  short or failed `AudioTrack` write counts what the track did not take as discarded (L6).
- S5 — The `backlog_ms` the phone already reports to the desk must include the queue that S3
  measures, so the desk sees the real latency. The stats message shape does not change.
- S6 — If the phone's clock is the faster one, nothing new is needed: underruns already grow the
  cushion (S4).

## Tasks

- T1 — S1-S6: drift compensation in the Android playback path, pure policy with unit tests, honest
  backlog. Route: delegated writer (reason: long-session context backstop in the parent). Commit: 1111baf
- T2 — S3 (catch-up after a stall), S4 (write accounting), S5: on the shared engine
  (`feat/shared-engine-rebase`), in `DriftTrimmedOutput`. Route: delegated writer. Commit: 1cc1842
- T3 — S3 rework (L8): catch up only a surplus that stood through a 0.5 s window, on
  `feat/shared-engine-stack`. Route: delegated writer. Commit: 51bb2c6
- T4 — S3 margin (L9): a cut keeps max(one packet, cushion) at the window's tightest point.
  Route: delegated writer. Commit: f270897

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
- Pending: device test on the S23 with the X32 for about 20 minutes, watching that latency stays flat
  with no cuts in steady state, no underruns clustered after a cut, a startup backlog gone within
  about a second, and a Wi-Fi stall's catch-up heard as one short cut.
