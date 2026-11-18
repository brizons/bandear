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

/// The `stored` id when the desk keeps it. Otherwise (the first launch, or a value damaged on the
/// device) a new one from `generate`, handed to `save` and returned (Android's `resolveClientId`).
pub fn resolve_client_id(
    stored: Option<&str>,
    generate: impl FnOnce() -> String,
    save: impl FnOnce(&str),
) -> String {
    if let Some(kept) = valid_client_id(stored) {
        return kept.to_owned();
    }
    let created = generate();
    save(&created);
    created
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
