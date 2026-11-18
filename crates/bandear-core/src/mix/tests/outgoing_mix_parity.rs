//! The iOS client's former `OutgoingMixParityTests`: the Android client's tests of the `mix` it
//! sends and of when it sends one, ported case for case: the two of its `ControlChannelTest` about
//! the message (they left that file with this port), and the send tests of its
//! `MonitorViewModelTest`, which Android keeps and runs through its view model.
//!
//! Android drove its ViewModel through a fake socket; here [`Phone`] drives the session through the
//! same events, sends what `MixSession::outgoing_mix` builds whenever a mix is numbered, and the
//! desk's reports arrive as the JSON Android's tests write. A `config` is handed over as the
//! session layer hands it ([`MixEvent::ConfigReceived`]), since its decoding has its own suite.
//! What Android checks on the receive thread (`receiver.mixState`) and in its store
//! (`store.channel`) belongs to the app and is left out where a test checks it.

use super::{
    choose, choose_none, config, config_with, configure, connected_with, controls, group, info,
    recall, set_cap, set_group_level, set_level, set_master, set_master_mute, set_mute, set_pan,
    set_solo, toggle, Detail, Phone, WireMix,
};
use crate::control::mix_bank::MixSnapshot;
use crate::control::server_message::{ChannelKind, DeskConfiguration};
use crate::mix::{MixEvent, MixSync, MoreOfMeNotice, RecallRefusal};
use serde_json::{json, Value};

// Fixtures, as the Android client's `MonitorViewModelTest` named them.

/// A mix report of `message_type` as the desk writes it (`mix_state` when the mix changed
/// elsewhere, `mix_ack` answering the phone's mix number `seq`): its full 32-channel and 16-group
/// tables, with `levels` on the first channels and the rest neutral.
fn mix_state(
    levels: &[i64],
    volume: i64,
    cap: i64,
    muted: bool,
    message_type: &str,
    seq: Option<i64>,
) -> Value {
    let mut object = json!({
        "type": message_type,
        "volume_percent": volume,
        "max_level_percent": cap,
        "muted": muted,
        "channels": (0..32).map(|c| levels.get(c).copied().unwrap_or(100)).collect::<Vec<i64>>(),
        "pans": (0..32).map(|c| if c % 2 == 0 { 0 } else { 100 }).collect::<Vec<i64>>(),
        "mutes": vec![false; 32],
        "solos": vec![false; 32],
        "group_levels": vec![100; 16],
    });
    if let Some(seq) = seq {
        object["seq"] = json!(seq);
    }
    object
}

/// The former Swift fixture `mixState(levels:)` with every other argument at its default.
fn pushed_levels(levels: &[i64]) -> Value {
    mix_state(levels, 100, 100, false, "mix_state", None)
}

/// A `mix_state` of neutral levels carrying the engineer's `limit` and `locked` channels.
fn engineer_state(limit: i64, locked: &[usize]) -> Value {
    let mut object = pushed_levels(&[]);
    object["engineer_limit_percent"] = json!(limit);
    object["locked"] = json!((0..32).map(|c| locked.contains(&c)).collect::<Vec<_>>());
    object
}

/// A `mix_state` with every channel at `level` that names whose mix the phone follows.
fn following_state(name: &str, level: i64) -> Value {
    let mut object = pushed_levels(&[level; 32]);
    object["following"] = json!(name);
    object
}

/// The desk announces a keys pair on channels 0 and 1 and a voice on channel 2: Android's
/// `announcePair`.
fn announce_pair() -> MixEvent {
    configure(config_with(
        3,
        Vec::new(),
        Some(vec![
            info("Keys", ChannelKind::Keys).linked(),
            info("", ChannelKind::Keys),
            info("Vox", ChannelKind::Voice),
        ]),
    ))
}

/// Three channels and a group of the first two.
fn band_config() -> DeskConfiguration {
    config_with(3, vec![group("Band", &[0, 1])], None)
}

