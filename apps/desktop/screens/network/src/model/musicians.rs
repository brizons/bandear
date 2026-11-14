//! The musicians table's rows (`musicianRows` in `network-view.js`), the address a phone is added
//! by (`validateAddress`), and each row's Buffer (`playback-buffer.js`) and Redundancy
//! (`redundancy.js`) cells with the words for the desk's answers. Pure: no console, no window.
//!
//! - A name the phone announced is the phone's, so it is not editable; a local label is offered only
//!   while the phone announced none, and the desk keeps it for the phone when it has a client id
//!   (the label follows it to any IP) and for the address otherwise (N4).
//! - A phone takes its playback buffer from the desk only when it announced `playback.buffer`:
//!   Auto, or a fixed length in whole milliseconds from 5 to 500 (N9, N10).
//! - Redundancy sends every packet twice, the copy right after it or 1-4 blocks later; a delayed
//!   copy needs a phone that announced `redundancy.hold`, and only one that announced
//!   `redundancy.recovered` reports what a copy recovered (N14, N15).

use bandear_desktop_console::format::{
    LinkState, format_percent, link_state, musician_label, sentence_with_reason, to_fixed,
};
use bandear_desktop_console::state::ConsoleState;
use bandear_desktop_console::types::dto::{BufferSetDto, RedundancySetDto};
use bandear_desktop_console::wire::{CaptureFormat, Musician};

/// What the musician's name field says on hover, for a name the phone announced.
pub const CLIENT_NAME_TITLE: &str =
    "Announced by the musician in their own app; the name is set on their phone, not on this desk";
/// The same, for a local label kept for the phone.
pub const LOCAL_LABEL_FOR_PHONE: &str = "Local label for this desk, kept for this phone even when its address changes, and shown only while the musician has not announced a name. Enter saves, Escape reverts";
/// The same, for a local label kept for the address.
pub const LOCAL_LABEL_FOR_ADDRESS: &str = "Local label for this desk, kept for this address, and shown only while the musician has not announced a name. Enter saves, Escape reverts";

/// The key the desk names a musician by: their phone's id, or their address.
pub fn musician_key(musician: &Musician) -> String {
    match musician.client_id.as_deref().filter(|id| !id.is_empty()) {
        Some(id) => format!("client:{id}"),
        None => format!("address:{}", musician.address),
    }
}

/// A whole number as `toLocaleString("en-US")` writes it: "1,200".
pub fn count(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// Whom a local label is kept for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelFollows {
    /// The phone: it has a client id, so the label follows it to any address.
    Phone,
    /// The address.
    Address,
}

/// One row of the musicians table.
#[derive(Clone, Debug, PartialEq)]
pub struct MusicianRow {
    pub address: String,
    pub client_id: Option<String>,
    /// The name shown, "" for none.
    pub name: String,
    /// False when the phone announced the name: the field is read-only.
    pub editable: bool,
    pub name_is_local: bool,
    pub label_follows: LabelFollows,
    pub connected: bool,
    pub link: LinkState,
    /// "85% · live".
    pub mix: String,
    pub sent: String,
    pub discarded: String,
    /// The share of packets this computer discarded for the phone: "2.44%".
    pub loss: String,
    pub loss_high: bool,
    pub buffer: BufferView,
    pub redundancy: RedundancyView,
}

impl MusicianRow {
    /// What the row's controls call the musician: their name, else the address.
    pub fn label(&self) -> &str {
        if self.name.is_empty() {
            &self.address
        } else {
            &self.name
        }
    }

    /// The name field's title: whose name it is, and for a local label whom it is kept for.
    pub fn name_title(&self) -> &'static str {
        match (self.editable, self.label_follows) {
            (false, _) => CLIENT_NAME_TITLE,
            (true, LabelFollows::Phone) => LOCAL_LABEL_FOR_PHONE,
            (true, LabelFollows::Address) => LOCAL_LABEL_FOR_ADDRESS,
        }
    }
}

