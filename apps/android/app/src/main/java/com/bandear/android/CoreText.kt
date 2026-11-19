package com.bandear.android

/**
 * This text with every lone surrogate replaced by U+FFFD, ready to cross into the shared core.
 *
 * The core takes UTF-8, and UniFFI's Kotlin binding encodes a `String` with
 * `CodingErrorAction.REPORT`: a lone surrogate would throw `MalformedInputException` out of the
 * call, out of `connect()` for a musician name or out of reading the saved mixes. No version of
 * the app writes one (its text fields never split a surrogate pair), so this is a guard: well-formed
 * text, the only kind the app produces, is returned as it is.
 */
internal fun String.wellFormedForCore(): String {
    if (!hasLoneSurrogate()) return this
    val text = StringBuilder(length)
    var index = 0
    while (index < length) {
        val char = this[index]
        val pairs = char.isHighSurrogate() && index + 1 < length && this[index + 1].isLowSurrogate()
        when {
            pairs -> {
                text.append(char).append(this[index + 1])
                index += 2
                continue
            }
            char.isSurrogate() -> text.append(REPLACEMENT_CHARACTER)
            else -> text.append(char)
        }
        index += 1
    }
    return text.toString()
}

/** Whether a surrogate here is not half of a high-then-low pair. */
private fun String.hasLoneSurrogate(): Boolean {
    var index = 0
    while (index < length) {
        val char = this[index]
        if (char.isHighSurrogate() && index + 1 < length && this[index + 1].isLowSurrogate()) {
            index += 2
            continue
        }
        if (char.isSurrogate()) return true
        index += 1
    }
    return false
}

private const val REPLACEMENT_CHARACTER = '�'
