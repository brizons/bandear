//! The master strip (`master-strip.js`): the output meters, the level, the phone's level cap, the
//! engineer's limit, the line saying the lower of the two wins, Mute, the master limiter and the
//! effect returns.

use bandear_desktop_console::mix::{EditValue, MixField};
use bandear_desktop_console::slider::SliderEvent;
use bandear_desktop_kit::a11y::seen;
use bandear_desktop_kit::buttons::{ToggleButton, ToggleKind};
use bandear_desktop_kit::gpui::{
    Div, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::meters::{LevelMeter, ReductionMeter};
use bandear_desktop_kit::slider::{FaderTone, fader, parameter};
use bandear_desktop_kit::theme::{FONT_MONO, color, colors, radius, text, weight};
use bandear_desktop_kit::tooltip::tooltip;

use crate::limiter::{LIMITER_REDUCTION_SCALE_DB, LimiterControl, LimiterField, SLIDER_STEPS};
use crate::model::{Body, MasterControl, Model, master_edit};
use crate::view::atoms::caption;

/// A field of the limiter or the returns: its word and value on one line, its slider under them.
fn field(name: &'static str, value: &str, slider: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .w_full()
        .text_size(px(text::XXS))
        .text_color(color(colors::TEXT_SECONDARY))
        .child(
            div()
                .flex()
                .justify_between()
                .gap(px(6.0))
                .child(seen(name))
                .child(
                    div()
                        .font_family(FONT_MONO)
                        .text_color(color(colors::TEXT))
                        .child(seen(value.to_owned())),
                ),
        )
        .child(slider)
}

/// A group under the master's Mute, with a rule above it.
fn group(id: &'static str, label: &'static str) -> bandear_desktop_kit::gpui::Stateful<Div> {
    div()
        .id(id)
        .role(Role::Group)
        .aria_label(label)
        .flex()
        .flex_col()
        .gap(px(6.0))
        .w_full()
        .pt(px(8.0))
        .border_t_1()
        .border_color(color(colors::BORDER))
}

/// A note in small type, centred.
fn note(words: &str) -> Div {
    div()
        .w_full()
        .text_center()
        .text_size(px(text::XXS))
        .line_height(px(text::XXS * 1.35))
        .text_color(color(colors::TEXT_SECONDARY))
        .child(words.to_owned())
}

/// A slider of the limiter, which tells the model which key moves it and that a press is never a
/// key, so a key step always moves at least one unit.
fn limiter_slider(
    model: &Model,
    field: LimiterField,
    position: i32,
    label: &'static str,
    value: &str,
    description: &str,
) -> impl IntoElement {
    let id = match field {
        LimiterField::Ceiling => "limiter-ceiling",
        LimiterField::Release => "limiter-release",
    };
    let (keys, up, pointer, edit) = (model.clone(), model.clone(), model.clone(), model.clone());
    div()
        .id(SharedString::from(format!("{id}-keys")))
        .w_full()
        .aria_description(SharedString::from(description.to_owned()))
        .capture_key_down(move |event, _, _| keys.limiter_key_down(field, &event.keystroke.key))
        .on_key_up(move |_, _, _| up.limiter_key_up())
        .capture_any_mouse_down(move |_, _, _| pointer.limiter_pointer_down())
        .child(
            // A native range has no reset of its own: a double click stays where it is.
            parameter(
                id,
                SLIDER_STEPS,
                f64::from(position),
                position,
                label,
                value.to_owned(),
            )
            .on_input(move |position, _, _| {
                edit.limiter_edit(LimiterControl::Slider(field, f64::from(position)))
            }),
        )
}

pub fn master_strip(body: &Body, model: &Model) -> impl IntoElement {
    let master = &body.master;
    let limiter = &body.limiter;
    let edit = |control: MasterControl| {
        let model = model.clone();
        move |event: SliderEvent| {
            if let Some((field, value)) = master_edit(control, event) {
                model.edit_master(field, value);
            }
        }
    };
    let (level_input, cap_input, limit_change) = (
        edit(MasterControl::Level),
        edit(MasterControl::Cap),
        edit(MasterControl::Limit),
    );
    let (mute, toggle) = (model.clone(), model.clone());
    let ceiling_note = master.ceiling_note.clone().unwrap_or_default();
    let level = fader(
        "master-level",
        f64::from(master.volume),
        "Master level",
        if master.muted {
            FaderTone::Muted
        } else {
            FaderTone::Accent
        },
        false,
    )
    .on_input(move |value, _, _| level_input(SliderEvent::Input(value)));
    let levels = div()
        .flex()
        .flex_1()
        .gap(px(8.0))
        .items_stretch()
        .min_h(px(200.0))
        .child(LevelMeter::new(
            "output-left",
            f64::from(body.master_levels.0),
            "Output left",
        ))
        .child(LevelMeter::new(
            "output-right",
            f64::from(body.master_levels.1),
            "Output right",
        ))
        .child(fader_column(
            "level-column",
            "LEVEL",
            None,
            level.into_any_element(),
        ))
        .when_some(master.cap, |row, cap| {
            row.child(fader_column(
                "cap-column",
                "CAP",
                Some("The musician's own level cap; they can change it on their phone too"),
                div()
                    .id("cap-described")
                    .h_full()
                    .aria_description(SharedString::from(ceiling_note.clone()))
                    .child(
                        fader(
                            "master-cap",
                            f64::from(cap),
                            "Phone level cap",
                            FaderTone::Default,
                            true,
                        )
                        .on_input(move |value, _, _| cap_input(SliderEvent::Input(value))),
                    )
                    .into_any_element(),
            ))
        })
        .when_some(master.limit, |row, limit| {
            row.child(fader_column(
                "limit-column",
                "LIMIT",
                Some("Your limit for this musician; their phone cannot change it"),
                div()
                    .id("limit-described")
                    .h_full()
                    .aria_description(SharedString::from(ceiling_note.clone()))
                    .child(
                        fader(
                            "master-limit",
                            f64::from(limit),
                            "Engineer limit",
                            FaderTone::Default,
                            true,
                        )
                        .on_change(move |value, _, _| limit_change(SliderEvent::Change(value))),
                    )
                    .into_any_element(),
            ))
        });
    let returns = body.returns.rows.iter().fold(
        group("returns", "Effect returns").child(
            div()
                .text_size(px(text::XXS))
                .text_color(color(colors::TEXT_SECONDARY))
                .child(seen("Effect returns")),
        ),
        |group, row| {
            let (model, effect) = (model.clone(), row.effect);
            let id: &'static str = if row.name == "Reverb" {
                "return-reverb"
            } else {
                "return-delay"
            };
            group.child(field(
                row.name,
                &row.text,
                parameter(
                    id,
                    100,
                    f64::from(row.level),
                    i32::from(row.level),
                    row.label.clone(),
                    row.text.clone(),
                )
                .on_input(move |level, _, _| model.return_edit(effect, level.clamp(0, 100) as u8)),
            ))
        },
    );
    div()
        .id("master")
        .role(Role::Group)
        .aria_label("Master")
        .flex()
        .flex_col()
        .flex_none()
        .items_center()
        .gap(px(7.0))
        .w(px(188.0))
        // A short window cuts the strip's foot (the web strip's did): it scrolls on its own, so the
        // limiter and the returns stay within reach.
        .min_h_0()
        .overflow_y_scroll()
        .pt(px(10.0))
        .px(px(6.0))
        .pb(px(8.0))
        .rounded(px(radius::LG))
        .border_1()
        .border_color(color(colors::ACCENT))
        .bg(color(colors::ACCENT_BG))
        .child(caption("MASTER"))
        .child(levels)
        .child(
            div()
                .w_full()
                .text_center()
                .font_family(FONT_MONO)
                .text_size(px(text::SM))
                .font_weight(weight::BOLD)
                .child(master.readout.clone()),
        )
        .when_some(master.ceiling_note.as_deref(), |strip, words| {
            strip.child(note(words))
        })
        .child(
            ToggleButton::new(
                "master-mute",
                "Mute",
                "Mute master",
                ToggleKind::Mute,
                master.muted,
            )
            .wide()
            .on_toggle(move |pressed, _, _| {
                mute.edit_master(MixField::Muted, EditValue::Flag(pressed))
            }),
        )
        .child(
            group("limiter", "Limiter")
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .child(
                            ToggleButton::new(
                                "limiter-on",
                                "Limiter",
                                "Limiter on",
                                ToggleKind::Plain,
                                limiter.settings.enabled,
                            )
                            .wide()
                            .on_toggle(move |pressed, _, _| {
                                toggle.limiter_edit(LimiterControl::Enabled(pressed))
                            }),
                        )
                        .child(
                            div()
                                .text_size(px(text::XXS))
                                .text_color(color(colors::TEXT_SECONDARY))
                                .child(if limiter.settings.enabled {
                                    "On"
                                } else {
                                    "Off"
                                }),
                        ),
                )
                .child(field(
                    "Ceiling",
                    &limiter.ceiling,
                    limiter_slider(
                        model,
                        LimiterField::Ceiling,
                        limiter.ceiling_position,
                        "Limiter ceiling",
                        &limiter.ceiling,
                        &limiter.note,
                    ),
                ))
                .child(field(
                    "Release",
                    &limiter.release,
                    limiter_slider(
                        model,
                        LimiterField::Release,
                        limiter.release_position,
                        "Limiter release",
                        &limiter.release,
                        "",
                    ),
                ))
                .child(ReductionMeter::new(
                    "limiter-reduction",
                    body.reduction_db,
                    LIMITER_REDUCTION_SCALE_DB,
                    "Limiter gain reduction",
                ))
                .child(note(&limiter.note)),
        )
        .child(returns)
}

/// A fader column of the levels row: the fader, its caption under it, and its tooltip.
fn fader_column(
    id: &'static str,
    caption_text: &'static str,
    title: Option<&'static str>,
    fader: bandear_desktop_kit::gpui::AnyElement,
) -> impl IntoElement {
    let column = div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.0))
        .child(div().flex().flex_1().child(fader))
        .child(caption(caption_text));
    match title {
        Some(title) => column
            .tooltip(move |window, cx| tooltip(title.into(), window, cx))
            .into_any_element(),
        None => column.into_any_element(),
    }
}
