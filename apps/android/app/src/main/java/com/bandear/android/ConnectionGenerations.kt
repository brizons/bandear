package com.bandear.android

import java.util.concurrent.atomic.AtomicLong

/**
 * Tells the current control connection apart from older ones, so a late callback from a socket that
 * was replaced, lost or stopped can never touch the connection that is live now.
 *
 * Every connection attempt takes a fresh generation from [begin], and its socket's callbacks carry
 * that generation. A callback acts only while [isCurrent] holds for it. The end of a connection is
 * claimed with [end], which succeeds exactly once, so a failure and a close racing on two threads
 * schedule one reconnect, not two. [invalidate] makes every existing generation stale at once, for
 * a stop. Values set by [end] and [invalidate] are never handed out by [begin], so no socket can
 * become current again.
 */
internal class ConnectionGenerations {
    private val current = AtomicLong(0L)

    /** Starts a new connection attempt and returns its generation; every earlier one becomes stale. */
    fun begin(): Long = current.incrementAndGet()

    /** True while [generation] belongs to the live connection attempt. */
    fun isCurrent(generation: Long): Boolean = current.get() == generation

    /**
     * Ends [generation] if it is still the current one. True only for the first caller; false for a
     * stale generation or one that already ended, which leaves the current connection untouched.
     */
    fun end(generation: Long): Boolean = current.compareAndSet(generation, generation + 1)

    /** Makes every generation handed out so far stale. */
    fun invalidate() {
        current.incrementAndGet()
    }
}
