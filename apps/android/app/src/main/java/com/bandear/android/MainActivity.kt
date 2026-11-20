package com.bandear.android

import android.Manifest
import android.content.pm.PackageManager
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.bandear.android.ui.screens.MonitorActions
import com.bandear.android.ui.screens.MonitorApp
import com.bandear.android.ui.state.MonitorViewModel
import com.bandear.android.ui.state.ServerEndpoint
import com.bandear.android.ui.theme.BandEarTheme

/**
 * The app's only screen: it shows [MonitorApp] and binds it to the [MonitorViewModel].
 *
 * The session, the mix and everything the screens show live in the ViewModel, so turning the
 * phone keeps the audio and the control channel going; they end when the ViewModel is cleared,
 * that is when the Activity finishes for good (swiping the app away from Recents included), as the
 * old screen ended them in `onDestroy`. While a session runs, [PlaybackService] keeps it playing
 * with the screen locked; the first Connect asks for the notification permission it needs to show
 * ([NotificationPrompt]).
 */
class MainActivity : ComponentActivity() {
    private val monitor: MonitorViewModel by viewModels { MonitorViewModel.Factory }

    /** Once the permission is granted, the notification of the session already playing shows. */
    private val notificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            if (granted) PlaybackService.refresh(this)
        }

    /**
     * The screens' actions: the ViewModel's, with a Connect that may then ask for notifications. Only
     * a Connect that started a session asks: one the ViewModel rejected (an empty address) does not
     * use up the single prompt.
     */
    private val actions: MonitorActions by lazy {
        object : MonitorActions by monitor {
            override fun connect(server: ServerEndpoint) {
                monitor.connect(server)
                askForNotificationsIfPlaying()
            }

            override fun connectManual(host: String, controlPort: String, audioPort: String) {
                monitor.connectManual(host, controlPort, audioPort)
                askForNotificationsIfPlaying()
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        // The app draws behind the system bars. Its look is dark only, so the bars keep light icons
        // whatever theme the phone itself uses.
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(Color.TRANSPARENT),
        )
        super.onCreate(savedInstanceState)
        setContent {
            BandEarTheme {
                val state by monitor.state.collectAsStateWithLifecycle()
                MonitorApp(state = state, actions = actions)
            }
        }
    }

    /**
     * Asks for the notification permission when a session is playing and [NotificationPrompt] says
     * this Connect should. The session plays in the background whatever the answer.
     */
    private fun askForNotificationsIfPlaying() {
        if (!monitor.state.value.connection.sessionRunning) return
        val sdk = Build.VERSION.SDK_INT
        val granted = sdk < NotificationPrompt.FIRST_SDK ||
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
        val prompts = getSharedPreferences(PROMPTS, MODE_PRIVATE)
        if (!NotificationPrompt.shouldAsk(sdk, granted, prompts.getBoolean(NOTIFICATIONS_ASKED, false))) return
        prompts.edit().putBoolean(NOTIFICATIONS_ASKED, true).apply()
        notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
    }

    private companion object {
        /** What the app has asked the musician already. */
        const val PROMPTS = "bandear_prompts"
        const val NOTIFICATIONS_ASKED = "notifications_asked"
    }
}
