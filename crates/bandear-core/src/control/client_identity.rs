//! This installation's client id: what counts as one the desk keeps, and when a stored one is kept.
//!
//! Ported from the rules of the iOS client's `ClientIdentity` (itself ported from Android's
//! `ClientIdentity`), without its storage and without making a new id; both apps' `ClientIdentity`
//! files keep those and call this module for the rule: a new one is a random UUID, which each
//! platform makes with its own generator, and keeping it is each platform's storage.
//!
//! The desk keys musicians by address, so a phone that comes back from a new one (DHCP after a
//! Wi-Fi blip) would otherwise appear as a second musician; with the id the desk moves the
//! musician's row, mix included, to the new address. It names this installation only: it is
//! random, carries nothing about the device or the musician, and is never shown.

use super::client_message::is_kept_client_id;

/// `raw` when the desk keeps it as a client id (1 to
/// [`Registration::MAXIMUM_CLIENT_ID_LENGTH`](super::client_message::Registration::MAXIMUM_CLIENT_ID_LENGTH)
/// ASCII letters, digits and hyphens), otherwise `None`, which the desk reads as no id at all
/// (Android's former `validClientId`, the name both apps call this by). The rule is the
/// registration's, written once.
pub fn valid_client_id(raw: Option<&str>) -> Option<&str> {
    raw.filter(|raw| is_kept_client_id(raw))
}

/// The client id a phone announces, and whether it must keep it: what [`choose_client_id`]
/// answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientIdChoice {
    /// The id to announce in every registration.
    pub client_id: String,
    /// Whether `client_id` is the new one, so the phone must store it from now on.
    pub should_save: bool,
}

/// [`resolve_client_id`] without closures, for the apps across the binding: the `stored` id when
/// the desk keeps it, with nothing to save; otherwise (the first launch, or a value damaged on the
/// device) `generated`, a new id the platform made, which the phone must save. Making the id and
/// keeping it stay on each platform.
pub fn choose_client_id(stored: Option<&str>, generated: String) -> ClientIdChoice {
    choose(stored, || generated)
}

/// The `stored` id when the desk keeps it. Otherwise (the first launch, or a value damaged on the
/// device) a new one from `generate`, handed to `save` and returned (Android's `resolveClientId`).
pub fn resolve_client_id(
    stored: Option<&str>,
    generate: impl FnOnce() -> String,
    save: impl FnOnce(&str),
) -> String {
    let choice = choose(stored, generate);
    if choice.should_save {
        save(&choice.client_id);
    }
    choice.client_id
}

/// The rule both forms share: the `stored` id when the desk keeps it, otherwise one from
/// `generate`, which is called only then.
fn choose(stored: Option<&str>, generate: impl FnOnce() -> String) -> ClientIdChoice {
    match valid_client_id(stored) {
        Some(kept) => ClientIdChoice {
            client_id: kept.to_owned(),
            should_save: false,
        },
        None => ClientIdChoice {
            client_id: generate(),
            should_save: true,
        },
    }
}

#[cfg(test)]
mod android_parity {
    //! The iOS client's `ClientIdentityParityTests`, its port of the Android client's
    //! `ClientIdentityTest`: one test per Swift test ported. The tests of making and keeping an id
    //! stay in both apps' files. Android's former `MAX_CLIENT_ID_LENGTH` is
    //! [`Registration::MAXIMUM_CLIENT_ID_LENGTH`].

    use super::*;
    use crate::control::client_message::Registration;

    /// The Swift test makes two ids with the platform's generator and checks each is a lowercase
    /// UUID the desk keeps and that they differ. Making ids stays on each platform; what this core
    /// answers is that a lowercase UUID, as both platforms write one, is a client id.
    #[test]
    fn a_new_client_id_is_a_random_uuid_the_server_accepts() {
        let id = "3f2504e0-4f89-11d3-9a0c-0305e82c3301";

        assert_eq!(valid_client_id(Some(id)), Some(id));
        assert_eq!(
            valid_client_id(Some(&id.to_uppercase())),
            Some(&*id.to_uppercase())
        );
    }