/// One row per listed musician, their delays priced in the block of the engine running.
pub fn musician_rows(state: &ConsoleState) -> Vec<MusicianRow> {
    let block_ms = block_ms_of(
        state
            .engine
            .status
            .as_ref()
            .map(|status| &status.capture_format),
    );
    state
        .musicians
        .iter()
        .map(|musician| {
            let (sent, discarded) = (musician.packets_sent, musician.packets_discarded);
            let total = sent + discarded;
            let loss = if total == 0 {
                0.0
            } else {
                discarded as f64 / total as f64 * 100.0
            };
            let client_id = musician.client_id.clone().filter(|id| !id.is_empty());
            MusicianRow {
                address: musician.address.clone(),
                label_follows: if client_id.is_some() {
                    LabelFollows::Phone
                } else {
                    LabelFollows::Address
                },
                client_id,
                name: musician.name.clone().unwrap_or_default(),
                editable: musician
                    .client_name
                    .as_deref()
                    .is_none_or(|name| name.is_empty()),
                name_is_local: musician.name_is_local,
                connected: musician.control_connected,
                link: link_state(musician),
                mix: format!(
                    "{} \u{b7} {}",
                    format_percent(f64::from(musician.mix.volume_percent)),
                    if musician.mix.muted { "muted" } else { "live" }
                ),
                sent: count(sent),
                discarded: count(discarded),
                loss: format!("{}%", to_fixed(loss, 2)),
                loss_high: loss > 0.0,
                buffer: buffer_view(musician),
                redundancy: redundancy_view(musician, block_ms),
            }
        })
        .collect()
}

/// Why `text` is not an address to send a phone's audio to, or the address trimmed: obvious
/// mistakes are caught here (nothing typed, spaces, an IPv4 part over 255, a port out of range);
/// anything else, such as a name that does not resolve, is the engine's to refuse (N31).
pub fn validate_address(text: &str) -> Result<String, String> {
    let address = text.trim();
    if address.is_empty() {
        return Err("Type the phone's address, for example 192.168.1.50.".to_owned());
    }
    if address.chars().any(char::is_whitespace) {
        return Err(format!("An address has no spaces: {address}"));
    }
    let ipv4 = ipv4_parts(address);
    let only_ip_characters = address
        .chars()
        .all(|character| character.is_ascii_digit() || character == '.' || character == ':');
    if only_ip_characters && ipv4.is_none() {
        return Err(format!(
            "{address} is not an IP address such as 192.168.1.50."
        ));
    }
    if ipv4.is_some_and(|parts| parts.iter().any(|part| *part > 255)) {
        return Err(format!(
            "{address} is not an IP address: each part goes up to 255."
        ));
    }
    let port = address
        .rsplit_once(':')
        .map(|(_, port)| port)
        .filter(|port| !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()));
    // A bare IPv6 address has colons of its own and no port to check.
    let bare = !address.starts_with('[') && address.split(':').count() > 2;
    if let Some(port) = port
        && !bare
        && !port
            .parse::<u64>()
            .is_ok_and(|number| (1..=65535).contains(&number))
    {
        return Err(format!("The port must be from 1 to 65535, not {port}."));
    }
    Ok(address.to_owned())
}

/// The four parts of `a.b.c.d[:port]` (each of 1-3 digits), or `None` for anything else.
fn ipv4_parts(address: &str) -> Option<[u32; 4]> {
    let host = match address.split_once(':') {
        Some((host, port)) => {
            if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            host
        }
        None => address,
    };
    let mut parts = [0; 4];
    let mut pieces = host.split('.');
    for part in &mut parts {
        let piece = pieces.next()?;
        if piece.is_empty() || piece.len() > 3 || !piece.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        *part = piece.parse().ok()?;
    }
    pieces.next().is_none().then_some(parts)
}

// ---- The playback buffer ------------------------------------------------------------------------

/// The shortest and longest fixed buffer the desk sets, in milliseconds.
pub const MIN_BUFFER_MS: u32 = 5;
pub const MAX_BUFFER_MS: u32 = 500;
/// The length Fixed starts at for a musician who had none.
pub const DEFAULT_FIXED_MS: u32 = 40;
/// What a row whose phone takes no buffer from the desk (or no delayed copy) says.
pub const NEEDS_NEWER_APP: &str = "needs a newer app";

/// Auto (the phone adapts its own cushion) or Fixed (the length set here).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferMode {
    Auto,
    Fixed,
}