/// The desk's answer to the phone's `sent` mix when it kept its own channels and groups: every
/// channel from `levels` on and the first group at `group_level`.
fn answer_keeping_the_servers_mix(sent: &WireMix, levels: &[i64], group_level: i64) -> Value {
    let mut object = mix_state(levels, 100, 100, false, "mix_ack", sent.int("seq"));
    object["group_levels"] = json!((0..16)
        .map(|g| if g == 0 { group_level } else { 100 })
        .collect::<Vec<i64>>());
    object
}

/// `object` as an answer to the phone: its `type` set to `mix_ack`.
fn as_ack(mut object: Value) -> Value {
    object["type"] = json!("mix_ack");
    object
}

// From the Android client's `ControlChannelTest`.

/// Android pushes the controls in a `mix_state` and reads them off the mix it sends next.
#[test]
fn a_mix_never_carries_the_engineers_controls() {
    let mut phone = Phone::connected(config(2));
    phone.answer_mixes();
    phone.receive(engineer_state(60, &[0]));
    // Not in Android's test, whose sent mix never came from the state it pushed: the phone holds
    // the controls, so their absence below is the rule and not an accident.
    assert_eq!(phone.session.mix.engineer, controls(60, &[0], None));

    let mix = WireMix::new(&phone.session.outgoing_mix());

    assert!(!mix.has("engineer_limit_percent"));
    assert!(!mix.has("locked"));
    assert!(!mix.has("following"));
}

/// Android hands the channel its newest number through `mixSequence`, 12 and then null. The
/// session's number is never absent, so the unnumbered half is [`MixCommand`]'s: a mix without a
/// number writes no `seq`.
///
/// [`MixCommand`]: crate::control::client_message::MixCommand
#[test]
fn a_mix_carries_the_number_of_the_newest_change_and_none_without_one() {
    let mut session = connected_with(2, Vec::new());
    session.sync = MixSync {
        latest: 12,
        answered: 12,
    };

    let mut unnumbered = session.outgoing_mix();
    unnumbered.sequence = None;

    assert_eq!(WireMix::new(&session.outgoing_mix()).int("seq"), Some(12));
    assert!(!WireMix::new(&unnumbered).has("seq"));
}

// From the Android client's `MonitorViewModelTest`, what is sent.

#[test]
fn channel_changes_are_sent_as_a_mix() {
    let mut phone = Phone::connected(config(2));

    phone.apply(&[
        set_level(1, 40),
        set_mute(0, true),
        set_solo(1, true),
        set_pan(0, 50),
    ]);

    let last = phone.last_mix();
    assert_eq!(last.ints("channels"), Some(vec![100, 40]));
    assert_eq!(last.bools("mutes"), Some(vec![true, false]));
    assert_eq!(last.bools("solos"), Some(vec![false, true]));
    assert_eq!(last.ints("pans"), Some(vec![50, 100]));
    assert_eq!(phone.session.mix.levels, [100, 40]);
}

#[test]
fn a_pan_from_the_screens_is_kept_and_sent_as_a_mix() {
    let mut phone = Phone::connected(config(2));

    phone.apply(&[set_pan(1, 35)]);

    assert_eq!(phone.session.mix.pans, [0, 35]);
    assert_eq!(phone.last_mix().ints("pans"), Some(vec![0, 35]));
}

#[test]
fn a_change_that_changes_nothing_sends_nothing() {
    let mut phone = Phone::connected(config(2));
    let sent = phone.sent.len();

    phone.apply(&[set_level(0, 100), set_level(7, 20)]);

    assert_eq!(phone.sent.len(), sent);
    // Not in Android's test: a change that changes something is sent, so the count above is not
    // a phone that sends nothing at all.
    phone.apply(&[set_level(0, 90)]);
    assert_eq!(phone.sent.len(), sent + 1);
}

