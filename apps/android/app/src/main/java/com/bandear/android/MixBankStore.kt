package com.bandear.android

import android.content.Context
import android.content.SharedPreferences
import com.bandear.android.core.SavedMix
import com.bandear.android.core.decodeSavedMixes
import com.bandear.android.core.encodeSavedMixes

/**
 * Device-local persistence for the saved mixes and the designated musician channel.
 *
 * The saved mixes live under one `SharedPreferences` key as JSON, in the shared core's saved-mix
 * format ([serializeBanks]). The rules for saving, renaming and deleting them are the core's
 * (`MixModel`); this store only keeps the list the model hands it and gives it back at launch and
 * before a recall. Reading is guarded: malformed or missing data yields an empty list instead of
 * crashing, because a mix can sit on a phone for weeks and may have been written by an older version
 * of the app.
 *
 * The designated musician channel is deliberately kept beside the mixes rather than inside a saved
 * mix: it describes who the musician is, not a particular balance, so recalling a mix must never
 * overwrite it.
 */
class MixBankStore(context: Context) {
    private val preferences: SharedPreferences =
        context.getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    /** The saved mixes, in the order they were saved. */
    fun loadSavedMixes(): List<SavedMix> = parseBanks(preferences.getString(KEY_BANKS, null))

    /** Keeps [mixes] as the saved mixes, replacing the list stored before. */
    fun storeSavedMixes(mixes: List<SavedMix>) {
        preferences.edit().putString(KEY_BANKS, serializeBanks(mixes)).apply()
    }

    /** The designated musician channel, or `null` when none has been chosen. */
    fun musicianChannel(): Int? =
        preferences.getInt(KEY_MUSICIAN_CHANNEL, NO_CHANNEL).takeIf { it >= 0 }

    fun setMusicianChannel(channel: Int?) {
        preferences.edit().putInt(KEY_MUSICIAN_CHANNEL, channel ?: NO_CHANNEL).apply()
    }

    /**
     * The musician's name, remembered across app restarts exactly like the musician channel.
     * Empty until one is typed; the registration still goes out, just unnamed.
     */
    fun musicianName(): String = preferences.getString(KEY_MUSICIAN_NAME, "") ?: ""

    /** Saves the musician's name. Whitespace is trimmed; an empty name is stored as empty. */
    fun setMusicianName(name: String) {
        preferences.edit().putString(KEY_MUSICIAN_NAME, name.trim()).apply()
    }

    private companion object {
        const val PREFERENCES_NAME = "bandear_banks"
        const val KEY_BANKS = "banks"
        const val KEY_MUSICIAN_CHANNEL = "musician_channel"
        const val KEY_MUSICIAN_NAME = "musician_name"
        const val NO_CHANNEL = -1
    }
}

/**
 * The saved mixes as the store keeps them: the shared core's saved-mix JSON
 * ([com.bandear.android.core.encodeSavedMixes]), which the app's earlier versions read too.
 */
internal fun serializeBanks(mixes: List<SavedMix>): String = encodeSavedMixes(mixes)

/**
 * The saved mixes [json] holds, read by the shared core ([com.bandear.android.core.decodeSavedMixes]),
 * which reads what every version of the app wrote. Never throws: a missing, malformed or partially
 * corrupted document falls back to an empty list, skipping only the entries that cannot be read.
 */
internal fun parseBanks(json: String?): List<SavedMix> = decodeSavedMixes(json?.wellFormedForCore())