impl BufferMode {
    /// The mode's name, as the desk names it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Fixed => "fixed",
        }
    }
}

/// A row's Buffer cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferView {
    pub key: String,
    pub supported: bool,
    pub mode: BufferMode,
    /// The fixed length; `None` for Auto.
    pub ms: Option<u16>,
    /// The cushion the phone reports it plays: "plays 42.7 ms".
    pub playing: Option<String>,
    /// "needs a newer app" for a phone that takes no buffer.
    pub note: Option<&'static str>,
}

/// The Buffer cell of `musician`'s row.
pub fn buffer_view(musician: &Musician) -> BufferView {
    let fixed = musician.buffer.mode == "fixed" && musician.buffer.ms.is_some();
    let cushion = musician
        .link_latest
        .as_ref()
        .and_then(|sample| sample.cushion_ms)
        .filter(|cushion| cushion.is_finite());
    BufferView {
        key: musician_key(musician),
        supported: musician.buffer_supported,
        mode: if fixed {
            BufferMode::Fixed
        } else {
            BufferMode::Auto
        },
        ms: musician.buffer.ms.filter(|_| fixed),
        playing: cushion.map(|cushion| format!("plays {} ms", to_fixed(cushion, 1))),
        note: (!musician.buffer_supported).then_some(NEEDS_NEWER_APP),
    }
}

/// A buffer the desk can be asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferRequest {
    pub mode: BufferMode,
    pub ms: Option<u32>,
}

/// What the engineer's choice asks for (`mode` "auto" or "fixed", the length as typed), or why it
/// cannot be sent: a fixed length is whole milliseconds from 5 to 500.
pub fn buffer_request(mode: &str, ms_text: &str) -> Result<BufferRequest, String> {
    match mode {
        "auto" => {
            return Ok(BufferRequest {
                mode: BufferMode::Auto,
                ms: None,
            });
        }
        "fixed" => {}
        other => return Err(format!("The buffer is Auto or Fixed, not {other}.")),
    }
    let text = ms_text.trim();
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "Type the buffer in whole milliseconds, from {MIN_BUFFER_MS} to {MAX_BUFFER_MS}."
        ));
    }
    match text.parse::<u32>() {
        Ok(ms) if (MIN_BUFFER_MS..=MAX_BUFFER_MS).contains(&ms) => Ok(BufferRequest {
            mode: BufferMode::Fixed,
            ms: Some(ms),
        }),
        parsed => {
            // As JavaScript's `Number`: leading zeros go, a length too long to read stays as typed.
            let shown = parsed.map_or_else(|_| text.to_owned(), |ms| ms.to_string());
            Err(format!(
                "A fixed buffer is from {MIN_BUFFER_MS} to {MAX_BUFFER_MS} ms, not {shown} ms."
            ))
        }
    }
}

/// What a row's outcome line says: done (`ok`) or not, in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub ok: bool,
    pub message: String,
}

impl Said {
    pub fn new(ok: bool, message: impl Into<String>) -> Self {
        Self {
            ok,
            message: message.into(),
        }
    }
}

/// " It was not saved for the next start: E." after an outcome whose save failed.
fn save_failure(save_error: Option<&str>) -> String {
    match save_error {
        Some(error) => format!(
            " {}",
            sentence_with_reason("It was not saved for the next start", Some(error))
        ),
        None => String::new(),
    }
}

/// The desk's answer to a buffer change, for `name`'s row (`formatBufferSet`): sent and kept for a
/// phone that is away are done; a phone that needs a newer app and a refused request are not.
pub fn format_buffer_set(answer: &BufferSetDto, name: &str) -> Said {
    let fixed = answer
        .buffer
        .filter(|buffer| buffer.mode == "fixed")
        .and_then(|buffer| buffer.ms);
    let described = match fixed {
        Some(ms) => format!("a fixed {ms} ms buffer"),
        None => "Auto".to_owned(),
    };
    let message = match answer.outcome {
        "sent" if fixed.is_some() => format!("Sent {name}'s phone {described}."),
        "sent" => format!("{name}'s phone is back on Auto."),
        "notConnected" => {
            format!("Kept for {name}: their phone gets {described} when it is connected again.")
        }
        "unsupported" => {
            return Said::new(
                false,
                format!("{name}'s phone {NEEDS_NEWER_APP} to take its buffer from the desk."),
            );
        }
        _ => {
            let message = if answer.message.is_empty() {
                "The buffer was not set.".to_owned()
            } else {
                answer.message.clone()
            };
            return Said::new(false, message);
        }
    };
    Said::new(true, message + &save_failure(answer.save_error.as_deref()))
}

