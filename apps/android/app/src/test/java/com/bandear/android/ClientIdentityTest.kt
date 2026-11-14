package com.bandear.android

import com.bandear.android.core.validClientId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.UUID

/**
 * Pure JVM coverage for the client id: how a new one is made, and when a stored one is kept or
 * replaced. What counts as a valid one is the shared core's rule, tested in
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

    @Test
    fun aStoredValidIdIsKeptAndNothingIsSaved() {
        val saved = mutableListOf<String>()

        val id = resolveClientId(stored = "phone-1", generate = { "unused" }, save = { saved += it })

        assertEquals("phone-1", id)
        assertTrue("nothing new is stored", saved.isEmpty())
    }

    @Test
    fun withNoStoredIdANewOneIsGeneratedAndSaved() {
        val saved = mutableListOf<String>()

        val id = resolveClientId(stored = null, generate = { "fresh-id" }, save = { saved += it })

        assertEquals("fresh-id", id)
        assertEquals(listOf("fresh-id"), saved)
    }

    @Test
    fun aCorruptedStoredIdIsReplacedAndSaved() {
        val saved = mutableListOf<String>()

        val id = resolveClientId(stored = "not valid!", generate = { "fresh-id" }, save = { saved += it })

        assertEquals("fresh-id", id)
        assertEquals(listOf("fresh-id"), saved)
    }
}
