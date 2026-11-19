//! The channel strips and the group strips (`channel-strip.js`, `group-strip.js`), and the
//! instrument kind icons (`kind-icons.js`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use bandear_desktop_console::mix::{EditValue, MixField};
use bandear_desktop_console::router::{Route, Section};
use bandear_desktop_kit::a11y::seen;
use bandear_desktop_kit::buttons::{ToggleButton, ToggleKind};
use bandear_desktop_kit::gpui::{
    AnyElement, ClickEvent, Div, Entity, Image, ImageFormat, InteractiveElement, IntoElement,
    ParentElement, Role, SharedString, StatefulInteractiveElement, Styled, div, img,
    prelude::FluentBuilder, px,
};
use bandear_desktop_kit::hub::Hub;
use bandear_desktop_kit::meters::LevelMeter;
use bandear_desktop_kit::slider::{FaderTone, fader, pan};
use bandear_desktop_kit::status::GroupChip;
use bandear_desktop_kit::theme::{FONT_MONO, color, colors, radius, text, weight};
use bandear_desktop_kit::tooltip::tooltip;

use crate::mix_view::{
    ChannelStrip, GroupStrip, PROCESSING_LINK_TITLE, ProcessingState, StripGroup, StripTone,
    following_state, processing_link,
};
use crate::model::Model;
use crate::view::atoms::caption;

/// The icons' lines, on a 24 px grid (`kind-icons.js`); an unknown kind draws as "other".
fn kind_paths(kind: &str) -> &'static str {
    match kind {
        "voice" => {
            r#"<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21M9 21h6"/>"#
        }
        "guitar" => {
            r#"<path d="M13.5 10.5 20 4M18 2.5l3.5 3.5"/><path d="M11 9.2a3.6 3.6 0 0 0-4.9 1 3.2 3.2 0 0 1-2.2 1.5 2.8 2.8 0 0 0-1.4 4.7l3.3 3.3a2.8 2.8 0 0 0 4.7-1.4 3.2 3.2 0 0 1 1.5-2.2 3.6 3.6 0 0 0 1-4.9z"/><circle cx="8.6" cy="15.4" r="1.4"/>"#
        }
        "bass" => {
            r#"<path d="M12 12 21 3"/><path d="M19 2.5 21.5 5"/><path d="M10 10.5a3.4 3.4 0 0 0-4.6.8 3 3 0 0 1-2 1.3 2.6 2.6 0 0 0-1.2 4.4l2.8 2.8a2.6 2.6 0 0 0 4.4-1.2 3 3 0 0 1 1.3-2 3.4 3.4 0 0 0 .8-4.6"/><path d="M7 14.5l2.5 2.5"/>"#
        }
        "keys" => {
            r#"<rect x="2.5" y="5" width="19" height="14" rx="2"/><path d="M7.25 19v-5M12 19v-5M16.75 19v-5"/><path d="M6 5v9h2.5V5M10.75 5v9h2.5V5M15.5 5v9H18V5"/>"#
        }
        "drums" => {
            r#"<ellipse cx="12" cy="10" rx="8" ry="3"/><path d="M4 10v6c0 1.7 3.6 3 8 3s8-1.3 8-3v-6"/><path d="M7 3.5l4 5M17 3.5l-4 5"/>"#
        }
        "percussion" => r#"<path d="M8 3h8l-1.5 18h-5z"/><path d="M8.4 8h7.2M9.2 16h5.6"/>"#,
        "horns" => {
            r#"<path d="M2.5 11h8"/><path d="M10.5 7.5 20.5 4v14l-10-3.5z"/><path d="M5 11v3.5M8 11v3.5"/>"#
        }
        "strings" => {
            r#"<path d="M9 3c-2.2 0-3.5 1.6-3.5 3.5 0 1.4.8 2.2.8 3.5S5 12.4 5 14.5C5 18.6 8 21 12 21s7-2.4 7-6.5c0-2.1-1.3-3.2-1.3-4.5s.8-2.1.8-3.5C18.5 4.6 17.2 3 15 3c-1.3 0-2 .9-3 .9S10.3 3 9 3z"/><path d="M12 6v15M9.5 12h5"/>"#
        }
        "playback" => r#"<circle cx="12" cy="12" r="9"/><path d="M10 8.5v7l6-3.5z"/>"#,
        "click" => r#"<path d="M8.5 3h7L19 21H5z"/><path d="M12 17l4.5-11"/><path d="M8 17h8"/>"#,
        _ => r#"<path d="M2.5 12h3.5l2-5 4 10 3-7 1.5 2h5"/>"#,
    }
}

