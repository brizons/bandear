//! The Audio input card (`audio-setup.js`): the recording lock, the driver with the ASIO caution,
//! the input device with its errors and the switch question, Start on this driver, the buffer, the
//! Local mix out with its lines, the engine's numbers and the hint (E1-E6).

use bandear_desktop_console::engine_config::HOSTS;
use bandear_desktop_kit::a11y::seen;
use bandear_desktop_kit::buttons::{Button, ButtonKind};
use bandear_desktop_kit::forms::{Choice, SelectEvent, select};
use bandear_desktop_kit::gpui::{
    Context, Div, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::theme::{FONT_MONO, color, colors, radius, text, weight};

use super::parts::{Tone, card, choices, focus_button, label, line, said_line, sync_select};
use super::{AudioFields, NetworkScreen};
use crate::model::audio::{BUFFER_OPTIONS, OutputKind, host_options};

/// The caution while an ASIO choice waits.
const ASIO_CAUTION: &str = "ASIO loads a kernel-mode driver: an unstable one can restart the whole machine, so it is never chosen for you. Pick the driver deliberately, preferring your interface's own over a built-in one; nothing starts until you press Start on this driver.";

/// The hint under the numbers.
const HINT: &str = "The driver, the buffer and, outside WASAPI, the device restart the capture engine, the way a DAW does when the driver changes: the phones reconnect on their own. The buffer only takes effect on ASIO, where the driver honours it; WASAPI shared mode pins the period to the device's own (10 ms). The sample rate is 48 kHz by design: it is the format on the wire the phones receive, not a free choice.";

/// The card's selects: each choice goes to the model.
pub(super) fn subscribe(
    audio: &AudioFields,
    window: &mut Window,
    cx: &mut Context<NetworkScreen>,
) -> Vec<Subscription> {
    vec![
        cx.subscribe_in(
            &audio.host,
            window,
            |this, _, event: &SelectEvent<Vec<Choice>>, _, _| {
                let SelectEvent::Confirm(Some(value)) = event else {
                    return;
                };
                if let Some(host) = HOSTS.into_iter().find(|host| host.name() == value.as_ref()) {
                    this.model.choose_host(host);
                }
            },
        ),
        cx.subscribe_in(
            &audio.device,
            window,
            |this, _, event: &SelectEvent<Vec<Choice>>, _, _| {
                let SelectEvent::Confirm(Some(value)) = event else {
                    return;
                };
                this.model.choose_device(value);
            },
        ),
        cx.subscribe_in(
            &audio.buffer,
            window,
            |this, _, event: &SelectEvent<Vec<Choice>>, _, _| {
                let SelectEvent::Confirm(Some(value)) = event else {
                    return;
                };
                if let Ok(frames) = value.parse::<u32>() {
                    this.model.choose_buffer(frames);
                }
            },
        ),
        cx.subscribe_in(
            &audio.output,
            window,
            |this, _, event: &SelectEvent<Vec<Choice>>, _, _| {
                let SelectEvent::Confirm(value) = event;
                this.model
                    .choose_output(value.as_ref().map_or("", |value| value.as_ref()));
            },
        ),
    ]
}

/// A line of the engine's numbers: the name, then the value, wrapping in the room left (a `dt`
/// and its `dd`).
fn readout(name: &'static str, value: String, warning: bool) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(12.0))
        .child(div().flex_none().w(px(150.0)).child(label(name)))
        .child(
            div().flex_1().min_w_0().flex().child(
                div()
                    .min_w_0()
                    .font_family(FONT_MONO)
                    .text_size(px(text::MD))
                    .when(warning, |value| {
                        value
                            .px(px(6.0))
                            .rounded(px(radius::SM))
                            .border_1()
                            .border_color(color(colors::WARN_BORDER))
                            .bg(color(colors::WARN_BG))
                            .text_color(color(colors::ACCENT))
                    })
                    .child(seen(value)),
            ),
        )
}

/// A row of the card's grid: the label in 110 px, the control after it.
fn row(name: &'static str, control: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(div().flex_none().w(px(110.0)).child(label(name)))
        .child(div().flex_1().min_w_0().child(control))
}

impl NetworkScreen {
    pub(super) fn sync_audio(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = &self.view;
        let audio = &mut self.audio;
        let [host, device, buffer, output] = &mut audio.shown;
        let hosts: Vec<Choice> = host_options()
            .into_iter()
            .map(|(host, label)| Choice::new(host.name(), label))
            .collect();
        sync_select(&audio.host, host, hosts, view.audio.host.name(), window, cx);
        sync_select(
            &audio.device,
            device,
            choices(&view.audio.device_choices),
            &view.audio.device,
            window,
            cx,
        );
        let buffers: Vec<Choice> = BUFFER_OPTIONS
            .iter()
            .map(|(frames, label)| Choice::new(frames.to_string(), *label))
            .collect();
        let frames = view
            .audio
            .buffer
            .map(|frames| frames.to_string())
            .unwrap_or_default();
        sync_select(&audio.buffer, buffer, buffers, &frames, window, cx);
        sync_select(
            &audio.output,
            output,
            choices(&view.local_output.options),
            &view.local_output.value,
            window,
            cx,
        );
    }

    pub(super) fn render_audio(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let audio = &self.view.audio;
        let local = &self.view.local_output;
        let confirm = self.model.clone();
        let cancel = self.model.clone();
        let apply = self.model.clone();
        let readouts = &audio.readouts;
        card("audio-setup", "Audio input")
            .when_some(audio.recording_lock, |card, lock| {
                card.child(line("recording-lock", Tone::Caution, lock))
            })
            .child(row(
                "Driver",
                select(&self.audio.host, "Driver").disabled(audio.host_disabled),
            ))
            .when(audio.asio_pending, |card| {
                card.child(line("asio-caution", Tone::Caution, ASIO_CAUTION))
            })
            .child(row(
                "Input device",
                select(&self.audio.device, "Input device")
                    .placeholder("")
                    .disabled(audio.device_disabled),
            ))
            .when_some(self.devices_error(), |card, error| {
                card.child(line("devices-error", Tone::Error, error))
            })
            .when_some(audio.warning.clone(), |card, warning| {
                card.child(
                    div()
                        .id("switch-warning")
                        .role(Role::Alert)
                        .aria_label(warning.clone())
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.0))
                        .py(px(8.0))
                        .px(px(10.0))
                        .rounded(px(radius::MD))
                        .border_1()
                        .border_color(color(colors::WARN_BORDER))
                        .bg(color(colors::WARN_BG))
                        .text_size(px(text::MD))
                        .child(seen(warning))
                        .child(
                            Button::new("switch-anyway", "Switch anyway", ButtonKind::Secondary)
                                .on_press(move |_, _| confirm.confirm_switch()),
                        )
                        .child(
                            Button::new("switch-cancel", "Cancel", ButtonKind::Secondary)
                                .on_press(move |_, _| cancel.cancel_switch()),
                        ),
                )
            })
            .when(audio.asio_pending, |card| {
                card.child(div().flex().child(focus_button(
                    "apply-asio",
                    "Start on this driver",
                    ButtonKind::Primary,
                    &self.audio.apply,
                    !audio.can_apply_asio,
                    move |_, _| apply.apply_asio(),
                )))
            })
            .child(row(
                "Buffer",
                select(&self.audio.buffer, "Buffer")
                    .placeholder("")
                    .disabled(audio.buffer_disabled),
            ))
            .child(row(
                "Local mix out",
                select(
                    &self.audio.output,
                    SharedString::from(format!(
                        "Local mix out. {} {}",
                        local.state.text, local.hint
                    )),
                )
                .disabled(local.disabled),
            ))
            .child(
                div()
                    .text_size(px(text::MD))
                    .text_color(color(if local.state.kind == OutputKind::Running {
                        colors::TEXT
                    } else {
                        colors::TEXT_SECONDARY
                    }))
                    .child(seen(local.state.text.clone())),
            )
            .when_some(local.state.alert.clone(), |card, alert| {
                card.child(line("local-alert", Tone::Error, alert))
            })
            .when_some(local.notice.clone(), |card, notice| {
                card.child(line("local-notice", Tone::Caution, notice))
            })
            .when_some(local.error.clone(), |card, error| {
                card.child(line("local-error", Tone::Error, error))
            })
            .when_some(self.view.output_said.as_ref(), |card, said| {
                card.child(said_line("local-said", said))
            })
            .child(line("local-hint", Tone::Hint, local.hint))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(readout("Capture format", readouts.format.clone(), false))
                    .child(readout("Sample rate", readouts.sample_rate.clone(), false))
                    .child(readout(
                        "Control port",
                        readouts.control_port.clone(),
                        false,
                    ))
                    .child(readout(
                        "Samples received",
                        readouts.samples_received.clone(),
                        false,
                    ))
                    .child(readout(
                        "Packets sent / discarded",
                        readouts.packets.clone(),
                        false,
                    ))
                    .child(readout(
                        "Engine load",
                        audio.load.text.clone(),
                        audio.load.warning,
                    )),
            )
            .child(line("audio-hint", Tone::Hint, HINT))
            .font_weight(weight::REGULAR)
    }

    /// Why the devices could not be listed (`Devices could not be listed: E`).
    fn devices_error(&self) -> Option<String> {
        let state = self.model.console().state();
        state
            .devices
            .error
            .as_ref()
            .map(|error| format!("Devices could not be listed: {error}"))
    }
}