#[test]
fn group_levels_are_sent_as_a_mix() {
    let mut phone = Phone::connected(config_with(2, vec![group("Band", &[0, 1])], None));

    phone.apply(&[set_group_level(0, 65)]);

    assert_eq!(phone.last_mix().ints("group_levels"), Some(vec![65]));
    assert_eq!(
        phone
            .session
            .mix
            .groups
            .iter()
            .map(|group| group.name.as_str())
            .collect::<Vec<_>>(),
        ["Band"]
    );
}

/// The Swift test checked the strips' labels, "Keys" and "Vox"; here, the names they are labelled
/// by.
#[test]
fn a_pairs_strip_sends_both_sides_and_more_of_me_lifts_both() {
    let mut phone = Phone::connected(config(3));
    phone.apply(&[announce_pair()]);

    phone.apply(&[set_level(0, 60), set_pan(0, 75)]);

    assert_eq!(
        phone
            .session
            .mix
            .strips()
            .into_iter()
            .map(|strip| strip.name)
            .collect::<Vec<_>>(),
        [Some("Keys".to_owned()), Some("Vox".to_owned())]
    );
    assert_eq!(phone.last_mix().ints("channels"), Some(vec![60, 60, 100]));
    assert_eq!(phone.last_mix().ints("pans"), Some(vec![50, 100, 0]));

    phone.apply(&[choose(0), toggle(true)]);

    assert_eq!(phone.last_mix().ints("channels"), Some(vec![100, 100, 50]));
}

/// Android also checks the channel it stored (`store.channel`), which is the app's.
#[test]
fn a_config_that_pairs_the_musicians_channel_remembers_the_pair_and_sends_nothing() {
    let mut phone = Phone::connected(config(3));
    phone.apply(&[choose(1)]);
    let sent = phone.sent.len();

    phone.apply(&[announce_pair()]);

    assert_eq!(phone.session.more_of_me.musician_channel, Some(0));
    // The desk panned the new pair apart; its `mix_state` follows, so the phone's mix stays put.
    assert_eq!(phone.sent.len(), sent);
}

/// Android also checks the channel it stored (`store.channel`), which is the app's.
#[test]
fn a_config_that_pairs_the_musicians_channel_with_more_of_me_on_drops_the_lift_once_the_links_mix_arrives(
) {
    let mut phone = Phone::connected(config(3));
    phone.apply(&[
        set_level(0, 80),
        set_level(1, 60),
        set_level(2, 40),
        set_pan(0, 30),
        set_pan(1, 70),
        set_pan(2, 50),
        choose(1),
        toggle(true),
    ]);
    assert_eq!(phone.last_mix().ints("channels"), Some(vec![40, 100, 20]));
    phone.answer_mixes();
    let sent = phone.sent.len();

    phone.apply(&[announce_pair()]);

    assert_eq!(phone.session.more_of_me.musician_channel, Some(0));
    assert!(!phone.session.more_of_me.is_enabled);
    assert_eq!(
        phone.session.more_of_me.notice,
        Some(MoreOfMeNotice::ChannelPaired)
    );
    // The link's own `mix_state` follows with the pans it moved; a send now would put the phone's
    // old pans back over them.
    assert_eq!(phone.sent.len(), sent);

    // That push still carries the lift (the second side took the first's lifted level) and the
    // pans the link moved apart.
    let mut linked = pushed_levels(&[40, 40, 20]);
    linked["pans"] = json!((0..32)
        .map(|c| match c {
            1 => 100,
            2 => 50,
            _ => 0,
        })
        .collect::<Vec<i64>>());
    phone.receive(linked);

    // One whole mix goes out: the musician's own levels (the pair's at its first side's) with the
    // moved pans, and the phone keeps the same.
    assert_eq!(phone.sent.len(), sent + 1);
    assert_eq!(phone.last_mix().ints("channels"), Some(vec![80, 80, 40]));
    assert_eq!(phone.last_mix().ints("pans"), Some(vec![0, 100, 50]));
    assert_eq!(phone.session.mix.levels, [80, 80, 40]);
    assert_eq!(phone.session.mix.pans, [0, 100, 50]);
    assert!(!phone.session.more_of_me.is_enabled);

    phone.answer_mixes();
    phone.apply(&[toggle(true)]);

    assert_eq!(phone.last_mix().ints("channels"), Some(vec![100, 100, 20]));
}

