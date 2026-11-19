//! The groups panel (`group-cards.js`): its head with New group, the hints, and one card per group
//! with its chip, count, members and an editor (name, inputs, delete with a confirmation).

use std::time::{Duration, Instant};

use bandear_desktop_kit::a11y::seen;
use bandear_desktop_kit::buttons::{Button, ButtonKind};
use bandear_desktop_kit::forms::text_input;
use bandear_desktop_kit::gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::gpui_component::input::Escape;
use bandear_desktop_kit::status::GroupChip;
use bandear_desktop_kit::theme::{color, colors, radius, text, weight};

use super::atoms::{Look, checkbox, focus_button};
use super::{Armed, ChannelsScreen};
use crate::groups::GroupCard;
use crate::model::{Edit, ScreenView};

/// How long "Delete" waits after the first press before it works.
const CONFIRM_DELAY: Duration = Duration::from_millis(600);
/// How long a delete stays armed.
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(10);

impl ChannelsScreen {
    /// The groups aside, beside the table (below it when narrow).
    pub(super) fn groups(
        &self,
        view: &ScreenView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = self.model.clone();
        let can_add = view.can_add;
        let head = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .id("groups-heading")
                    .role(Role::Heading)
                    .aria_level(2)
                    .aria_label("Groups")
                    .text_size(px(text::LG))
                    .font_weight(weight::BOLD)
                    .child(seen("Groups")),
            )
            .child(
                div()
                    .text_size(px(text::XS))
                    .text_color(color(colors::TEXT_MUTED))
                    .child("up to 16"),
            )
            .child(div().flex_1())
            .child(
                Button::new("groups-add", "New group", ButtonKind::Primary)
                    .disabled(!can_add)
                    .on_press(move |_, _| {
                        if can_add {
                            let flow = model.handle(Edit::GroupAdd);
                            model.console().spawn(flow);
                        }
                    }),
            );
        let hint = |id: &'static str, words: &'static str| {
            div()
                .id(id)
                .text_size(px(text::SM))
                .text_color(color(colors::TEXT_MUTED))
                .child(words)
        };
        let cards: Vec<AnyElement> = view
            .cards
            .iter()
            .map(|card| self.card(card, window, cx))
            .collect();
        div()
            .id("groups")
            .role(Role::Complementary)
            .aria_label("Groups")
            .flex_1()
            .flex_basis(px(280.0))
            .min_w(px(260.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(head)
            .when(!can_add, |panel| {
                panel.child(hint(
                    "groups-full",
                    "The engine takes at most 16 groups: delete one to make another.",
                ))
            })
            .when(view.cards.is_empty(), |panel| {
                panel.child(hint(
                    "groups-empty",
                    "No groups yet. A group gives every musician one fader for its inputs.",
                ))
            })
            .children(cards)
            .into_any_element()
    }

    fn card(&self, card: &GroupCard, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let id = card.id.clone();
        let element = |part: &str| SharedString::from(format!("group-{}-{part}", card.id));
        let open = self.open.contains(&card.id);
        let Some(fields) = self.cards.get(&card.id) else {
            return div().into_any_element();
        };
        let toggle = {
            let id = id.clone();
            cx.listener(move |this: &mut Self, (): &(), window: &mut Window, cx| {
                if this.open.remove(&id) {
                    if this.armed.as_ref().is_some_and(|armed| armed.id == id) {
                        this.armed = None;
                    }
                } else {
                    this.open.insert(id.clone());
                    if let Some(fields) = this.cards.get(&id) {
                        fields.name.update(cx, |state, cx| state.focus(window, cx));
                    }
                }
                cx.notify();
            })
        };
        let edit = focus_button(
            element("edit"),
            if open { "Close" } else { "Edit" },
            format!("{} {}", if open { "Close" } else { "Edit" }, card.name),
            Look::Secondary,
            &fields.edit,
            false,
            move |window, cx| toggle(&(), window, cx),
        )
        .aria_expanded(open);
        let top = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(GroupChip::new(
                element("chip"),
                card.tone,
                Some(card.name.clone().into()),
                card.invalid,
            ))
            .child(
                div()
                    .text_size(px(text::XS))
                    .text_color(color(colors::TEXT_MUTED))
                    .child(card.count.clone()),
            )
            .child(div().flex_1())
            .child(edit);
        let members = div()
            .flex()
            .flex_wrap()
            .gap(px(6.0))
            .children(card.members.iter().map(|member| {
                div()
                    .px(px(8.0))
                    .py(px(2.0))
                    .rounded(px(radius::PILL))
                    .bg(color(colors::SURFACE_RAISED))
                    .text_size(px(text::XS))
                    .text_color(color(colors::TEXT_SECONDARY))
                    .child(member.clone())
            }));
        let disarm = cx.listener(move |this: &mut Self, event: &KeyDownEvent, _, cx| {
            if event.keystroke.key == "escape" && this.armed.is_some() {
                this.armed = None;
                cx.notify();
            }
        });
        div()
            .id(element("card"))
            .role(Role::Region)
            .aria_label(card.name.clone())
            .on_key_down(disarm)
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(color(colors::BORDER_SUBTLE))
            .bg(color(colors::SURFACE_SUNKEN))
            .child(top)
            .when(!card.members.is_empty(), |element| element.child(members))
            .when(card.members.is_empty(), |element| {
                element.child(
                    div()
                        .text_size(px(text::SM))
                        .text_color(color(colors::TEXT_MUTED))
                        .child("No channels yet."),
                )
            })
            .when(open, |element| element.child(self.editor(card, window, cx)))
            .into_any_element()
    }

    /// A card's editor: its name, its inputs, the hint and the actions.
    fn editor(&self, card: &GroupCard, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let id = card.id.clone();
        let element = |part: &str| SharedString::from(format!("group-{}-{part}", card.id));
        let Some(fields) = self.cards.get(&card.id) else {
            return div().into_any_element();
        };
        let name = div()
            .id(element("name-field"))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .on_action({
                let id = id.clone();
                cx.listener(move |this, _: &Escape, window, cx| {
                    this.revert_group_name(&id, window, cx);
                    cx.stop_propagation();
                })
            })
            .child(
                div()
                    .text_size(px(text::XS))
                    .text_color(color(colors::TEXT_MUTED))
                    .child(seen("Name")),
            )
            .child(text_input(
                &fields.name,
                format!("Name of the {} group", card.name),
            ));
        let choices = card.choices.iter().map(|choice| {
            let model = self.model.clone();
            let (group, key, numbers) = (id.clone(), choice.key.clone(), choice.numbers.clone());
            let mut notes = Vec::new();
            if let Some(owner) = &choice.owner {
                notes.push(format!("in {owner}"));
            }
            if choice.hidden {
                notes.push("hidden".to_owned());
            }
            let note = notes.join(", ");
            let label = if note.is_empty() {
                choice.label.clone()
            } else {
                format!("{}, {note}", choice.label)
            };
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(checkbox(
                    SharedString::from(format!("group-{id}-choice-{}", choice.key)),
                    choice.checked,
                    label,
                    choice.label.clone(),
                    None,
                    move |member, _, _| {
                        let flow = model.handle(Edit::GroupMember {
                            id: group.clone(),
                            key: key.clone(),
                            numbers: numbers.clone(),
                            member,
                        });
                        model.console().spawn(flow);
                    },
                ))
                .when(!note.is_empty(), |row| {
                    row.child(
                        div()
                            .text_size(px(text::XS))
                            .text_color(color(colors::TEXT_MUTED))
                            .child(seen(note)),
                    )
                })
        });
        let inputs = div()
            .id(element("inputs"))
            .role(Role::Group)
            .aria_label(format!("Inputs in {}", card.name))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .max_h(px(260.0))
            .overflow_y_scroll()
            .p(px(8.0))
            .rounded(px(radius::SM))
            .border_1()
            .border_color(color(colors::BORDER_SUBTLE))
            .child(
                div()
                    .text_size(px(text::XS))
                    .text_color(color(colors::TEXT_MUTED))
                    .child(seen(format!("Inputs in {}", card.name))),
            )
            .children(choices);
        let done = {
            let id = id.clone();
            let edit = fields.edit.clone();
            focus_button(
                element("done"),
                "Done",
                "Done",
                Look::Secondary,
                &fields.done,
                false,
                {
                    let screen = cx.entity().downgrade();
                    move |window, cx| {
                        let id = id.clone();
                        let edit = edit.clone();
                        drop(screen.update(cx, |this, cx| {
                            this.open.remove(&id);
                            if this.armed.as_ref().is_some_and(|armed| armed.id == id) {
                                this.armed = None;
                            }
                            window.focus(&edit, cx);
                            cx.notify();
                        }));
                    }
                },
            )
        };
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(name)
            .child(inputs)
            .child(
                div()
                    .text_size(px(text::XS))
                    .text_color(color(colors::TEXT_MUTED))
                    .child(
                        "Choosing an input that is in another group moves it here. A stereo pair \
                         joins as one.",
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.delete_slot(card, cx))
                    .child(div().flex_1())
                    .child(done),
            )
            .into_any_element()
    }

    /// The delete slot: "Delete group", or once pressed, Keep, the question and Delete (which
    /// works 600 ms after the first press, and disarms by itself after 10 s).
    fn delete_slot(&self, card: &GroupCard, cx: &mut Context<Self>) -> AnyElement {
        let id = card.id.clone();
        let element = |part: &str| SharedString::from(format!("group-{}-{part}", card.id));
        let screen = cx.entity().downgrade();
        let armed = self.armed.as_ref().filter(|armed| armed.id == card.id);
        let Some(armed) = armed else {
            let keep = self.keep.clone();
            return Button::new(element("delete"), "Delete group", ButtonKind::Danger)
                .description(format!("Delete the {} group", card.name))
                .on_press(move |window, cx| {
                    let id = id.clone();
                    drop(screen.update(cx, |this, cx| {
                        this.armings += 1;
                        let generation = this.armings;
                        this.armed = Some(Armed {
                            id,
                            at: Instant::now(),
                            generation,
                        });
                        window.focus(&keep, cx);
                        // Draw again once Delete works, and disarm once the wait is over.
                        cx.spawn(async move |this, cx| {
                            cx.background_executor().timer(CONFIRM_DELAY).await;
                            drop(this.update(cx, |_, cx| cx.notify()));
                            cx.background_executor()
                                .timer(CONFIRM_TIMEOUT - CONFIRM_DELAY)
                                .await;
                            drop(this.update(cx, |this, cx| {
                                if this
                                    .armed
                                    .as_ref()
                                    .is_some_and(|armed| armed.generation == generation)
                                {
                                    this.armed = None;
                                    cx.notify();
                                }
                            }));
                        })
                        .detach();
                        cx.notify();
                    }));
                })
                .into_any_element();
        };
        let ready = armed.at.elapsed() >= CONFIRM_DELAY;
        let keep = focus_button(
            element("keep"),
            "Keep",
            "Keep",
            Look::Secondary,
            &self.keep,
            false,
            {
                let screen = screen.clone();
                move |_, cx| {
                    drop(screen.update(cx, |this, cx| {
                        this.armed = None;
                        cx.notify();
                    }));
                }
            },
        );
        let model = self.model.clone();
        let confirm = focus_button(
            element("confirm"),
            "Delete",
            "Delete",
            Look::Danger,
            &self.confirm,
            !ready,
            move |_, cx| {
                drop(screen.update(cx, |this, cx| {
                    this.armed = None;
                    cx.notify();
                }));
                let flow = model.handle(Edit::GroupDelete { id: id.clone() });
                model.console().spawn(flow);
            },
        )
        .when(!ready, |button| button.aria_description("Wait a moment"));
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .child(keep)
            .child(
                div()
                    .text_size(px(text::SM))
                    .text_color(color(colors::TEXT_SECONDARY))
                    .child(format!(
                        "Delete {}? Its inputs keep their own faders.",
                        card.name
                    )),
            )
            .child(confirm)
            .into_any_element()
    }
}