/// The stereo pair's mark: two overlapping rings.
const STEREO_PATHS: &str =
    r#"<circle cx="8.5" cy="12" r="5.5"/><circle cx="15.5" cy="12" r="5.5"/>"#;

thread_local! {
    /// The icons drawn so far, by their lines: each is built once.
    static ICONS: RefCell<HashMap<&'static str, Arc<Image>>> = RefCell::new(HashMap::new());
}

/// An icon of `paths` in the secondary text colour, `size` px. The kit serves no kind icons, so
/// the screen draws its own from their SVG lines (decorative: the kind or "STEREO" is written
/// beside it).
fn icon(paths: &'static str, size: f32) -> impl IntoElement {
    let image = ICONS.with(|icons| {
        icons
            .borrow_mut()
            .entry(paths)
            .or_insert_with(|| {
                let svg = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#{:06x}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">{paths}</svg>"##,
                    colors::TEXT_SECONDARY
                );
                Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes()))
            })
            .clone()
    });
    img(image).flex_none().size(px(size))
}

/// The common column of every strip (`strip-styles.js`).
fn strip_column(width: f32) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .items_center()
        .gap(px(7.0))
        .w(px(width))
        .min_h_0()
        .pt(px(10.0))
        .px(px(6.0))
        .pb(px(8.0))
        .rounded(px(radius::LG))
        .border_1()
        .border_color(color(colors::BORDER))
        .bg(color(colors::SURFACE))
}

/// The "Follows X" line of a mix that follows another, in the accent.
fn follows_line(id: SharedString, words: String, title: Option<String>) -> AnyElement {
    let line = div()
        .id(id)
        .max_w_full()
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .text_color(color(colors::ACCENT))
        .text_size(px(text::XXS))
        .font_weight(weight::BOLD)
        .child(seen(words));
    match title {
        Some(title) => {
            let title = SharedString::from(title);
            line.tooltip(move |window, cx| tooltip(title.clone(), window, cx))
                .into_any_element()
        }
        None => line.into_any_element(),
    }
}

/// A group's chip, with the reason in a tooltip when it names an input the device lacks.
fn chip(id: SharedString, group: &StripGroup) -> AnyElement {
    let chip = GroupChip::new(
        id.clone(),
        group.tone,
        Some(group.name.clone().into()),
        group.invalid,
    );
    if group.invalid {
        div()
            .id(SharedString::from(format!("{id}-why")))
            .tooltip(|window, cx| {
                tooltip(
                    "This group names an input the current device does not have".into(),
                    window,
                    cx,
                )
            })
            .child(chip)
            .into_any_element()
    } else {
        chip.into_any_element()
    }
}

/// The readout under a fader: mono, the muted colour while muted.
fn readout(words: &str, muted: bool) -> Div {
    div()
        .font_family(FONT_MONO)
        .text_size(px(text::SM))
        .font_weight(weight::BOLD)
        .whitespace_nowrap()
        .text_color(color(if muted {
            colors::TEXT_MUTED
        } else {
            colors::TEXT
        }))
        .child(words.to_owned())
}

