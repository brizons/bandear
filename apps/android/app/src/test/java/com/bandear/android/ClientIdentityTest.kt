package com.bandear.android

import com.bandear.android.core.validClientId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test
import java.util.UUID

/**
 * Pure JVM coverage for the client id: how a new one is made. When a stored one is kept or replaced
 * is the shared core's rule ([com.bandear.android.core.chooseClientId]), tested in
 * `crates/bandear-core/src/control/client_identity.rs`.
 */
class ClientIdentityTest {
    @Test
    fun aNewClientIdIsARandomUuidTheServerAccepts() {
        val first = newClientId()
        val second = newClientId()

        assertEquals(first, UUID.fromString(first).toString())
        assertEquals(first, validClientId(first))
        assertNotEquals(first, second)
    }
}
