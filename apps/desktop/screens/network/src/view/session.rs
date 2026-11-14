//! The Session card (`session-panel.js`): the desk's notice, Save session as, the saved sessions
//! with Load (and its question while edits are on their way), Rename to and the guarded Delete,
//! the hint, then the Mix backups with their Restore question, and the last outcome (D1-D6).

use bandear_desktop_kit::a11y::seen;
use bandear_desktop_kit::buttons::{Button, ButtonKind};
use bandear_desktop_kit::forms::{Choice, SelectEvent, select, text_input};
use bandear_desktop_kit::gpui::{
    Context, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window, div, prelude::FluentBuilder, px,
};
use bandear_desktop_kit::gpui_component::input::InputEvent;
use bandear_desktop_kit::theme::{FONT_SANS, color, colors, radius, text, weight};

use super::parts::{Tone, card, focus_button, label, line, said_line, sync_select};
use super::{NetworkScreen, SessionFields};
use crate::model::sessions::{
    backups_unread_text, load_ask_text, restore_ask_text, sessions_unread_text,
};

const LOAD_HINT: &str = "Loading replaces the channels, groups, presets, labels, limits, locks and mirrors, and sends each saved mix to the musicians who are connected. A phone that is not connected keeps its own mix. The saved audio input is used from the next start.";
const BACKUPS_HINT: &str = "The musicians' mixes are backed up a few seconds after they change, and the last 10 are kept. Restore sends each backed-up mix to the musicians who are connected now; a phone that is not connected keeps its own mix.";

/// The panel's fields: Enter saves or renames; a session chosen goes to the model.
pub(super) fn subscribe(
    session: &SessionFields,
    window: &mut Window,
    cx: &mut Context<NetworkScreen>,
) -> Vec<Subscription> {
    vec![
        cx.subscribe_in(
            &session.save_name,
            window,
            |this, state, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let typed = state.read(cx).value().to_string();
                    this.model.save_session(&typed);
                }
            },
        ),
        cx.subscribe_in(
            &session.rename,
            window,
            |this, state, event: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let typed = state.read(cx).value().to_string();
                    this.model.rename_session(&typed);
                }
            },
        ),
        cx.subscribe_in(
            &session.saved,
            window,
            |this, _, event: &SelectEvent<Vec<Choice>>, _, _| {
                if let SelectEvent::Confirm(Some(id)) = event {
                    this.model.choose_session(id);
                }
            },
        ),
    ]
}

fn separator() -> impl IntoElement {
    div().h(px(1.0)).bg(color(colors::BORDER_SUBTLE))
}

fn ask_box(
    id: &'static str,
    words: String,
) -> bandear_desktop_kit::gpui::Stateful<bandear_desktop_kit::gpui::Div> {
    div()
        .id(id)
        .role(Role::Alert)
        .aria_label(words.clone())
        .flex()
        .flex_col()
        .gap(px(8.0))
        .py(px(8.0))
        .px(px(10.0))
        .rounded(px(radius::MD))
        .border_1()
        .border_color(color(colors::WARN_BORDER))
        .bg(color(colors::WARN_BG))
        .text_size(px(text::MD))
        .child(seen(words))
}