/// Android also checks the receive thread's ceiling (`receiver.mixState`), which is the app's; it
/// is copied from [`MixState::effective_cap`](crate::mix::MixState::effective_cap).
#[test]
fn the_engineers_limit_holds_the_phones_own_playback_and_is_never_sent() {
    let mut phone = Phone::connected(config(3));
    phone.answer_mixes();

    phone.receive(engineer_state(60, &[]));
    phone.apply(&[set_cap(90)]);

    assert_eq!(phone.session.mix.effective_cap(), 60);
    let last = phone.last_mix();
    assert_eq!(last.int("max_level_percent"), Some(90));
    assert!(!last.has("engineer_limit_percent"));
    assert!(!last.has("locked"));
}

#[test]
fn a_locked_channels_edit_sends_nothing_and_more_of_me_leaves_it() {
    let mut phone = Phone::connected(config(3));
    phone.answer_mixes();
    phone.receive(engineer_state(100, &[0]));
    let sent = phone.sent.len();

    phone.apply(&[set_level(0, 40), set_mute(0, true)]);

    assert_eq!(phone.sent.len(), sent);

    phone.apply(&[choose(1), toggle(true)]);

    assert_eq!(phone.last_mix().ints("channels"), Some(vec![100, 100, 50]));
}

/// Android checks the receive thread's ceiling (`receiver.mixState.maxLevelPercent`), which is
/// the app's. The value it copies is the effective cap, checked here in its place.
#[test]
fn a_limit_only_report_moves_the_phones_own_ceiling_with_no_master_edit() {
    let mut phone = Phone::connected(config(3));
    phone.answer_mixes();
    assert_eq!(phone.session.mix.effective_cap(), 100);

    phone.receive(engineer_state(60, &[]));

    assert_eq!(phone.session.mix.effective_cap(), 60);

    // An answer that only lowers the limit moves it too; the change it answers kept the cap.
    phone.apply(&[set_level(0, 40)]);
    let mut answer = as_ack(Value::Object(phone.last_mix().object));
    answer["engineer_limit_percent"] = json!(45);
    answer["locked"] = json!(vec![false; 32]);
    phone.receive(answer);

    assert_eq!(phone.session.mix.effective_cap(), 45);
    assert_eq!(phone.session.mix.level_cap, 100);
}

#[test]
fn more_of_me_leaves_both_sides_of_a_half_locked_pair_in_what_it_sends() {
    let mut phone = Phone::connected(config(3));
    phone.apply(&[announce_pair()]);
    phone.answer_mixes();
    phone.receive(engineer_state(100, &[1]));

    phone.apply(&[choose(2), toggle(true)]);

    assert_eq!(phone.last_mix().ints("channels"), Some(vec![100, 100, 100]));
}

#[test]
fn while_following_only_the_master_moves_and_the_answer_brings_the_leaders_channels() {
    let mut phone = Phone::connected(config(3));
    phone.answer_mixes();
    phone.receive(following_state("Ana", 70));
    let sent = phone.sent.len();

    phone.apply(&[set_level(0, 40), set_mute(1, true)]);

    assert_eq!(phone.sent.len(), sent);

    phone.apply(&[set_master(50)]);

    assert_eq!(phone.last_mix().ints("channels"), Some(vec![70, 70, 70]));
    assert_eq!(phone.last_mix().int("volume_percent"), Some(50));

    // The desk keeps the leader's channels (moved on meanwhile) and the answer carries them.
    let mut answer = as_ack(following_state("Ana", 65));
    answer["seq"] = json!(phone.last_mix().int("seq"));
    answer["volume_percent"] = json!(50);
    phone.receive(answer);

    assert_eq!(phone.session.mix.levels, [65, 65, 65]);
    assert_eq!(phone.session.mix.master_volume, 50);
}

