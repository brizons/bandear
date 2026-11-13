package com.bandear.android.core

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Proves the Rust core is linked and callable through the generated bindings: JNA loads the host
 * build of `libbandear_ffi` (Gradle passes its directory as `jna.library.path`) and the call returns
 * the version written in `crates/bandear-core/Cargo.toml`.
 */
class CoreVersionTest {
    @Test
    fun coreVersionIsTheVersionOfTheRustCore() {
        assertEquals(coreManifestVersion(), coreVersion())
    }

    /** `version = "..."` from bandear-core's manifest, whose path Gradle passes in. */
    private fun coreManifestVersion(): String {
        val manifest = File(System.getProperty("bandear.coreManifest")).readLines()
        return manifest.first { it.startsWith("version = \"") }
            .removePrefix("version = \"")
            .removeSuffix("\"")
    }
}
