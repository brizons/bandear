# Android background playback

Objective: keep the Android app receiving and playing a monitor session while the screen is locked
or another app is in front. Branch `feat/android-background-playback`, stacked on
`feat/android-wifi-lock` (PR #126) at 47c362d. Delivery: single PR based on #126's branch. Tests:
JVM unit tests (`gradle -p apps/android :app:testDebugUnitTest`).

## Specs

- S1 — "Make the BandEar Android app keep receiving and playing a monitor session while the screen
  is locked or the app is in the background." (L1)
- S2 — "First, confirm on current main that the same happens: lock the screen during a session and
  watch `adb logcat` and the desk's log." (L1)
- S3 — A foreground service of type `mediaPlayback`, with "the `FOREGROUND_SERVICE` and
  `FOREGROUND_SERVICE_MEDIA_PLAYBACK` permissions", "an ongoing notification with a way to
  disconnect" and "`POST_NOTIFICATIONS` handling on API 33+" (L1). As chosen (L2): "La notificación
  muestra el servidor y un botón Desconectar, y tocarla abre la app. En Android 13+ se pide el
  permiso de notificaciones al primer Connect. Deslizar la app desde Recientes corta la sesión, como
  hoy." So the session stays in `MonitorViewModel` and ends with it, as today; the brief's "The
  service owns the receive session's lifetime so the activity can go away" belonged to the option
  not chosen.
- S4 — "Check whether the low-latency Wi-Fi lock should stay held while in the background; Android
  only applies its low-latency mode in the foreground with the screen on, so it then falls back to
  the normal mode." (L1)
- S5 — Done when: "a session keeps playing with the screen locked for at least 10 minutes on the
  S23"; "JVM tests cover the service's start and stop seams (`gradle -p apps/android
  :app:testDebugUnitTest`)"; "docs are updated"; "a PR is open." (L1)

## Tasks

- T1 — S2: confirm the cut on main with the screen locked (S23, `.abbase` test build of 6936584).
  Route: inline (device run). Status: done (L5).
- T2 — S1, S3, S4: `SessionKeepAlive` seam in `MonitorViewModel` (started by a session the user
  connects, stopped when it ends, the notification's Disconnect ends only its own session, a refused
  start leaves the session playing); `PlaybackService` (foreground `mediaPlayback`, notification
  with the server, Disconnect and tap to open); manifest permissions; the notification permission
  asked at the first Connect on API 33+. Route: inline (one understood change, no writer reason).
  Status: done, bfac70e (RED: 6 of the new tests failed against stubs; GREEN: 476 tests, 0 failures).
- T3 — S5: 10 locked minutes on the S23, docs, PR. Route: inline. Status: the session keeps playing
  locked (L6, L7), with dropouts while the screen is off (L7); README updated in bfac70e and after
  L8; PR: brizons/bandear#127, based on #126's branch (626364e).

## Log

- L1 (2026-11-20, the user): "Haz esta tarea aquí: “Keep Android playback alive with the screen
  locked”", the task card whose brief they started here: "Make the BandEar Android app keep receiving
  and playing a monitor session while the screen is locked or the app is in the background." / "The
  app declares no service at all in `apps/android/app/src/main/AndroidManifest.xml`; the receive
  loop runs from the activity's `MonitorViewModel` / `PmonPlayer` / `ReceiveSession`." / "First,
  confirm on current main that the same happens: lock the screen during a session and watch `adb
  logcat` and the desk's log. Then design the fix, likely a foreground service of type
  `mediaPlayback` (`FOREGROUND_SERVICE` and `FOREGROUND_SERVICE_MEDIA_PLAYBACK` permissions, an
  ongoing notification with a way to disconnect, `POST_NOTIFICATIONS` handling on API 33+). The
  service owns the receive session's lifetime so the activity can go away. Check whether the
  low-latency Wi-Fi lock should stay held while in the background; Android only applies its
  low-latency mode in the foreground with the screen on, so it then falls back to the normal mode.
  Ask the user before any product decision (the notification's content and actions, whether to stop
  on swipe-away). Done when: a session keeps playing with the screen locked for at least 10 minutes
  on the S23, JVM tests cover the service's start and stop seams (`gradle -p apps/android
  :app:testDebugUnitTest`), docs are updated, and a PR is open."
- L2 (2026-11-20, the user's answer to the product question): "Desconectar + deslizar corta
  (Recomendado)": "La notificación muestra el servidor y un botón Desconectar, y tocarla abre la
  app. En Android 13+ se pide el permiso de notificaciones al primer Connect. Deslizar la app desde
  Recientes corta la sesión, como hoy. Es el cambio más chico."
- L3 (evidence that started this, from #126's L14): on the S23 with #126's build, about 6 s after
  the screen went off the phone stopped sending its stats; at about 22 s Android logged
  "AudioHardening background playback would be muted for com.bandear.android.abreserve (10478),
  level: partial, reason: 6, usage: USAGE_MEDIA", the phone reset the control connection in the same
  second, and the desk dropped the registration after its grace period.
- L4 (design): a session only starts from the user's Connect (`MonitorViewModel.startSession`, on
  the main thread, with the app in front, where Android lets a foreground service start) and ends
  in `closeSession` (Disconnect, or the receiver ending on its own) or in `onCleared` (the Activity
  finishing for good, swiping it away included). The keep-alive follows those two points.
  `closeSession` can run on the receive thread, so the service's end is posted to the main thread
  and only ends the session it was asked for, never one started since. The service always calls
  `startForeground` when started, as Android requires after `startForegroundService`, and stops at
  once if its session already ended. The Wi-Fi lock (S4) stays held: it costs nothing while
  Android has the low-latency mode off, and the mode comes back by itself when the app is in front
  with the screen on again.
- L5 (T1 evidence, S2): main (6936584) as the `.abbase` test build on the S23, release, the desk a
  demo engine on the wired Mac. The session registered at 15:51:49 UTC; within seconds of the
  screen going off the phone's stats stopped (the last at 48 s of the desk's clock), Android logged
  "AudioHardening background playback would be muted for com.bandear.android.abbase (10473), level:
  partial, reason: 6, usage: USAGE_MEDIA" at 15:52:26, the desk's log read "control connection
  192.168.1.10:43434 error: IO error: Connection reset by peer (os error 54)" in the same second,
  and "control registration 192.168.1.10:50000 removed after the grace period" at 15:52:41. With
  the screen off, `dumpsys wifi` read "Screen state: false", "Current operation mode: 0". So main
  cuts the same way #126's build did (L3).
- L6 (T3 evidence, S3, unlocked; bfac70e as the `.abfgs` release test build on the S23, whose
  system language is Spanish). The first Connect showed Android's dialog "¿Quieres permitir que
  BandEar te envíe notificaciones?" ("Permitir" / "No permitir"); before it was answered the
  session was already playing and `PlaybackService` ran in the foreground. After "Permitir"
  (`POST_NOTIFICATIONS: granted=true`), a later Connect asked nothing; while it played, `dumpsys
  activity services` read "isForeground=true foregroundId=1 types=0x00000002" (`mediaPlayback`),
  with the ongoing notification ("flags=ONGOING_EVENT|NO_CLEAR|FOREGROUND_SERVICE ... actions=1"),
  and the shade showed "Playing your mix" and "Connected to 192.168.1.54:50001". Its Disconnect
  (shown once the notification is expanded) ended the session: the service and the notification
  were gone and the app was back on its server list. One desk-side reset at 16:49:56 in this stretch
  was the test script force-stopping the app, not the change.
- L7 (T3 evidence, S1, S4, S5: 10 minutes with the screen off, the `.abfgs` build, the desk a demo
  engine on the wired Mac): the session kept playing for the whole 10.01 minutes, with 590 stats
  reports and no gap; at the end `PlaybackService` still read "isForeground=true foregroundId=1
  types=0x00000002"; Android logged no AudioHardening line; and the desk never reset the control
  connection (one registration at 16:52:35 throughout). With the screen off, `dumpsys wifi` read
  "Screen state: false", "Current operation mode: 0", the lock still held and `isFg=false` (S4: the
  lock stays held; Android turns its low-latency mode off, as expected). The audio, though, was
  worse than with the screen on: 22.2 underruns a minute (13 to 41 in each minute, against 1.0 to
  2.5 with the screen on in #126's L14), some of them with about 140 ms standing in the queue, 96
  packets lost (79 in one minute, 17 in another), backlog mean 263 ms (up to 521 ms, the bunch
  reserve following longer droughts), the Auto cushion 9.6 ms on average. So S5's "keeps playing"
  holds, but with dropouts while the screen is off; why (the Wi-Fi's power save with its
  low-latency mode off, the CPU or the audio path with the screen off) is not known yet.
- L8 (independent verification, Risk item 4: the service's end posted from the receive thread to
  the main thread, and item 6: no JVM test drives `PlaybackService`): a read-only verifier over
  47c362d..364f7a2 found no blocker. No interleaving of start, stop, the notification's Disconnect,
  `onStartCommand` and `onDestroy` ends a newer session's service, leaves one running after its
  session, misses `startForeground`, or disconnects another session. Verdict: S1-S4 met; S5 partly,
  the PR not open yet. Advisories taken: a Connect the ViewModel rejected (an empty address) used up
  the single notification prompt, so only a Connect that started a session asks now; the README now
  states the screen-off dropouts of L7; the `startForeground` catch's comment says which start it
  covers (the notification's Disconnect, a plain start, reaching a service no longer running).
  Advisories left: the service's own ordering has no automated test (only `FakeKeepAlive` is driven);
  not recorded on the device: tapping the notification to open the app, swiping the app away with
  the service running, another app in front, and the notification showing right after a later
  grant (`PlaybackService.refresh`).