/// A Buffer choice sent and not answered yet: the mode, and the length as typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferAsked {
    pub mode: String,
    pub ms: Option<String>,
}

/// True once the row reports the choice `asked`, compared by value (a typed "040" is the 40
/// reported); a new answer for the row settles it too (the model clears it then).
pub fn buffer_choice_settled(asked: &BufferAsked, buffer: &BufferView) -> bool {
    if buffer.mode.name() != asked.mode {
        return false;
    }
    if buffer.mode == BufferMode::Auto {
        return true;
    }
    let typed = asked.ms.as_deref().unwrap_or("").trim();
    !typed.is_empty()
        && typed.bytes().all(|byte| byte.is_ascii_digit())
        && typed.parse::<u64>().ok() == buffer.ms.map(u64::from)
}

/// The name the desk shows for the musician `key` names among `musicians`, else the key itself.
pub fn name_for_key(musicians: &[Musician], key: &str) -> String {
    musicians
        .iter()
        .find(|musician| musician_key(musician) == key)
        .map_or_else(
            || key.to_owned(),
            |musician| musician_label(musician.name.as_deref(), &musician.address),
        )
}

// ---- Redundancy ---------------------------------------------------------------------------------

/// The delay of the copies, in blocks, when none is named: right after their packet.
pub const DEFAULT_DELAY_BLOCKS: u8 = 0;
/// The longest delay of a copy, in blocks.
pub const MAX_DELAY_BLOCKS: u8 = 4;
/// What a row whose phone does not report recoveries says.
pub const NOT_REPORTED: &str = "not reported by this phone";
/// One engine block at the default buffer, 256 frames at 48 kHz: about 5.3 ms.
pub const DEFAULT_BLOCK_MS: f64 = 256.0 * 1000.0 / 48_000.0;

/// How long one engine block lasts for `format`: a fixed buffer of n frames at the device's rate
/// lasts n / rate (the engine sends one block per callback), anything else the default block.
pub fn block_ms_of(format: Option<&CaptureFormat>) -> f64 {
    let Some(format) = format else {
        return DEFAULT_BLOCK_MS;
    };
    let frames = format
        .buffer_size
        .strip_prefix("Fixed(")
        .and_then(|rest| rest.strip_suffix(')'))
        .filter(|frames| !frames.is_empty() && frames.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|frames| frames.parse::<f64>().ok());
    match frames {
        Some(frames) if format.sample_rate > 0 => frames * 1000.0 / f64::from(format.sample_rate),
        _ => DEFAULT_BLOCK_MS,
    }
}

fn blocks(delay: u8) -> String {
    format!("{delay} {}", if delay == 1 { "block" } else { "blocks" })
}

fn milliseconds(value: f64) -> String {
    format!("{} ms", to_fixed(value, 1))
}

/// "immediate", or "2 blocks later (+10.7 ms)".
pub fn delay_label(delay: u8, block_ms: f64) -> String {
    if delay == 0 {
        return "immediate".to_owned();
    }
    format!(
        "{} later (+{})",
        blocks(delay),
        milliseconds(f64::from(delay) * block_ms)
    )
}

/// A Copy choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelayChoice {
    pub value: u8,
    pub label: String,
    pub disabled: bool,
}

/// A row's Redundancy cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedundancyView {
    pub key: String,
    pub on: bool,
    /// The copies' delay; `None` while off.
    pub delay_blocks: Option<u8>,
    pub hold_supported: bool,
    /// Immediate first; a delay is disabled for a phone that cannot hold packets back for it.
    pub delays: Vec<DelayChoice>,
    pub hold_note: Option<String>,
    /// What it costs in words, latency included.
    pub bandwidth: String,
    pub recovered: Option<String>,
    pub duplicates: Option<String>,
}

