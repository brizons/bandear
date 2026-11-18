package com.bandear.android.ui.state

import com.bandear.android.core.DeskConfiguration
import com.bandear.android.core.MixCommand
import com.bandear.android.core.MixEvent
import com.bandear.android.core.MixModel
import com.bandear.android.core.MixModelState
import com.bandear.android.core.MixReport
import com.bandear.android.core.MixUpdate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Collections
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.CountDownLatch
import kotlin.concurrent.thread

/**
 * The order [MixChanges] keeps between the main thread (the musician) and the control thread (the
 * desk), both calling the real shared core at once: the states shown never go back, the mixes sent
 * go out in the order of their numbers, and each one is the mix its own change returned.
 */
class MixChangesTest {
    /** Every state shown, in the order it was shown. */
    private val shown: MutableList<MixModelState> = Collections.synchronizedList(mutableListOf())

    /** Every mix handed over to send, in the order it was handed over, and whether it was an opening. */
    private val sent: MutableList<Pair<MixCommand, Boolean>> = Collections.synchronizedList(mutableListOf())

    /** Set on the desk's thread while it asks for an opening mix, which goes out on that thread. */
    private val opening = ThreadLocal.withInitial { false }

    private fun config(channels: Int) = MixEvent.ConfigReceived(
        DeskConfiguration(sourceChannels = channels.toUByte(), sampleRate = null, groups = emptyList(), channels = null),
    )

    /** The desk's whole mix, pushed because it changed elsewhere: every channel at [level]. */
    private fun pushed(level: Long) = MixEvent.MixPushed(
        MixReport(
            sequence = null,
            volumePercent = 100,
            maxLevelPercent = 100,
            muted = false,
            channelGains = List(32) { level },
            pans = List(32) { if (it % 2 == 0) 0L else 100L },
            channelMutes = List(32) { false },
            channelSolos = List(32) { false },
            groupLevels = List(16) { 100L },
            engineerLimitPercent = 100,
            locked = List(32) { false },
            following = null,
        ),
    )

    /** Pauses after each effect, so the other thread gets every chance to slip in between. */
    private fun changes(model: MixModel) = MixChanges(
        model,
        show = { state ->
            shown += state
            Thread.yield()
        },
        send = { command ->
            sent += command to (opening.get() == true)
            Thread.yield()
        },
    )

    @Test
    fun changesFromBothThreadsAreShownAndSentInTheOrderTheyLanded() {
        MixModel(null, emptyList()).use { model ->
            val changes = changes(model)
            changes.apply(MixEvent.SessionStarted)
            changes.apply(config(4))
            val musicianUpdates = ConcurrentLinkedQueue<MixUpdate>()
            val deskUpdates = ConcurrentLinkedQueue<MixUpdate>()
            val start = CountDownLatch(1)

            val musician = thread {
                start.await()
                repeat(ROUNDS) { round ->
                    musicianUpdates += changes.apply(MixEvent.ChannelLevelChanged(channel = (round % 4).toLong(), level = (round % 101).toLong()))
                    musicianUpdates += changes.apply(MixEvent.MusicianChannelChosen(channel = 3))
                    musicianUpdates += changes.apply(MixEvent.MoreOfMeToggled(on = round % 3 != 0))
                }
            }
            val desk = thread {
                start.await()
                repeat(ROUNDS) { round ->
                    // Two channels drop the musician's: with More of me on, that turns it off and is sent.
                    deskUpdates += changes.apply(config(if (round % 2 == 0) 2 else 4))
                    deskUpdates += changes.apply(pushed(level = (round % 100).toLong()))
                    val newest = model.state().sync.latest
                    deskUpdates += changes.apply(MixEvent.MixAnswered(sequence = newest, report = null))
                    if (round % 10 == 0) {
                        opening.set(true)
                        changes.sendOpeningMix()
                        opening.set(false)
                    }
                }
            }
            start.countDown()
            musician.join()
            desk.join()

            assertTrue("the musician's thread sent mixes", musicianUpdates.any { it.command != null })
            assertTrue("the desk's thread sent mixes", deskUpdates.any { it.command != null })
            val numbered = (musicianUpdates + deskUpdates).mapNotNull { it.command }

            // The states shown never go back: neither the newest number nor the newest answer.
            val latest = shown.map { it.sync.latest }
            val answered = shown.map { it.sync.answered }
            assertEquals("the numbers shown never go back", latest.sorted(), latest)
            assertEquals("the answers shown never go back", answered.sorted(), answered)
            assertEquals(model.state(), shown.last())

            // The mixes go out in the order of their numbers, openings included.
            val sequences = sent.map { (command, _) -> command.sequence!! }
            assertEquals("the mixes sent never go back", sequences.sorted(), sequences)

            // Each change's mix is sent once, as its own change returned it, in the order of the numbers.
            val changesSent = sent.filterNot { (_, isOpening) -> isOpening }.map { (command, _) -> command }
            assertEquals(numbered.sortedBy { it.sequence }, changesSent)
            assertEquals(changesSent.map { it.sequence }.distinct(), changesSent.map { it.sequence })
        }
    }

    @Test
    fun eachMixSentIsTheStateItsChangeLeft() {
        MixModel(null, emptyList()).use { model ->
            val changes = changes(model)
            changes.apply(MixEvent.SessionStarted)
            changes.apply(config(4))
            val updates = ConcurrentLinkedQueue<MixUpdate>()
            val start = CountDownLatch(1)
            val threads = listOf(0L, 1L).map { channel ->
                thread {
                    start.await()
                    repeat(ROUNDS) { round ->
                        updates += changes.apply(MixEvent.ChannelLevelChanged(channel = channel, level = (round % 101).toLong()))
                        updates += changes.apply(MixEvent.MasterVolumeChanged(level = ((round + channel) % 101)))
                    }
                }
            }
            start.countDown()
            threads.forEach(Thread::join)

            for (update in updates) {
                val command = update.command ?: continue
                val state = update.state
                assertEquals(state.sync.latest, command.sequence)
                assertEquals(state.strips.map { it.level }, command.channelGains)
                assertEquals(state.masterVolume, command.volumePercent)
                assertEquals(state.levelCap, command.maxLevelPercent)
            }
            val sequences = sent.map { (command, _) -> command.sequence!! }
            assertEquals((1uL..sequences.size.toULong()).toList(), sequences)
        }
    }

    private companion object {
        const val ROUNDS = 1_500
    }
}
