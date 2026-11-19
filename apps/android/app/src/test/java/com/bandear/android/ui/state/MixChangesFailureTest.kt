package com.bandear.android.ui.state

import com.bandear.android.core.DeskConfiguration
import com.bandear.android.core.MixCommand
import com.bandear.android.core.MixEvent
import com.bandear.android.core.MixModel
import com.bandear.android.core.MixModelState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Collections
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread

/**
 * Regression tests from the verification of f4e07c6: only a send the closed channel rejects is
 * swallowed, a failed show still sends, and numbering and ordering survive failures, also with
 * two threads and a real executor shut down mid-run.
 */
class MixChangesFailureTest {
    private fun config(channels: Int) = MixEvent.ConfigReceived(
        DeskConfiguration(sourceChannels = channels.toUByte(), sampleRate = null, groups = emptyList(), channels = null),
    )

    private fun connected(): MixModel = MixModel(null, emptyList()).also { model ->
        model.apply(MixEvent.SessionStarted)
        model.apply(config(4))
    }

    /** P1: a send failure that is not a rejection is not swallowed. */
    @Test
    fun p1_aSendFailureOtherThanRejectionGoesOn() {
        connected().use { model ->
            val shown = mutableListOf<MixModelState>()
            val changes = MixChanges(model, show = { shown += it }, send = { error("socket broke") })
            val failure = assertThrows(IllegalStateException::class.java) {
                changes.apply(MixEvent.ChannelLevelChanged(channel = 0L, level = 30L))
            }
            assertEquals("socket broke", failure.message)
            assertEquals(1, shown.size)
            val opening = assertThrows(IllegalStateException::class.java) { changes.sendOpeningMix() }
            assertEquals("socket broke", opening.message)
        }
    }

    /** P2: show fails and the send is rejected: the show failure is the one the caller sees. */
    @Test
    fun p2_aFailedShowFollowedByARejectedSendReportsTheShowFailure() {
        connected().use { model ->
            var sendCalls = 0
            val changes = MixChanges(
                model,
                show = { error("screen gone") },
                send = { sendCalls++; throw RejectedExecutionException("closed") },
            )
            val failure = assertThrows(IllegalStateException::class.java) {
                changes.apply(MixEvent.ChannelLevelChanged(channel = 0L, level = 30L))
            }
            assertEquals("screen gone", failure.message)
            assertEquals(1, sendCalls)
        }
    }

    /** P3: after rejected sends, numbering carries on and every later mix is handed over in order. */
    @Test
    fun p3_afterRejectionsNumberingCarriesOn() {
        connected().use { model ->
            val sent = mutableListOf<ULong>()
            var closed = true
            val changes = MixChanges(
                model,
                show = {},
                send = { if (closed) throw RejectedExecutionException("closed") else sent += it.sequence!! },
            )
            val before = (1..5).map { changes.apply(MixEvent.ChannelLevelChanged(channel = 0L, level = 10L + it)) }
            closed = false
            val after = (1..5).map { changes.apply(MixEvent.ChannelLevelChanged(channel = 1L, level = 10L + it)) }
            val firstAfter = before.last().state.sync.latest + 1uL
            assertEquals((firstAfter until firstAfter + 5uL).toList(), sent)
            assertEquals(after.map { it.command!!.sequence!! }, sent)
        }
    }

    /**
     * P4: two threads, shows failing at random: every number the model took is handed to send once,
     * in order, and the states shown never go back.
     */
    @Test
    fun p4_failingShowsUnderTwoThreadsLoseNoNumberedMix() {
        connected().use { model ->
            val sent: MutableList<ULong> = Collections.synchronizedList(mutableListOf())
            val shown: MutableList<ULong> = Collections.synchronizedList(mutableListOf())
            val counter = AtomicInteger()
            val changes = MixChanges(
                model,
                show = { state ->
                    if (counter.incrementAndGet() % 5 == 0) error("flaky screen")
                    shown += state.sync.latest
                    Thread.yield()
                },
                send = { command ->
                    sent += command.sequence!!
                    Thread.yield()
                },
            )
            val baseline = model.state().sync.latest
            val failures = AtomicInteger()
            val start = CountDownLatch(1)
            val threads = listOf(0L, 1L).map { channel ->
                thread {
                    start.await()
                    repeat(3_000) { round ->
                        try {
                            changes.apply(MixEvent.ChannelLevelChanged(channel = channel, level = (round % 101).toLong()))
                        } catch (_: IllegalStateException) {
                            failures.incrementAndGet()
                        }
                        try {
                            changes.apply(MixEvent.MasterVolumeChanged(level = ((round + channel) % 101)))
                        } catch (_: IllegalStateException) {
                            failures.incrementAndGet()
                        }
                        if (round % 50 == 25) changes.sendOpeningMix()
                    }
                }
            }
            start.countDown()
            threads.forEach(Thread::join)

            assertTrue("some shows failed", failures.get() > 100)
            val latest = model.state().sync.latest
            // Openings repeat the newest number; drop those repeats to get the numbered mixes.
            val distinct = sent.distinct()
            assertEquals("never back", sent.sorted(), sent)
            assertEquals("every number handed over", ((baseline + 1uL)..latest).toList(), distinct)
            assertEquals("shown never back", shown.sorted(), shown)
        }
    }

    /**
     * P5: a real executor shut down while two threads change the mix: nothing escapes, and what was
     * delivered is every number up to some point, in order.
     */
    @Test
    fun p5_aChannelShutDownMidStreamDropsOnlyTheTail() {
        connected().use { model ->
            val executor = Executors.newSingleThreadExecutor()
            val delivered: MutableList<ULong> = Collections.synchronizedList(mutableListOf())
            val changes = MixChanges(
                model,
                show = { Thread.yield() },
                send = { command -> executor.execute { delivered += command.sequence!! } },
            )
            val baseline = model.state().sync.latest
            val escaped: MutableList<Throwable> = Collections.synchronizedList(mutableListOf())
            val start = CountDownLatch(1)
            val threads = listOf(0L, 1L).map { channel ->
                thread {
                    start.await()
                    try {
                        repeat(3_000) { round ->
                            changes.apply(MixEvent.ChannelLevelChanged(channel = channel, level = (round % 101).toLong()))
                            if (round == 1_000 && channel == 0L) executor.shutdown()
                        }
                    } catch (t: Throwable) {
                        escaped += t
                    }
                }
            }
            start.countDown()
            threads.forEach(Thread::join)
            assertTrue(executor.awaitTermination(10, TimeUnit.SECONDS))

            assertEquals(emptyList<Throwable>(), escaped)
            assertTrue("some delivered", delivered.isNotEmpty())
            assertEquals(((baseline + 1uL)..(baseline + delivered.size.toULong())).toList(), delivered)
            assertTrue("the tail was dropped", delivered.last() < model.state().sync.latest)
        }
    }
}
