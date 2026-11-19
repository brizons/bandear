package com.bandear.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * From the T7 verification: a lone surrogate made the core's binding throw
 * `MalformedInputException` out of reading the saved mixes and out of `connect()`.
 */
class CoreTextTest {
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
    fun savedMixesWithALoneSurrogateStillReadAndWrite() {
        val banks = listOf(MixBank("A\uD83Dna", MixSnapshot()), MixBank("Verse", MixSnapshot()))

        val read = parseBanks(serializeBanks(banks))

        assertEquals(listOf("A�na", "Verse"), read.map { it.name })
    }

    @Test
    fun aStoredDocumentHoldingALoneSurrogateKeepsEveryMix() {
        val stored = serializeBanks(listOf(MixBank("A-na", MixSnapshot()), MixBank("Verse", MixSnapshot())))
            .replace("A-na", "A\uD83Dna")

        val read = parseBanks(stored)

        assertEquals(listOf("A�na", "Verse"), read.map { it.name })
        assertTrue(read.all { it.snapshot.channelGains.isEmpty() && it.snapshot.volumePercent == 100 })
    }
}
