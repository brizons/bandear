package com.bandear.android.ui.state

import com.bandear.android.core.MixBankChange
import com.bandear.android.core.MixCommand
import com.bandear.android.core.MixEvent
import com.bandear.android.core.MixModel
import com.bandear.android.core.MixModelState
import com.bandear.android.core.MixRecall
import com.bandear.android.core.MixUpdate
import com.bandear.android.core.SavedMix

/**
 * The phone's [MixModel] as two threads change it: the main thread with the musician's edits and
 * the saved mixes, the control thread with what the desk reports and the mix each connection opens
 * with.
 *
 * Every call runs under one hold together with what it asks of the phone, in this order: [show] the
 * state it left, then [send] the mix it numbered, then the caller's own effects. The model already
 * lands a change and its number together, under its own lock; this hold keeps the order around it:
 *
 * - States are shown in the order the model made them, so the screen never goes back to an older
 *   mix: no other change can come between a change and its showing.
 * - Mixes reach [send] in the order of their numbers. [send] only queues them for the control
 *   channel's one thread, which sends in queue order, so the desk gets them in that order and its
 *   answers are matched to the mixes they answer. The mix a connection opens with is read and
 *   queued under the same hold, so it never overtakes a newer change already queued.
 * - Each mix sent is the one its own change returned, never read again later: a change made since
 *   cannot slip into a mix numbered before it.
 *
 * No call is ever retried, since a change has effects (a number taken, a mix sent). [show], [send]
 * and every callback run under the hold, so they must never wait for another thread that may want
 * it: they publish, queue or store, and return.
 */
internal class MixChanges(
    private val model: MixModel,
    private val show: (MixModelState) -> Unit,
    private val send: (MixCommand) -> Unit,
) {
    private val hold = Any()

    /** The state as it stands. */
    fun state(): MixModelState = synchronized(hold) { model.state() }

    /** Applies [event]: shows the state it left, sends the mix it numbered, then runs [alongside]. */
    fun apply(event: MixEvent, alongside: (MixUpdate) -> Unit = {}): MixUpdate = synchronized(hold) {
        model.apply(event).also { update ->
            taken(update)
            alongside(update)
        }
    }

    /**
     * Recalls the mix saved as [name] among [savedMixes], the mixes the phone's store holds now
     * (the model keeps no store of its own, so it reads them first): shows the state, sends the mix
     * when it was recalled, then runs [alongside].
     */
    fun recall(name: String, savedMixes: List<SavedMix>, alongside: (MixRecall) -> Unit = {}): MixRecall =
        synchronized(hold) {
            model.reloadSavedMixes(savedMixes)
            model.recallMix(name).also { recall ->
                taken(recall.update)
                alongside(recall)
            }
        }

    /**
     * Saves, renames or deletes a saved mix through [change]. When the list changed, [store] gets it
     * whole, as the phone keeps it; then the state is shown and [alongside] runs.
     */
    fun changeSavedMixes(
        change: (MixModel) -> MixBankChange,
        store: (List<SavedMix>) -> Unit,
        alongside: (MixBankChange) -> Unit = {},
    ): MixBankChange = synchronized(hold) {
        change(model).also { changed ->
            if (changed.savedMixesChanged) store(model.savedMixes())
            show(changed.state)
            alongside(changed)
        }
    }

    /** Sends the mix a connection opens with: the mix as it stands, under its newest number. */
    fun sendOpeningMix() {
        synchronized(hold) { send(model.openingMix()) }
    }

    private fun taken(update: MixUpdate) {
        show(update.state)
        update.command?.let(send)
    }
}
