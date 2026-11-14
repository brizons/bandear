package com.bandear.android.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.lang.reflect.Method
import java.nio.ByteBuffer

/**
 * Regression test from the B1 recheck: calls the private JNI functions directly, by
 * reflection, with the arguments the Kotlin wrapper never passes, to check the native side's own
 * refusals: a closed handle, a null or wrong-length words array, a null, heap or empty outcome
 * buffer, a datagram length outside its array.
 */
class NativeArgumentsTest {
    private val type = NativeReceivePipeline::class.java

    private fun method(name: String): Method =
        type.declaredMethods.single { it.name == name }.also { it.isAccessible = true }

    private fun handleOf(pipeline: NativeReceivePipeline): Long =
        type.getDeclaredField("handle").also { it.isAccessible = true }.getLong(pipeline)

    private val invalid = -2

    @Test
    fun theNativeSideRefusesWhatTheWrapperNeverSends() {
        val pipeline = NativeReceivePipeline(0L)
        val handle = handleOf(pipeline)
        val onWake = method("nativeOnWake")
        val opened = method("nativeOnOutputOpened")
        val failed = method("nativeOnOutputFailed")
        val take = method("nativeTakeOutcome")
        val due = method("nativeReportDueAtMs")
        val destroy = method("nativeDestroy")
        val direct = ByteBuffer.allocateDirect(4_096)
        val words = LongArray(25)
        val datagram = ByteArray(64)

        // A closed (0) handle, on every call.
        assertEquals(invalid, onWake.invoke(pipeline, 0L, words, null, 0, direct))
        assertEquals(invalid, opened.invoke(pipeline, 0L, 0L, 0L, LongArray(6), direct))
        assertEquals(invalid, failed.invoke(pipeline, 0L, direct))
        assertEquals(invalid, take.invoke(pipeline, 0L, direct))
        assertEquals(0L, due.invoke(pipeline, 0L))
        destroy.invoke(pipeline, 0L)

        // Words: null, too few, too many.
        assertEquals(invalid, onWake.invoke(pipeline, handle, null, null, 0, direct))
        assertEquals(invalid, onWake.invoke(pipeline, handle, LongArray(24), null, 0, direct))
        assertEquals(invalid, onWake.invoke(pipeline, handle, LongArray(26), null, 0, direct))
        assertEquals(invalid, opened.invoke(pipeline, handle, 0L, 0L, LongArray(25), direct))

        // The outcome buffer: null, on the heap, empty.
        assertEquals(invalid, onWake.invoke(pipeline, handle, words, null, 0, null))
        assertEquals(invalid, onWake.invoke(pipeline, handle, words, null, 0, ByteBuffer.allocate(4_096)))
        assertEquals(invalid, onWake.invoke(pipeline, handle, words, null, 0, ByteBuffer.allocateDirect(0)))

        // A datagram length outside its array, including Int extremes.
        for (length in listOf(65, -1, Int.MIN_VALUE, Int.MAX_VALUE)) {
            assertEquals(invalid, onWake.invoke(pipeline, handle, words, datagram, length, direct))
        }
        // A null datagram ignores its length: a timeout, answered with an outcome (a size >= 16).
        val timeout = onWake.invoke(pipeline, handle, words, null, Int.MIN_VALUE, direct) as Int
        assertTrue("timeout answered $timeout", timeout >= 16)
        // A buffer too small for the header is answered with the size it needs, not overrun.
        val tiny = ByteBuffer.allocateDirect(1)
        val needed = onWake.invoke(pipeline, handle, words, null, 0, tiny) as Int
        assertEquals(16, needed)
        assertEquals(0.toByte(), tiny.get(0))

        // The pipeline still works after every refusal.
        assertEquals(16, take.invoke(pipeline, handle, direct))
        pipeline.close()
        pipeline.close()
    }
}