/// One input's strip (or a stereo pair's), keyed by its first input so it keeps its gestures when
/// strips are reordered, hidden or paired.
pub fn channel_strip(
    strip: &ChannelStrip,
    level: u8,
    model: &Model,
    hub: &Entity<Hub>,
) -> impl IntoElement {
    let key = format!("strip-{}", strip.index);
    let id = |part: &str| SharedString::from(format!("{key}-{part}"));
    let label = strip.label.clone();
    let mirror = following_state(
        strip.following.as_deref(),
        &if strip.stereo {
            format!("{label}, stereo pair")
        } else {
            label.clone()
        },
    );
    let edit = |field: MixField| {
        let (model, sides, stereo) = (model.clone(), strip.sides.clone(), strip.stereo);
        move |value: EditValue| model.edit_channel(&sides, stereo, field, value)
    };
    let link = processing_link(&strip.sides, &label, strip.processing);
    let pan_edit = edit(MixField::Pans);
    let gain_edit = edit(MixField::ChannelGains);
    let mute_edit = edit(MixField::ChannelMuted);
    let solo_edit = edit(MixField::ChannelSolo);
    let lock_edit = edit(MixField::LockedChannels);
    let channel = link.channel;
    let hub = hub.clone();
    let lock_title: SharedString = if strip.locked {
        "Locked: the musician's phone cannot change this channel; you still can".into()
    } else {
        "Unlocked: the musician's phone can change this channel".into()
    };
    let name_title = SharedString::from(label.clone());
    strip_column(84.0)
        .id(SharedString::from(key.clone()))
        .role(Role::Group)
        .aria_label(SharedString::from(mirror.label.clone()))
        .child(
            div()
                .id(id("name"))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(3.0))
                .w_full()
                .text_size(px(text::MD))
                .font_weight(weight::BOLD)
                .whitespace_nowrap()
                .tooltip(move |window, cx| tooltip(name_title.clone(), window, cx))
                .child(icon(kind_paths(&strip.kind), 14.0))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(seen(label.clone())),
                ),
        )
        .child(
            div()
                .min_h(px(20.0))
                .flex()
                .justify_center()
                .max_w_full()
                .when_some(strip.group.as_ref(), |row, group| {
                    row.child(chip(id("chip"), group))
                }),
        )
        .when(mirror.disabled, |column| {
            let leader = strip.following.clone().unwrap_or_default();
            column.child(follows_line(
                id("follows"),
                mirror.text.clone(),
                Some(format!(
                    "This mix follows {leader}'s: move {leader}'s instead"
                )),
            ))
        })
        .child(
            // A mono strip keeps the line's room, so every strip's pan and fader line up.
            div()
                .flex()
                .items_center()
                .gap(px(3.0))
                .text_color(color(colors::TEXT_SECONDARY))
                .text_size(px(text::XXS))
                .font_weight(weight::BOLD)
                .when(!strip.stereo, |line| line.opacity(0.0))
                .child(icon(STEREO_PATHS, 12.0))
                .child(seen("STEREO")),
        )
        .child(
            pan(
                id("pan"),
                f64::from(strip.pan),
                if strip.stereo {
                    format!("{label} balance")
                } else {
                    format!("{label} pan")
                },
            )
            .disabled(mirror.disabled)
            .on_input(move |value, _, _| pan_edit(EditValue::Level(value.clamp(0, 100) as u8))),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .gap(px(6.0))
                .items_stretch()
                .min_h(px(200.0))
                .child(LevelMeter::new(
                    id("meter"),
                    f64::from(level),
                    format!("{label} input"),
                ))
                .child(
                    fader(
                        id("fader"),
                        f64::from(strip.gain),
                        format!("{label} level"),
                        match strip.tone {
                            StripTone::Muted => FaderTone::Muted,
                            StripTone::Default => FaderTone::Default,
                        },
                        false,
                    )
                    .disabled(mirror.disabled)
                    .on_input(move |value, _, _| {
                        gain_edit(EditValue::Level(value.clamp(0, 100) as u8))
                    }),
                ),
        )
        .child(readout(&strip.readout, strip.muted))
        .child(
            div()
                .flex()
                .gap(px(4.0))
                .child(
                    ToggleButton::new(
                        id("mute"),
                        "M",
                        format!("Mute {label}"),
                        ToggleKind::Mute,
                        strip.muted,
                    )
                    .disabled(mirror.disabled)
                    .on_toggle(move |pressed, _, _| mute_edit(EditValue::Flag(pressed))),
                )
                .child(
                    ToggleButton::new(
                        id("solo"),
                        "S",
                        format!("Solo {label}"),
                        ToggleKind::Solo,
                        strip.solo,
                    )
                    .disabled(mirror.disabled)
                    .on_toggle(move |pressed, _, _| solo_edit(EditValue::Flag(pressed))),
                ),
        )
        .when(strip.lockable, |column| {
            // A lock binds only the phone: it stays live while the mix follows another.
            column.child(
                div()
                    .id(id("lock-why"))
                    .tooltip(move |window, cx| tooltip(lock_title.clone(), window, cx))
                    .child(
                        ToggleButton::new(
                            id("lock"),
                            strip.lock_text,
                            format!("Lock {label} on the musician's phone"),
                            ToggleKind::Plain,
                            strip.locked,
                        )
                        .wide()
                        .on_toggle(move |pressed, _, _| lock_edit(EditValue::Flag(pressed))),
                    ),
            )
        })
        .child(
            div()
                .id(id("processing"))
                .role(Role::Link)
                .aria_label(SharedString::from(link.label.clone()))
                .focusable()
                .tab_stop(true)
                .flex()
                .items_center()
                .justify_center()
                .w_full()
                .min_h(px(24.0))
                .rounded(px(radius::SM))
                .border_1()
                .border_color(color(if link.state == ProcessingState::On {
                    colors::ACCENT
                } else {
                    colors::BORDER
                }))
                .bg(color(colors::SURFACE_RAISED))
                .text_color(color(if link.state == ProcessingState::On {
                    colors::TEXT
                } else {
                    colors::TEXT_SECONDARY
                }))
                .text_size(px(text::XXS))
                .font_weight(weight::BOLD)
                .whitespace_nowrap()
                .cursor_pointer()
                .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
                .tooltip(|window, cx| tooltip(PROCESSING_LINK_TITLE.into(), window, cx))
                .on_click(move |_: &ClickEvent, _, cx| {
                    // The input's processing is shared by every mix; a pair's opens on its first side.
                    hub.update(cx, |hub, cx| {
                        hub.navigate(
                            Route {
                                section: Section::Processing,
                                channel: Some(channel),
                            },
                            cx,
                        )
                    });
                })
                .child(seen(link.text)),
        )
}