/// With the opening mix's lists checked as `aConfigArrivingBeforeTheOpeningMixIsSent…` checks
/// them; that test's own subject, a `config` landing before the queued opening mix is sent, is the
/// transport's.
#[test]
fn on_the_first_connect_the_phone_shows_the_mix_the_server_already_holds() {
    let mut phone = Phone::default();
    phone.apply(&[MixEvent::SessionStarted]);
    phone.opened();
    phone.apply(&[configure(band_config())]);

    assert_eq!(phone.sent.len(), 1);
    let opening = WireMix::new(&phone.sent[0]);
    assert_eq!(opening.ints("channels"), Some(vec![]));
    assert_eq!(opening.ints("pans"), Some(vec![]));
    assert_eq!(opening.bools("mutes"), Some(vec![]));
    assert_eq!(opening.bools("solos"), Some(vec![]));
    assert_eq!(opening.ints("group_levels"), Some(vec![]));
    // Not in Android's test: the master values are the phone's own, so they go out as they are.
    assert_eq!(opening.int("volume_percent"), Some(100));
    assert_eq!(opening.int("max_level_percent"), Some(100));

    phone.receive(answer_keeping_the_servers_mix(&opening, &[40, 50, 60], 70));

    assert_eq!(phone.session.mix.levels, [40, 50, 60]);
    assert_eq!(phone.session.mix.group_levels, [70]);
    assert_eq!(phone.sent.len(), 1);
}

/// Android also checks the receive thread's copy (`receiver.mixState`), which is the app's.
#[test]
fn master_changes_reach_the_audio_thread_and_the_server() {
    let mut phone = Phone::connected(config(2));

    phone.apply(&[set_master(40), set_cap(80), set_master_mute(true)]);

    let last = phone.last_mix();
    assert_eq!(last.int("volume_percent"), Some(40));
    assert_eq!(last.int("max_level_percent"), Some(80));
    assert_eq!(
        last.object.get("muted").and_then(Value::as_bool),
        Some(true)
    );
}

// From the Android client's `MonitorViewModelTest`, More of me.

/// Android also checks the channel it stored (`store.channel`), which is the app's.
#[test]
fn more_of_me_lifts_the_musicians_channel_only_in_what_is_sent() {
    let mut phone = Phone::connected(config(2));
    phone.apply(&[set_level(0, 80)]);
    let sent = phone.sent.len();

    phone.apply(&[toggle(true)]);

    assert_eq!(
        phone.session.more_of_me.notice,
        Some(MoreOfMeNotice::NeedsChannel)
    );
    assert_eq!(phone.sent.len(), sent);

    phone.apply(&[choose(1), toggle(true)]);

    assert_eq!(phone.last_mix().ints("channels"), Some(vec![40, 100]));
    assert_eq!(phone.session.mix.levels, [80, 100]);

    phone.apply(&[toggle(false)]);

    assert_eq!(phone.last_mix().ints("channels"), Some(vec![80, 100]));
}

/// Android also checks the channel it stored (`store.channel`), which is the app's.
#[test]
fn choosing_no_channel_turns_more_of_me_off_and_resends_the_mix() {
    let mut phone = Phone::connected(config(2));
    phone.apply(&[choose(0), toggle(true)]);
    assert_eq!(phone.last_mix().ints("channels"), Some(vec![100, 50]));

    phone.apply(&[choose_none()]);

    assert!(!phone.session.more_of_me.is_enabled);
    assert_eq!(phone.last_mix().ints("channels"), Some(vec![100, 100]));
}

// From the Android client's `MonitorViewModelTest`, a mix the desk pushed.

