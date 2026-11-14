package com.bandear.android

import android.content.Context
import android.content.SharedPreferences
import com.bandear.android.core.validClientId
import java.util.UUID

/**
 * A new random client id. A UUID is not guessable on the LAN and passes the shared core's
 * [validClientId] (1 to 64 ASCII letters, digits and hyphens, the rule the server applies).
 */
internal fun newClientId(): String = UUID.randomUUID().toString()

/**
 * Returns the [stored] id when the shared core finds it valid ([validClientId]). Otherwise (first
 * run, or a value damaged on the device) makes a new one with [generate], hands it to [save] and
 * returns it.
 */
internal fun resolveClientId(
    stored: String?,
    generate: () -> String = ::newClientId,
    save: (String) -> Unit,
): String {
    validClientId(stored)?.let { return it }
    val created = generate()
    save(created)
    return created
}

/**
 * This installation's client id, generated on first use and kept in its own `SharedPreferences`
 * file from then on.
 *
 * The phone announces it in every registration. The server keys musicians by IP, and a phone that
 * comes back from a new IP (DHCP after a Wi-Fi blip) would otherwise appear as a second musician;
 * with the id the server moves the musician's row, mix included, to the new address. It names this
 * installation only: it is random, carries nothing about the device or the musician, and is never
 * shown.
 */
class ClientIdentity(context: Context) {
    private val preferences: SharedPreferences =
        context.getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    val clientId: String by lazy {
        resolveClientId(preferences.getString(KEY_CLIENT_ID, null)) { created ->
            preferences.edit().putString(KEY_CLIENT_ID, created).apply()
        }
    }

    private companion object {
        const val PREFERENCES_NAME = "bandear_identity"
        const val KEY_CLIENT_ID = "client_id"
    }
}
