//! How the phone matches the desk's answers to the mixes it sent.

/// How the phone matches the desk's answers to the mixes it sent.
///
/// The phone numbers every change it sends, and the desk answers each `mix` in order with that
/// number (`seq`): with a `mix_ack` and the whole mix it now plays, or with an `error`. `latest` is
/// the number of the newest change, taken in the same step as the change, so a send that is still
/// queued already counts. `answered` is the newest number the desk answered. Only the answer to
/// `latest` is the desk's latest word on the mix; while an older one is outstanding, any mix the
/// desk reports is overtaken by a mix of the phone's still on its way.
///
/// The numbers only grow, across reconnects too: every connection sends the mix again under
/// `latest`, so its answer settles it.
///
/// Ported from the iOS client's former `MixSync`, itself from the Android client's former
/// `MixSyncState`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MixSync {
    /// The number of the newest change the phone made to send; 0 before the first.
    pub latest: u64,
    /// The newest number the desk answered.
    pub answered: u64,
}

impl MixSync {
    /// True when the desk answered the newest change, so nothing of the phone's is on its way.
    pub fn is_settled(&self) -> bool {
        self.answered >= self.latest
    }

    /// A new change, numbered as the newest. The number stops at `u64::MAX` rather than wrap (the
    /// former Swift `MixSync` trapped there); no phone makes that many changes.
    pub fn changed(self) -> MixSync {
        MixSync {
            latest: self.latest.saturating_add(1),
            answered: self.answered,
        }
    }

    /// The desk answered change `sequence`; it answers in order, so every older one is answered
    /// too.
    pub fn answered_up_to(self, sequence: u64) -> MixSync {
        MixSync {
            latest: self.latest,
            answered: self.answered.max(sequence),
        }
    }
}