    #[test]
    fn an_id_of_up_to_64_letters_digits_and_hyphens_is_valid() {
        let longest = format!("{}z", "A-9".repeat(21));
        assert_eq!(
            longest.chars().count(),
            Registration::MAXIMUM_CLIENT_ID_LENGTH
        );

        for id in ["a", "phone-1", "ABC-def-123", &longest] {
            assert_eq!(valid_client_id(Some(id)), Some(id));
        }
    }

    #[test]
    fn anything_else_is_not_a_client_id() {
        let too_long = "a".repeat(Registration::MAXIMUM_CLIENT_ID_LENGTH + 1);
        let invalid = [
            "",
            "has space",
            "under_score",
            "slash/id",
            "acentuación",
            too_long.as_str(),
        ];

        for id in invalid {
            assert_eq!(
                valid_client_id(Some(id)),
                None,
                "'{id}' must not be a client id"
            );
        }
        assert_eq!(valid_client_id(None), None);
    }

    #[test]
    fn a_stored_valid_id_is_kept_and_nothing_is_saved() {
        let mut saved = Vec::new();

        let id = resolve_client_id(
            Some("phone-1"),
            || "unused".to_owned(),
            |id| saved.push(id.to_owned()),
        );

        assert_eq!(id, "phone-1");
        assert!(saved.is_empty(), "nothing new is stored");
    }

    #[test]
    fn with_no_stored_id_a_new_one_is_generated_and_saved() {
        let mut saved = Vec::new();

        let id = resolve_client_id(
            None,
            || "fresh-id".to_owned(),
            |id| saved.push(id.to_owned()),
        );

        assert_eq!(id, "fresh-id");
        assert_eq!(saved, ["fresh-id"]);
    }

    #[test]
    fn a_corrupted_stored_id_is_replaced_and_saved() {
        let mut saved = Vec::new();

        let id = resolve_client_id(
            Some("not valid!"),
            || "fresh-id".to_owned(),
            |id| saved.push(id.to_owned()),
        );

        assert_eq!(id, "fresh-id");
        assert_eq!(saved, ["fresh-id"]);
    }
}

#[cfg(test)]
mod tests {
    //! [`choose_client_id`], the form both apps call: the same rule as [`resolve_client_id`].

    use super::*;

    #[test]
    fn a_stored_valid_id_is_chosen_with_nothing_to_save() {
        assert_eq!(
            choose_client_id(Some("phone-1"), "fresh-id".to_owned()),
            ClientIdChoice {
                client_id: "phone-1".to_owned(),
                should_save: false,
            }
        );
    }

    #[test]
    fn with_no_stored_id_the_generated_one_is_chosen_and_saved() {
        assert_eq!(
            choose_client_id(None, "fresh-id".to_owned()),
            ClientIdChoice {
                client_id: "fresh-id".to_owned(),
                should_save: true,
            }
        );
    }

    #[test]
    fn a_corrupted_stored_id_is_replaced_by_the_generated_one_and_saved() {
        assert_eq!(
            choose_client_id(Some("not valid!"), "fresh-id".to_owned()),
            ClientIdChoice {
                client_id: "fresh-id".to_owned(),
                should_save: true,
            }
        );
    }

    /// Whatever is stored, the choice is the id [`resolve_client_id`] returns and saves.
    #[test]
    fn the_choice_is_what_resolving_returns_and_saves() {
        let too_long = "a".repeat(65);
        let stored = [
            None,
            Some(""),
            Some("phone-1"),
            Some("3f2504e0-4f89-11d3-9a0c-0305e82c3301"),
            Some("has space"),
            Some("acentuación"),
            Some(too_long.as_str()),
        ];

        for stored in stored {
            let mut saved = Vec::new();
            let resolved = resolve_client_id(
                stored,
                || "fresh-id".to_owned(),
                |id| saved.push(id.to_owned()),
            );

            let choice = choose_client_id(stored, "fresh-id".to_owned());

            assert_eq!(choice.client_id, resolved, "{stored:?}");
            assert_eq!(
                choice.should_save,
                saved == [resolved.clone()],
                "{stored:?}"
            );
        }
    }
}
