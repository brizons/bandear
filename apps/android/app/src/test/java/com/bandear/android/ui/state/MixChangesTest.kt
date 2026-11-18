package com.bandear.android.ui.state

import com.bandear.android.core.DeskConfiguration
import com.bandear.android.core.MixBankChange
import com.bandear.android.core.MixBankOutcome
import com.bandear.android.core.MixCommand
import com.bandear.android.core.MixEvent
import com.bandear.android.core.MixModel
import com.bandear.android.core.MixModelState
import com.bandear.android.core.MixReport
import com.bandear.android.core.MixSnapshot
import com.bandear.android.core.MixUpdate
import com.bandear.android.core.RecallOutcome
import com.bandear.android.core.SavedMix
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Collections
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.RejectedExecutionException
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

    // One thread: what each call asks of the phone, in order, and what happens when that fails.

    /** What [MixChanges] asked of the phone, and whether the hold was taken while it ran. */
    private data class Effect(val name: String, val value: Any?, val held: Boolean)

    /** The threads [othersWait] started; each finishes once the hold is free. */
    private val askers = mutableListOf<Thread>()

    @After
    fun askersFinish() {
        askers.forEach(Thread::join)
    }

    /**
     * Whether another thread asking [changes] for its state has to wait now, that is whether the
     * hold is taken: the asker either blocks on it or finishes.
     */
    private fun othersWait(changes: MixChanges): Boolean {
        val asker = thread { changes.state() }
        askers += asker
        val deadline = System.nanoTime() + 10_000_000_000L
        while (true) {
            when (asker.state) {
                Thread.State.BLOCKED -> return true
                Thread.State.TERMINATED -> return false
                else -> check(System.nanoTime() < deadline) { "the asker neither waited nor finished" }
            }
            Thread.yield()
        }
    }

    /** Changes on [model] that record every state shown and mix sent into [effects]. */
    private fun recording(model: MixModel, effects: MutableList<Effect>): MixChanges {
        lateinit var changes: MixChanges
        changes = MixChanges(
            model,
            show = { state -> effects += Effect("show", state, othersWait(changes)) },
            send = { command -> effects += Effect("send", command, othersWait(changes)) },
        )
        return changes
    }

    /** A model in a session with a desk of two channels. */
    private fun connected(): MixModel = MixModel(null, emptyList()).also { model ->
        model.apply(MixEvent.SessionStarted)
        model.apply(config(2))
    }

    private val verse = SavedMix(
        name = "Verse",
        snapshot = MixSnapshot(
            channelGains = listOf(30L, 40L),
            channelPans = listOf(0L, 100L),
            channelMutes = listOf(false, false),
            channelSolos = listOf(false, false),
            groupLevels = emptyList(),
            volumePercent = 80L,
            maxLevelPercent = 100L,
            muted = false,
        ),
    )

    @Test
    fun aRecallReadsThePhonesMixesFirstThenShowsAndSendsUnderTheHold() {
        connected().use { model ->
            val effects = mutableListOf<Effect>()
            val changes = recording(model, effects)

            // The model was made with no saved mix: it only knows "Verse" from the list handed over.
            val recall = changes.recall("Verse", listOf(verse)) { recall ->
                effects += Effect("alongside", recall.outcome, othersWait(changes))
            }

            assertEquals(RecallOutcome.Recalled(name = "Verse"), recall.outcome)
            assertEquals(listOf(verse), model.savedMixes())
            val command = checkNotNull(recall.update.command) { "the recall numbered a mix" }
            assertEquals(listOf(30L, 40L), recall.update.state.strips.map { it.level })
            assertEquals(
                listOf(
                    Effect("show", recall.update.state, held = true),
                    Effect("send", command, held = true),
                    Effect("alongside", recall.outcome, held = true),
                ),
                effects,
            )
        }
    }

    @Test
    fun aSavedMixChangeStoresTheListOnlyWhenItChangedThenShows() {
        connected().use { model ->
            val effects = mutableListOf<Effect>()
            val changes = recording(model, effects)
            val store: (List<SavedMix>) -> Unit = { mixes -> effects += Effect("store", mixes, othersWait(changes)) }
            val alongside: (MixBankChange) -> Unit = { change ->
                effects += Effect("alongside", change.outcome, othersWait(changes))
            }

            val saved = changes.changeSavedMixes({ it.saveMix("Verse") }, store, alongside)

            assertEquals(MixBankOutcome.Saved(name = "Verse"), saved.outcome)
            assertTrue(saved.savedMixesChanged)
            assertEquals(
                listOf(
                    Effect("store", model.savedMixes(), held = true),
                    Effect("show", saved.state, held = true),
                    Effect("alongside", saved.outcome, held = true),
                ),
                effects,
            )

            effects.clear()
            val refused = changes.changeSavedMixes({ it.saveMix("  ") }, store, alongside)

            assertEquals(MixBankOutcome.NameNeeded, refused.outcome)
            assertFalse(refused.savedMixesChanged)
            assertEquals(
                listOf(
                    Effect("show", refused.state, held = true),
                    Effect("alongside", refused.outcome, held = true),
                ),
                effects,
            )
        }
    }

    @Test
    fun aChangeWhoseShowingFailsStillSendsTheMixItNumbered() {
        connected().use { model ->
            val sent = mutableListOf<MixCommand>()
            var screenGone = false
            val changes = MixChanges(
                model,
                show = { check(!screenGone) { "the screen is gone" } },
                send = { sent += it },
            )

            screenGone = true
            val failure = assertThrows(IllegalStateException::class.java) {
                changes.apply(MixEvent.ChannelLevelChanged(channel = 0L, level = 30L))
            }

            assertEquals("the screen is gone", failure.message)
            val numbered = model.state()
            assertEquals(listOf(numbered.sync.latest), sent.map { it.sequence })
            assertEquals(numbered.strips.map { it.level }, sent.single().channelGains)

            // The next change goes out after it, under the next number.
            screenGone = false
            val next = changes.apply(MixEvent.ChannelLevelChanged(channel = 0L, level = 40L))
            assertEquals(listOf(numbered.sync.latest, next.state.sync.latest), sent.map { it.sequence })
        }
    }

    @Test
    fun aMixTheClosedChannelRejectsIsNothingToSend() {
        connected().use { model ->
            val shown = mutableListOf<MixModelState>()
            val changes = MixChanges(
                model,
                show = { shown += it },
                send = { throw RejectedExecutionException("the control channel is closed") },
            )
            var alongside: MixUpdate? = null

            val update = changes.apply(MixEvent.ChannelLevelChanged(channel = 0L, level = 30L)) { alongside = it }
            changes.sendOpeningMix()

            assertNotNull("the change numbered a mix", update.command)
            assertEquals(listOf(update.state), shown)
            assertEquals(update, alongside)
            assertEquals(update.state, model.state())
        }
    }

    @Test
    fun aSavedMixTheStoreFailsToKeepIsStillShownAndTheFailureGoesOn() {
        connected().use { model ->
            val shown = mutableListOf<MixModelState>()
            val changes = MixChanges(model, show = { shown += it }, send = {})
            var said = false

            val failure = assertThrows(IllegalStateException::class.java) {
                changes.changeSavedMixes(
                    change = { it.saveMix("Verse") },
                    store = { throw IllegalStateException("the disk is full") },
                    alongside = { said = true },
                )
            }

            assertEquals("the disk is full", failure.message)
            // The screen shows what the model holds, and nothing says the mix was saved.
            assertEquals(listOf(model.state()), shown)
            assertEquals(listOf("Verse"), shown.single().savedMixNames)
            assertFalse(said)
        }
    }

    private companion object {
        const val ROUNDS = 1_500
    }
}
