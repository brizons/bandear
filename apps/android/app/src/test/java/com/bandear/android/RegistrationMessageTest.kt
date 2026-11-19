package com.bandear.android

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException
import java.util.concurrent.atomic.AtomicBoolean

/**
 * The registration the control channel sends when it opens, written by the shared core
 * (`Registration`): the core's rules (trimming and capping the name, dropping a client id the desk
 * would not keep, each capability once, refusing port 0) are tested in
 * `crates/bandear-core/src/control/client_message.rs`; these tests cover the channel's wiring.
 */
class RegistrationMessageTest {
    private val transport = RecordingTransport()
    private val scheduler = StepScheduler()
    private val listener = RecordingListener()
    private val channel = ControlChannel(
        listener = listener,
        running = AtomicBoolean(true),
        currentSnapshot = { MixSnapshot() },
        moreOfMeEnabled = { false },
        musicianChannels = { emptyList() },
        transport = transport,
        scheduler = scheduler,
    )

    @Test
    fun theRegistrationIsTheCoresMessageForWhatTheMusicianTyped() {
        channel.connect("192.168.1.20", 9001, 50000, "  ${"é".repeat(70)}  ", "has space")
        val socket = transport.sockets.single()
        socket.events.onOpen(socket)

        assertEquals(
            """{"type":"register","name":"${"é".repeat(64)}","udp_port":50000,""" +
                """"capabilities":["stats.v2","playback.buffer","redundancy.recovered","redundancy.hold"]}""",
            socket.sent.first(),
        )
    }

    /** From the T7 verification: a lone surrogate in the name made `connect()` throw. */
    @Test
    fun aNameWithALoneSurrogateRegistersWithTheReplacementCharacter() {
        channel.connect("192.168.1.20", 9001, 50000, "A\uD83Dna")
        val socket = transport.sockets.single()
        socket.events.onOpen(socket)

        assertEquals(
            """{"type":"register","name":"A�na","udp_port":50000,""" +
                """"capabilities":["stats.v2","playback.buffer","redundancy.recovered","redundancy.hold"]}""",
            socket.sent.first(),
        )
    }

    @Test
    fun aRegistrationTheDeskWouldRefuseIsNeverSentAndTheMusicianIsTold() {
        for (udpPort in listOf(0, -1, 65_536)) {
            channel.connect("192.168.1.20", 9001, udpPort, "Ana")
            val socket = transport.sockets.last()
            socket.events.onOpen(socket)

            assertEquals(listOf("mix"), socket.sent.map { JSONObject(it).getString("type") })
        }
        assertEquals(List(3) { "Could not register: the UDP port must be between 1 and 65535" }, listener.errors)
    }

    /**
     * The id given to [ControlChannel.connect] travels in every registration of the session: the
     * first open, an open after a lost connection, and a rename on the open channel.
     */
    @Test
    fun theChannelAnnouncesTheSameClientIdOnEveryOpenAndOnARename() {
        channel.connect("192.168.1.20", 9001, 50000, "Ana", CLIENT_ID)
        val first = transport.sockets.single()
        first.events.onOpen(first)
        first.events.onFailure(first, IOException("wifi gone"))
        scheduler.runDelayed()
        val second = transport.sockets[1]
        second.events.onOpen(second)
        channel.updateRegistrationName("Bea")
        scheduler.runDelayed()

        val registrations = transport.sockets.flatMap { socket -> socket.sent.map(::JSONObject) }
            .filter { it.getString("type") == "register" }
        assertEquals(listOf("Ana", "Ana", "Bea"), registrations.map { it.getString("name") })
        assertEquals(listOf(CLIENT_ID, CLIENT_ID, CLIENT_ID), registrations.map { it.optString("client_id") })
        assertTrue(
            "every registration announces the capabilities",
            registrations.all { it.getJSONArray("capabilities").getString(0) == "stats.v2" },
        )
    }

    private class RecordingSocket(val events: ControlSocketEvents) : ControlSocket {
        val sent = mutableListOf<String>()

        override fun send(text: String): Boolean {
            sent += text
            return true
        }

        override fun close(code: Int, reason: String?): Boolean = true

        override fun cancel() = Unit
    }

    private class RecordingTransport : ControlTransport {
        val sockets = mutableListOf<RecordingSocket>()

        override fun open(url: String, events: ControlSocketEvents): ControlSocket =
            RecordingSocket(events).also { sockets += it }

        override fun shutdown() = Unit
    }

    /** Runs immediate work at once and delayed work only when the test asks for it. */
    private class StepScheduler : ControlScheduler {
        private val delayed = mutableListOf<() -> Unit>()

        override fun execute(task: () -> Unit) = task()

        override fun schedule(delayMs: Long, task: () -> Unit): ScheduledTask {
            delayed += task
            return ScheduledTask { delayed.remove(task) }
        }

        override fun shutdown() = Unit

        fun runDelayed() {
            val due = delayed.toList()
            delayed.clear()
            due.forEach { it() }
        }
    }

    private class RecordingListener : ControlChannel.Listener {
        val errors = mutableListOf<String>()

        override fun onOpened() = Unit
        override fun onConfig(channels: Int, groups: List<GroupInfo>, details: List<ChannelInfo>?) = Unit
        override fun onAcknowledged() = Unit
        override fun onClosed() = Unit
        override fun onControlError(message: String) {
            errors += message
        }
    }

    private companion object {
        const val CLIENT_ID = "0b6f1c9e-2d4a-4f7b-9c3e-5a8d7e6f1a2b"
    }
}