/// Android also checks the receive thread's copy (`receiver.mixState`), which is the app's.
#[test]
fn a_pushed_mix_is_shown_and_heard_without_being_sent_back() {
    let mut phone = Phone::connected(config(2));
    phone.answer_mixes();
    let sent = phone.sent.len();

    phone.receive(mix_state(&[80, 70], 50, 90, true, "mix_state", None));

    assert_eq!(phone.session.mix.levels, [80, 70]);
    assert_eq!(phone.session.mix.master_volume, 50);
    assert_eq!(phone.session.mix.effective_cap(), 90);
    assert!(phone.session.mix.master_muted);
    assert_eq!(phone.sent.len(), sent);
}

/// Android also checks the channel it stored (`store.channel`), which is the app's.
#[test]
fn a_push_during_more_of_me_turns_it_off_without_sending_and_the_next_move_sends_the_pushed_mix() {
    let mut phone = Phone::connected(config(2));
    phone.apply(&[choose(1), toggle(true)]);
    phone.answer_mixes();
    let sent = phone.sent.len();

    phone.receive(pushed_levels(&[80, 70]));

    assert!(!phone.session.more_of_me.is_enabled);
    assert_eq!(
        phone.session.more_of_me.notice,
        Some(MoreOfMeNotice::MixChangedElsewhere)
    );
    assert_eq!(phone.sent.len(), sent);

    // A fader still moving when the push landed sends its own value with the pushed mix around
    // it; the desk keeps whichever whole mix it got last.
    phone.apply(&[set_level(1, 30)]);

    assert_eq!(phone.last_mix().ints("channels"), Some(vec![80, 30]));
}

#[test]
fn the_server_sending_back_the_mix_the_phone_sent_changes_nothing() {
    let mut phone = Phone::connected(config(2));
    phone.apply(&[choose(1), toggle(true)]);
    phone.answer_mixes();
    let before = phone.session.clone();
    let sent = phone.sent.len();
    // Not in Android's test: the mix sent last is lifted, so hearing it back is not vacuous.
    assert!(phone.session.more_of_me.is_enabled && sent == 2);

    // After missing events the desk re-sends the whole mix it holds, which is the phone's own.
    let mut resent = Value::Object(phone.last_mix().object);
    resent["type"] = json!("mix_state");
    phone.receive(resent);

    assert_eq!(phone.session, before);
    assert_eq!(phone.sent.len(), sent);
}

// From the Android client's `MonitorViewModelTest`, the desk's answers to the phone's mixes.

#[test]
fn a_push_older_than_a_mix_still_in_flight_gives_way_to_the_answer_to_that_mix() {
    let mut phone = Phone::connected(config(2));
    phone.answer_mixes();
    phone.apply(&[set_level(0, 40)]);
    let sent = phone.sent.len();

    // The desk pushed its mix before the phone's newer one reached it; the gesture has ended.
    phone.receive(pushed_levels(&[80, 70]));

    assert_eq!(phone.session.mix.levels, [40, 100]);

    phone.answer_mixes();

    assert_eq!(phone.session.mix.levels, [40, 100]);
    assert_eq!(phone.sent.len(), sent);
}

/// Android also checks the receive thread's copy (`receiver.mixState`), which is the app's.
#[test]
fn with_two_mixes_in_flight_only_the_answer_to_the_newest_is_taken() {
    let mut phone = Phone::connected(config(2));
    phone.answer_mixes();
    phone.apply(&[set_level(0, 40), set_level(0, 50)]);
    assert!(phone.sent.len() >= 2);
    let first = WireMix::new(&phone.sent[phone.sent.len() - 2]).int("seq");
    let second = WireMix::new(&phone.sent[phone.sent.len() - 1]).int("seq");
    let sent = phone.sent.len();

    phone.receive(mix_state(&[40, 100], 100, 100, false, "mix_ack", first));

    assert_eq!(phone.session.mix.levels, [50, 100]);

    // What the desk answers to the newest mix is what it plays, so the phone takes it.
    phone.receive(mix_state(&[50, 60], 70, 100, false, "mix_ack", second));

    assert_eq!(phone.session.mix.levels, [50, 60]);
    assert_eq!(phone.session.mix.master_volume, 70);
    assert_eq!(phone.sent.len(), sent);
}