/// One group's strip, keyed by the group's id. Groups have no mute of their own in the engine.
pub fn group_strip(group: &GroupStrip, model: &Model) -> impl IntoElement {
    let key = format!("group-{}", group.id);
    let id = |part: &str| SharedString::from(format!("{key}-{part}"));
    let mirror = following_state(group.following.as_deref(), &format!("{} group", group.name));
    let (model, index) = (model.clone(), group.index);
    strip_column(74.0)
        .id(SharedString::from(key.clone()))
        .bg(color(colors::SURFACE_SUNKEN))
        .role(Role::Group)
        .aria_label(SharedString::from(mirror.label.clone()))
        .child(caption("GROUP"))
        .child(chip(
            id("chip"),
            &StripGroup {
                id: group.id.clone(),
                name: group.name.clone(),
                tone: group.tone,
                invalid: group.invalid,
            },
        ))
        .when(mirror.disabled, |column| {
            column.child(follows_line(id("follows"), mirror.text.clone(), None))
        })
        .child(
            div().flex().flex_1().min_h(px(200.0)).child(
                fader(
                    id("fader"),
                    f64::from(group.level),
                    format!("{} group level", group.name),
                    FaderTone::Default,
                    false,
                )
                .disabled(mirror.disabled)
                .on_input(move |value, _, _| model.edit_group(index, value.clamp(0, 100) as u8)),
            ),
        )
        .child(readout(&group.readout, false))
}