/// The Redundancy cell of `musician`'s row, its delays priced in blocks of `block_ms`.
pub fn redundancy_view(musician: &Musician, block_ms: f64) -> RedundancyView {
    let on = musician.redundancy.on;
    let delay = on.then(|| {
        musician
            .redundancy
            .delay_blocks
            .unwrap_or(DEFAULT_DELAY_BLOCKS)
    });
    let hold_supported = musician.redundancy_hold_supported;
    let recovered = delay.map(|delay| {
        if !musician.redundancy_supported {
            format!("Recovered packets {NOT_REPORTED}")
        } else if delay == 0 {
            "Recovered packets are counted for delayed copies only".to_owned()
        } else {
            let packets = musician.recovered;
            format!(
                "{} {} recovered",
                count(packets),
                if packets == 1 { "packet" } else { "packets" }
            )
        }
    });
    let bandwidth = match delay {
        None => "Sends once; on doubles this musician's bandwidth".to_owned(),
        Some(0) => "2\u{d7} bandwidth, no added latency".to_owned(),
        Some(delay) => format!(
            "2\u{d7} bandwidth, +{} latency on this phone",
            milliseconds(f64::from(delay) * block_ms)
        ),
    };
    let delays = (0..=MAX_DELAY_BLOCKS)
        .map(|value| {
            let label = format!("Copy: {}", delay_label(value, block_ms));
            let disabled = value > 0 && !hold_supported;
            DelayChoice {
                value,
                label: if disabled {
                    format!("{label}, {NEEDS_NEWER_APP}")
                } else {
                    label
                },
                disabled,
            }
        })
        .collect();
    RedundancyView {
        key: musician_key(musician),
        on,
        delay_blocks: delay,
        hold_supported,
        delays,
        hold_note: (!hold_supported).then(|| format!("Delayed copies: {NEEDS_NEWER_APP}")),
        bandwidth,
        recovered,
        duplicates: on.then(|| format!("{} copies sent", count(musician.duplicates_sent))),
    }
}

/// The desk's answer to a redundancy change, for `name`'s row (`formatRedundancySet`), with what
/// it does to their bandwidth and latency in blocks of `block_ms`.
pub fn format_redundancy_set(answer: &RedundancySetDto, name: &str, block_ms: f64) -> Said {
    if answer.outcome == "unsupported" {
        return Said::new(
            false,
            format!(
                "{name}'s phone {NEEDS_NEWER_APP} for a delayed copy; an immediate copy works with any app."
            ),
        );
    }
    if answer.outcome != "set" {
        let message = if answer.message.is_empty() {
            "Redundancy was not set.".to_owned()
        } else {
            answer.message.clone()
        };
        return Said::new(false, message);
    }
    let factor = answer.bandwidth_factor;
    let message = match answer.redundancy.filter(|redundancy| redundancy.on) {
        None => format!(
            "{name}'s audio is sent once again: their bandwidth is back to normal (\u{d7}{factor})."
        ),
        Some(redundancy) => match redundancy.delay_blocks.unwrap_or(DEFAULT_DELAY_BLOCKS) {
            0 => format!(
                "{name}'s audio is now sent twice, each copy right after its original: their bandwidth doubles (\u{d7}{factor}), with no added latency."
            ),
            delay => format!(
                "{name}'s audio is now sent twice, each copy {} later: their bandwidth doubles (\u{d7}{factor}) and their phone plays {} later.",
                blocks(delay),
                milliseconds(f64::from(delay) * block_ms)
            ),
        },
    };
    Said::new(true, message + &save_failure(answer.save_error.as_deref()))
}

/// A Redundancy choice sent and not answered yet; the delay `None` for the default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RedundancyAsked {
    pub on: bool,
    pub delay_blocks: Option<u8>,
}

/// True once the row reports the choice `asked` (on with that delay, or off); a new answer for the
/// row settles it too (the model clears it then).
pub fn redundancy_choice_settled(asked: &RedundancyAsked, cell: &RedundancyView) -> bool {
    if cell.on != asked.on {
        return false;
    }
    let delay = asked.delay_blocks.unwrap_or(DEFAULT_DELAY_BLOCKS);
    !asked.on || cell.delay_blocks == Some(delay)
}

#[cfg(test)]
#[path = "musicians_tests.rs"]
mod tests;