impl NetworkScreen {
    pub(super) fn sync_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = &self.view.session;
        let options: Vec<Choice> = view
            .panel
            .rows
            .iter()
            .map(|row| Choice::new(row.id.clone(), row.label.clone()))
            .collect();
        let chosen = view
            .chosen
            .as_ref()
            .map(|row| row.id.clone())
            .unwrap_or_default();
        sync_select(
            &self.session.saved,
            &mut self.session.saved_shown,
            options,
            &chosen,
            window,
            cx,
        );
    }

    pub(super) fn render_session(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = &self.view.session;
        let panel = &view.panel;
        let saving = self.model.clone();
        let save_name = self.session.save_name.clone();
        let chosen_name = view
            .chosen
            .as_ref()
            .map(|row| row.name.clone())
            .unwrap_or_default();
        card("session", "Session")
            .when_some(panel.notice.clone(), |card, notice| {
                card.child(line("session-notice", Tone::Caution, notice))
            })
            .child(label("Save session as"))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .child(text_input(&self.session.save_name, "Save session as")),
                    )
                    .child(
                        Button::new("save-session", "Save", ButtonKind::Secondary).on_press(
                            move |_, cx| {
                                let typed = save_name.read(cx).value().to_string();
                                saving.save_session(&typed);
                            },
                        ),
                    ),
            )
            .when(!panel.engine_running, |card| {
                card.child(line(
                    "save-idle",
                    Tone::Hint,
                    "The engine is not running: the session is saved without the musicians' mixes.",
                ))
            })
            .child(separator())
            .when_some(panel.unread.clone(), |card, unread| {
                let listing = self.model.clone();
                card.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.0))
                        .child(line(
                            "sessions-unread",
                            Tone::Hint,
                            sessions_unread_text(&unread),
                        ))
                        .child(
                            Button::new("list-sessions", "List them again", ButtonKind::Secondary)
                                .on_press(move |_, _| listing.list_sessions()),
                        ),
                )
            })
            .when(panel.unread.is_none() && panel.rows.is_empty(), |card| {
                card.child(line("sessions-none", Tone::Hint, "No saved sessions yet."))
            })
            .when(!panel.rows.is_empty(), |card| {
                card.child(self.render_saved(&chosen_name, cx))
            })
            .child(line("load-hint", Tone::Hint, LOAD_HINT))
            .child(separator())
            .child(self.render_backups(cx))
            .when_some(view.said.as_ref(), |card, said| {
                card.child(said_line("session-said", said))
            })
            .font_family(FONT_SANS)
    }

    fn render_saved(&self, name: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let view = &self.view.session;
        let detail = view
            .chosen
            .as_ref()
            .map(|row| row.detail.clone())
            .unwrap_or_default();
        let loader = self.model.clone();
        let cancel_focus = self.session.cancel_load.clone();
        let renaming = self.model.clone();
        let rename = self.session.rename.clone();
        let loadable = view.load_ready;
        let (armed, ready) = view.delete;
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(label("Saved sessions"))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(select(&self.session.saved, "Saved sessions")),
                    )
                    .child(focus_button(
                        "load-session",
                        "Load",
                        ButtonKind::Primary,
                        &self.session.load,
                        !loadable,
                        move |window, cx| {
                            loader.press_load();
                            // While it asks, Cancel takes the focus.
                            cancel_focus.focus(window, cx);
                        },
                    )),
            )
            .child(
                div()
                    .text_size(px(text::SM))
                    .text_color(color(colors::TEXT_MUTED))
                    .child(seen(detail)),
            )
            .when_some(view.asking.as_ref(), |saved, asking| {
                let cancel = self.model.clone();
                let anyway = self.model.clone();
                let escape = self.model.clone();
                let load_focus = self.session.load.clone();
                let back = self.session.load.clone();
                saved.child(
                    ask_box("load-ask", load_ask_text(asking.unsaved, &asking.name))
                        .on_key_down(move |event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" {
                                cx.stop_propagation();
                                escape.cancel_load();
                                back.focus(window, cx);
                            }
                        })
                        .child(
                            div()
                                .flex()
                                .gap(px(8.0))
                                .child(focus_button(
                                    "cancel-load",
                                    "Cancel",
                                    ButtonKind::Secondary,
                                    &self.session.cancel_load,
                                    false,
                                    move |window, cx| {
                                        cancel.cancel_load();
                                        load_focus.focus(window, cx);
                                    },
                                ))
                                .child(
                                    Button::new("load-anyway", "Load anyway", ButtonKind::Danger)
                                        .on_press(move |_, _| anyway.confirm_load()),
                                ),
                        ),
                )
            })
            .child(label("Rename to"))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(div().flex_1().child(text_input(
                        &self.session.rename,
                        format!("New name for {name}"),
                    )))
                    .child(
                        Button::new("rename-session", "Rename", ButtonKind::Secondary)
                            .disabled(!loadable)
                            .on_press(move |_, cx| {
                                let typed = rename.read(cx).value().to_string();
                                renaming.rename_session(&typed);
                            }),
                    ),
            )
            .child(self.render_delete(name, armed, ready, cx))
    }

    fn render_delete(
        &self,
        name: &str,
        armed: bool,
        ready: bool,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let slot = div().flex().items_center().gap(px(8.0));
        if !armed {
            let arming = self.model.clone();
            let keep = self.session.keep_delete.clone();
            return slot.child(
                Button::new("delete-session", "Delete", ButtonKind::Danger)
                    .description(format!("Delete the saved session {name}"))
                    .on_press(move |window, cx| {
                        arming.arm_delete();
                        // Keep takes Delete's place, and the focus.
                        keep.focus(window, cx);
                    }),
            );
        }
        let keeper = self.model.clone();
        let escape = self.model.clone();
        let deleter = self.model.clone();
        slot.child(
            div()
                .id("keep-delete-wrap")
                .on_key_down(move |event: &KeyDownEvent, _, _| {
                    if event.keystroke.key == "escape" {
                        escape.keep_delete();
                    }
                })
                .child(focus_button(
                    "keep-delete",
                    "Keep",
                    ButtonKind::Secondary,
                    &self.session.keep_delete,
                    false,
                    move |_, _| keeper.keep_delete(),
                )),
        )
        .child(div().text_size(px(text::MD)).child(seen(format!(
            "Delete {name}? The desk's session does not change."
        ))))
        .child(
            Button::new("confirm-delete", "Delete", ButtonKind::Danger)
                .disabled(!ready)
                .on_press(move |_, _| deleter.confirm_delete()),
        )
    }

    fn render_backups(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let view = &self.view.session;
        let backups = &view.backups;
        let refresh = self.model.clone();
        let running = backups.engine_running;
        div()
            .id("backups")
            .role(Role::Group)
            .aria_label("Mix backups")
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .id("backups-title")
                            .role(Role::Heading)
                            .aria_level(3)
                            .aria_label("Mix backups")
                            .text_size(px(text::BASE))
                            .font_weight(weight::BOLD)
                            .child(seen("Mix backups")),
                    )
                    .child(
                        Button::new("refresh-backups", "Refresh", ButtonKind::Secondary)
                            .on_press(move |_, _| refresh.refresh_backups()),
                    ),
            )
            .when_some(backups.unread.clone(), |group, unread| {
                group.child(line("backups-unread", Tone::Hint, backups_unread_text(&unread)))
            })
            .when(backups.unread.is_none() && backups.rows.is_empty(), |group| {
                group.child(line(
                    "backups-none",
                    Tone::Hint,
                    "No backup yet: the musicians' mixes are backed up a few seconds after one changes.",
                ))
            })
            .when(!backups.rows.is_empty(), |group| {
                group.child(
                    div()
                        .id("backup-list")
                        .role(Role::List)
                        .aria_label("Mix backups, the newest first")
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .children(backups.rows.iter().map(|row| {
                            let stamp = row.stamp.clone();
                            let asking = self.model.clone();
                            let focus = self.session.cancel_restore.clone();
                            let enabled = row.restorable && running;
                            div()
                                .id(SharedString::from(format!("backup-{}", row.stamp)))
                                .role(Role::ListItem)
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(px(8.0))
                                .child(div().text_size(px(text::MD)).child(seen(row.label.clone())))
                                .child(
                                    Button::new(
                                        SharedString::from(format!("restore-{}", row.stamp)),
                                        "Restore",
                                        ButtonKind::Secondary,
                                    )
                                    .description(format!("Restore the mixes backed up at {}", row.label))
                                    .disabled(!enabled)
                                    .on_press(move |window, cx| {
                                        asking.ask_restore(&stamp);
                                        focus.focus(window, cx);
                                    }),
                                )
                                .child(
                                    div()
                                        .w_full()
                                        .text_size(px(text::SM))
                                        .text_color(color(colors::TEXT_MUTED))
                                        .child(seen(row.detail.clone())),
                                )
                        })),
                )
            })
            .when_some(view.restore_asking.as_ref(), |group, row| {
                let cancel = self.model.clone();
                let escape = self.model.clone();
                let confirm = self.model.clone();
                group.child(
                    ask_box("restore-ask", restore_ask_text(row))
                        .on_key_down(move |event: &KeyDownEvent, _, cx| {
                            if event.keystroke.key == "escape" {
                                cx.stop_propagation();
                                escape.cancel_restore();
                            }
                        })
                        .child(
                            div()
                                .flex()
                                .gap(px(8.0))
                                .child(focus_button(
                                    "cancel-restore",
                                    "Cancel",
                                    ButtonKind::Secondary,
                                    &self.session.cancel_restore,
                                    false,
                                    move |_, _| cancel.cancel_restore(),
                                ))
                                .child(
                                    Button::new("confirm-restore", "Restore", ButtonKind::Primary)
                                        .on_press(move |_, _| confirm.confirm_restore()),
                                ),
                        ),
                )
            })
            .when(!running && !backups.rows.is_empty(), |group| {
                group.child(line("backups-idle", Tone::Hint, "Start the engine to restore a backup."))
            })
            .child(line("backups-hint", Tone::Hint, BACKUPS_HINT))
    }
}
