package com.bandear.android

import com.bandear.android.core.MixSnapshot
import com.bandear.android.core.SavedMix
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * From the T7 verification: a lone surrogate made the core's binding throw
 * `MalformedInputException` out of reading the saved mixes and out of `connect()`. A name typed for
 * a saved mix crosses the same way (`MonitorViewModelTest`).
 */
class CoreTextTest {
    private val emptyMix = MixSnapshot(
        channelGains = emptyList(),
        channelPans = emptyList(),
        channelMutes = emptyList(),
        channelSolos = emptyList(),
        groupLevels = emptyList(),
        volumePercent = 100,
        maxLevelPercent = 100,
        muted = false,
    )

    @Test
    fun wellFormedTextCrossesAsItIs() {
        val text = "Ana é 🎸 漢字"
        assertSame(text, text.wellFormedForCore())
    }

    @Test
    fun aLoneSurrogateBecomesTheReplacementCharacter() {
        assertEquals("A�na", "A\uD83Dna".wellFormedForCore())
        assertEquals("A�na", "A\uDE00na".wellFormedForCore())
        assertEquals("�", "\uD83D".wellFormedForCore())
        assertEquals("🎸�", "🎸\uD83D".wellFormedForCore())
        assertEquals("��", "\uDE00\uD83D".wellFormedForCore())
    }

    @Test
    fun aStoredDocumentHoldingALoneSurrogateKeepsEveryMix() {
        val stored = serializeBanks(listOf(SavedMix("A-na", emptyMix), SavedMix("Verse", emptyMix)))
            .replace("A-na", "A\uD83Dna")

        val read = parseBanks(stored)

        assertEquals(listOf("A�na", "Verse"), read.map { it.name })
        assertTrue(read.all { it.snapshot.channelGains.isEmpty() && it.snapshot.volumePercent == 100L })
    }
}
