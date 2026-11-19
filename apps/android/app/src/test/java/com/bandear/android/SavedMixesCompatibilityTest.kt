package com.bandear.android

import com.bandear.android.core.SavedMix
import com.bandear.android.core.decodeSavedMixes
import com.bandear.android.core.encodeSavedMixes
import com.bandear.android.core.MixSnapshot as CoreMixSnapshot
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The saved mixes already on musicians' phones survive the move to the shared core.
 *
 * Each fixture in `src/test/resources/saved-mixes/` is, byte for byte, what BandEar 0.2.0 keeps under
 * its `banks` preference: the app's own `serializeBanks` (unchanged since 0.2.0, here at commit
 * 2145012) run on Android's runtime with Android's org.json, on a Galaxy S23 (Android 17), so the keys
 * are in insertion order and a `/` is written `\/`. The JVM's org.json writes differently, which is
 * why the fixtures were not made here. Every value the format has is in them: several mixes in saved
 * order, names with `/`, quotes, a backslash, accents, an emoji, CJK and control characters, a mix
 * with no channels, a full desk (32 channels, 16 groups) and the extremes of an `Int`.
 *
 * Before the app's own JSON code was deleted, these tests also proved that it read every fixture as
 * the core does and read what the core writes (commit 7218b77); on the same phone, the 0.2.0 reader
 * read the core's re-encoding of every fixture to exactly the values it read from the fixture itself.
 * So a phone that goes back to 0.2.0 still reads what this version saved.
 */
class SavedMixesCompatibilityTest {
    @Test
    fun theCoreReadsEverySavedMixAsTheAppSavedIt() {
        for ((fixture, saved) in FIXTURES) {
            assertEquals(fixture, saved, decodeSavedMixes(read(fixture)))
        }
    }

    @Test
    fun theStoreReadsEverySavedMixAsTheAppSavedIt() {
        for ((fixture, saved) in FIXTURES) {
            assertEquals(fixture, saved, parseBanks(read(fixture)))
        }
    }

    @Test
    fun theStoreWritesTheCoresTextAndReadsItBack() {
        for ((fixture, saved) in FIXTURES) {
            val banks = parseBanks(read(fixture))

            val written = serializeBanks(banks)

            assertEquals(fixture, encodeSavedMixes(saved), written)
            assertEquals(fixture, saved, parseBanks(written))
        }
    }

    private fun read(fixture: String): String =
        checkNotNull(javaClass.getResource("/saved-mixes/$fixture")) { "missing fixture $fixture" }.readText()

    private companion object {
        /** A saved mix as the app saved it, in the app's own `Int`s. */
        fun mix(
            name: String,
            gains: List<Int>,
            pans: List<Int>,
            mutes: List<Boolean>,
            solos: List<Boolean>,
            groups: List<Int>,
            volume: Int,
            max: Int,
            muted: Boolean,
        ) = SavedMix(
            name,
            CoreMixSnapshot(
                channelGains = gains.map(Int::toLong),
                channelPans = pans.map(Int::toLong),
                channelMutes = mutes,
                channelSolos = solos,
                groupLevels = groups.map(Int::toLong),
                volumePercent = volume.toLong(),
                maxLevelPercent = max.toLong(),
                muted = muted,
            ),
        )

        val NAMES_TO_ESCAPE = listOf(
            "Verse/Chorus",
            "Show \"A\"",
            "Back\\slash",
            "Güemes – ñandú 🎸",
            "日本語のミックス",
            "Tab\tand\nnewline",
            "</script> & 'apostrophes'",
            "Bell\u0007 and \u001f",
        )

        /** Each fixture and the mixes the app saved into it, in saved order. */
        val FIXTURES: Map<String, List<SavedMix>> = mapOf(
            "no-saved-mixes.json" to emptyList(),
            "one-mix-every-field.json" to listOf(
                mix(
                    "Verse",
                    gains = listOf(0, 55, 100),
                    pans = listOf(0, 50, 100),
                    mutes = listOf(true, false, true),
                    solos = listOf(false, true, false),
                    groups = listOf(30, 70),
                    volume = 85,
                    max = 90,
                    muted = true,
                ),
            ),
            "mix-with-no-channels.json" to listOf(
                mix("Empty", emptyList(), emptyList(), emptyList(), emptyList(), emptyList(), 100, 100, false),
            ),
            "names-to-escape.json" to NAMES_TO_ESCAPE.mapIndexed { index, name ->
                mix(name, listOf(10 * index), listOf(50), listOf(index % 2 == 0), listOf(false), emptyList(), 100 - index, 100, false)
            },
            "full-desk-and-extremes.json" to listOf(
                mix(
                    "Full desk",
                    gains = List(32) { (it * 7) % 101 },
                    pans = List(32) { if (it % 2 == 0) 0 else 100 },
                    mutes = List(32) { it % 5 == 0 },
                    solos = List(32) { it % 7 == 3 },
                    groups = List(16) { 100 - it * 3 },
                    volume = 64,
                    max = 77,
                    muted = false,
                ),
                mix(
                    "Extremes",
                    gains = listOf(Int.MIN_VALUE, -1, 0, Int.MAX_VALUE),
                    pans = listOf(Int.MAX_VALUE, Int.MIN_VALUE, 0, 100),
                    mutes = listOf(true, true, false, false),
                    solos = listOf(false, false, true, true),
                    groups = listOf(Int.MIN_VALUE, Int.MAX_VALUE),
                    volume = Int.MAX_VALUE,
                    max = Int.MIN_VALUE,
                    muted = true,
                ),
            ),
            "a-phones-saved-mixes.json" to listOf(
                mix("Soundcheck", listOf(100, 100, 100, 100), listOf(0, 100, 0, 100), List(4) { false }, List(4) { false }, listOf(100), 100, 100, false),
                mix("Verse", listOf(80, 40, 60, 25), listOf(20, 80, 50, 50), listOf(false, false, true, false), List(4) { false }, listOf(90), 90, 95, false),
                mix(
                    "Chorus",
                    gains = listOf(95, 55, 70, 30, 10, 0, 45, 60),
                    pans = listOf(0, 100, 0, 100, 50, 50, 25, 75),
                    mutes = List(8) { false },
                    solos = List(8) { it == 1 },
                    groups = listOf(100, 60),
                    volume = 100,
                    max = 100,
                    muted = false,
                ),
                mix("Bridge", listOf(50, 50), listOf(50, 50), listOf(false, true), List(2) { false }, emptyList(), 70, 100, true),
                mix("Solo – Ana", listOf(100, 20, 20, 20), listOf(50, 0, 100, 50), List(4) { false }, List(4) { it == 0 }, listOf(40, 40, 40), 100, 80, false),
                mix("Monitor B/2", listOf(33, 66, 99), listOf(10, 20, 30), List(3) { false }, List(3) { false }, listOf(5), 1, 2, false),
                mix("Quiet", listOf(5, 5, 5, 5), listOf(0, 100, 0, 100), List(4) { false }, List(4) { false }, listOf(10), 20, 30, true),
            ),
        )
    }
}