#[test]
fn an_answer_that_is_the_mix_the_phone_plays_changes_nothing() {
    let mut phone = Phone::connected(config(2));
    phone.apply(&[choose(1), toggle(true)]);
    let before = phone.session.clone();
    let sent = phone.sent.len();
    // Not in Android's test: a mix lifted by More of me is on its way, so its answer below is not
    // vacuous.
    assert!(!phone.session.sync.is_settled());

    // The answers carry the mixes as sent, the last one lifted by More of me.
    phone.answer_mixes();

    assert_eq!(phone.session.mix, before.mix);
    assert_eq!(phone.session.more_of_me, before.more_of_me);
    assert!(phone.session.more_of_me.is_enabled);
    assert_eq!(phone.sent.len(), sent);
}

#[test]
fn an_error_without_a_number_never_makes_an_older_answer_the_newest() {
    let mut phone = Phone::connected(config(2));
    phone.answer_mixes();
    phone.apply(&[set_level(0, 40), set_level(0, 50)]);
    assert!(phone.sent.len() >= 2);
    let first = WireMix::new(&phone.sent[phone.sent.len() - 2]).object;
    let second = WireMix::new(&phone.sent[phone.sent.len() - 1]).int("seq");

    // An error answering a stats report, then the answer to the first mix, then a push: the second
    // mix is still on its way, so none of them is the desk's latest word.
    phone.receive(json!({"type": "error", "message": "no UDP target configured for client IP"}));
    phone.receive(as_ack(Value::Object(first)));
    phone.receive(pushed_levels(&[80, 70]));

    assert_eq!(phone.session.mix.levels, [50, 100]);

    phone.receive(mix_state(&[50, 60], 100, 100, false, "mix_ack", second));

    assert_eq!(phone.session.mix.levels, [50, 60]);
}

// From the Android client's `MonitorViewModelTest`, a saved mix recalled.
//
// The session's half of two saved-mix tests: what a recall sends. The save is [`MixBank`]'s
// (`mix_bank_parity`).
//
// [`MixBank`]: crate::mix::MixBank

/// The recall half of Android's `savingAndRecallingAMix`. Android's `receiver.mixState` is the
/// app's, so the master is checked on the session and in the mix sent.
#[test]
fn saving_and_recalling_a_mix() {
    let mut phone = Phone::connected(config(2));
    phone.apply(&[set_level(0, 30)]);
    let saved = phone.session.mix.snapshot();
    phone.apply(&[set_level(0, 90), set_master(50)]);
    let sent = phone.sent.len();

    phone.apply(&[recall(saved)]);

    assert_eq!(phone.session.mix.levels, [30, 100]);
    // Not in Android's test: the recall is numbered as one change and sent once.
    assert_eq!(phone.sent.len(), sent + 1);
    assert_eq!(phone.last_mix().ints("channels"), Some(vec![30, 100]));
    assert_eq!(phone.session.mix.master_volume, 100);
    assert_eq!(phone.last_mix().int("volume_percent"), Some(100));
}

/// Android's `recallingWhileFollowingSendsNothingAndSaysWhy`. Its notice is the screen's; the
/// refusal it words is checked here.
#[test]
fn recalling_while_following_sends_nothing_and_says_why() {
    let mut phone = Phone::connected(config(3));
    phone.answer_mixes();
    phone.receive(following_state("Ana", 70));
    let sent = phone.sent.len();

    phone.apply(&[recall(MixSnapshot {
        channel_gains: vec![10, 20, 30],
        ..MixSnapshot::default()
    })]);

    assert_eq!(phone.sent.len(), sent);
    assert_eq!(
        phone.session.recall_refusal(),
        Some(RecallRefusal::Following {
            leader: "Ana".to_owned()
        })
    );
    assert_eq!(phone.session.mix.levels, [70, 70, 70]);
}
