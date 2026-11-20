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
  Route: inline (device run). Status: pending.
- T2 — S1, S3, S4: `SessionKeepAlive` seam in `MonitorViewModel` (started by a session the user
  connects, stopped when it ends, the notification's Disconnect ends only its own session, a refused
  start leaves the session playing); `PlaybackService` (foreground `mediaPlayback`, notification
  with the server, Disconnect and tap to open); manifest permissions; the notification permission
  asked at the first Connect on API 33+. Route: inline (one understood change, no writer reason).
  Status: pending.
- T3 — S5: 10 locked minutes on the S23, docs, PR. Route: inline. Status: pending.

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
