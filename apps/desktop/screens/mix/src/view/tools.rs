//! The Mix tools beside the tabs (`mix-tools.js`): the Mirror select, Copy mix to… and Presets with
//! their panels. The rules live in [crate::tools::Tools] (in the model); this draws what it answers
//! and hands it the engineer's moves.

use std::time::Instant;

use bandear_desktop_kit::a11y::seen;
use bandear_desktop_kit::forms::{
    Choice, ChoiceState, InputEvent, SelectEvent, choice_state, select, set_choices, text_input,
    text_state,
};
use bandear_desktop_kit::gpui::{
    AnyElement, App, Context, Div, Entity, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Role, SharedString, StatefulInteractiveElement, Styled,
    Subscription, WeakEntity, Window, deferred, div, prelude::FluentBuilder, px, relative,
};
use bandear_desktop_kit::gpui_component::input::InputState;
use bandear_desktop_kit::theme::{color, colors, radius, text, weight};

use crate::tools::{ARMED_FOR, DELETE_DELAY, Focus, MixToolsView, Panel, TickList, ToolRequest};
use crate::view::MixScreen;
use crate::view::atoms::{Look, ToolButton, checkbox, hint};

/// The tools' parts that live between frames: the selects, the name fields and the focus handles
/// the tools move the focus to.
pub struct ToolsParts {
    mirror: Entity<ChoiceState>,
    mirror_shown: Option<(Vec<Choice>, String)>,
    preset: Entity<ChoiceState>,
    presets_shown: Option<Vec<Choice>>,
    save_name: Entity<InputState>,
    rename_to: Entity<InputState>,
    copy_opener: FocusHandle,
    presets_opener: FocusHandle,
    copy_first: FocusHandle,
    keep: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

/// A preset name is at most this long, as the desk keeps it.
const NAME_LENGTH: usize = crate::tools::MAX_PRESET_NAME_LENGTH;

impl ToolsParts {
    pub fn new(window: &mut Window, cx: &mut Context<MixScreen>) -> Self {
        let mirror = choice_state(Vec::new(), None, window, cx);
        let preset = choice_state(Vec::new(), None, window, cx);
        let save_name = text_state("", window, cx);
        let rename_to = text_state("", window, cx);
        let subscriptions = vec![
            cx.subscribe_in(
                &mirror,
                window,
                |this: &mut MixScreen, _, event: &SelectEvent<Vec<Choice>>, _, cx| {
                    let SelectEvent::Confirm(Some(value)) = event else {
                        return;
                    };
                    let request = this.model.with_tools(|tools| tools.choose_mirror(value));
                    if let Some(request) = request {
                        this.model.tool(request);
                    }
                    cx.notify();
                },
            ),
            cx.subscribe_in(
                &preset,
                window,
                |this: &mut MixScreen, _, _: &SelectEvent<Vec<Choice>>, _, cx| {
                    // Another preset: an armed delete was for the one before.
                    this.model.with_tools(|tools| tools.keep());
                    cx.notify();
                },
            ),
            cx.subscribe_in(
                &save_name,
                window,
                |this: &mut MixScreen, input, event, window, cx| match event {
                    InputEvent::PressEnter { .. } => this.save(cx),
                    InputEvent::Change => keep_length(input, window, cx),
                    _ => {}
                },
            ),
            cx.subscribe_in(
                &rename_to,
                window,
                |this: &mut MixScreen, input, event, window, cx| match event {
                    InputEvent::PressEnter { .. } => this.rename(cx),
                    InputEvent::Change => keep_length(input, window, cx),
                    _ => {}
                },
            ),
        ];
        Self {
            mirror,
            mirror_shown: None,
            preset,
            presets_shown: None,
            save_name,
            rename_to,
            copy_opener: cx.focus_handle(),
            presets_opener: cx.focus_handle(),
            copy_first: cx.focus_handle(),
            keep: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }
}

/// Holds a name field to the desk's longest name, as `maxlength` did.
fn keep_length(input: &Entity<InputState>, window: &mut Window, cx: &mut Context<MixScreen>) {
    let value = input.read(cx).value();
    if value.chars().count() > NAME_LENGTH {
        let kept: String = value.chars().take(NAME_LENGTH).collect();
        input.update(cx, |input, cx| input.set_value(kept, window, cx));
    }
}

fn panel_box(id: &'static str, label: &'static str) -> bandear_desktop_kit::gpui::Stateful<Div> {
    div()
        .id(id)
        .role(Role::Group)
        .aria_label(label)
        .occlude()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .w(px(440.0))
        .p(px(14.0))
        .rounded(px(radius::LG))
        .border_1()
        .border_color(color(colors::BORDER))
        .bg(color(colors::SURFACE))
        .shadow_lg()
}

fn title(words: String) -> Div {
    div()
        .text_size(px(text::BASE))
        .font_weight(weight::BOLD)
        .child(words)
}

/// A fieldset of ticks with its legend.
fn fieldset(legend: &'static str, rows: Vec<AnyElement>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .py(px(8.0))
        .px(px(10.0))
        .rounded(px(radius::MD))
        .border_1()
        .border_color(color(colors::BORDER_SUBTLE))
        .child(
            div()
                .text_size(px(text::SM))
                .text_color(color(colors::TEXT_SECONDARY))
                .child(legend),
        )
        .children(rows)
}

fn row() -> Div {
    div().flex().flex_wrap().items_center().gap(px(8.0))
}

/// The words before a field.
fn label(words: String) -> Div {
    div()
        .text_size(px(text::MD))
        .text_color(color(colors::TEXT_SECONDARY))
        .child(seen(words))
}

impl MixScreen {
    /// Puts the tools' state on their selects and fields: the Mirror's choices and value, the
    /// presets kept as they were picked, and the fields a done save or rename emptied.
    pub(super) fn sync_tools(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let clears = self.model.take_clears();
        if clears.save {
            self.tools
                .save_name
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        if clears.rename {
            self.tools
                .rename_to
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        let shown = {
            let tools = self.model.tools();
            tools.data().map(|data| {
                let choices: Vec<Choice> = data
                    .mirror
                    .options
                    .iter()
                    .map(|option| Choice::new(option.value.clone(), option.label.clone()))
                    .collect();
                let presets: Vec<Choice> = data
                    .presets
                    .iter()
                    .flatten()
                    .map(|preset| Choice::new(preset.id.clone(), preset.name.clone()))
                    .collect();
                ((choices, tools.mirror_value()), presets)
            })
        };
        let Some((mirror, presets)) = shown else {
            return;
        };
        if self.tools.mirror_shown.as_ref() != Some(&mirror) {
            set_choices(
                &self.tools.mirror,
                mirror.0.clone(),
                Some(&mirror.1),
                window,
                cx,
            );
            self.tools.mirror_shown = Some(mirror);
        }
        if self.tools.presets_shown.as_ref() != Some(&presets) {
            // The choice stays when it is still listed, else the first preset is picked.
            let kept = self.selected_preset(cx);
            let pick = kept
                .filter(|id| presets.iter().any(|choice| choice.value.as_ref() == id))
                .or_else(|| presets.first().map(|choice| choice.value.to_string()));
            set_choices(
                &self.tools.preset,
                presets.clone(),
                pick.as_deref(),
                window,
                cx,
            );
            self.tools.presets_shown = Some(presets);
        }
    }

    fn selected_preset(&self, cx: &App) -> Option<String> {
        self.tools
            .preset
            .read(cx)
            .selected_value()
            .map(|value| value.to_string())
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let name = self.tools.save_name.read(cx).value().to_string();
        if let Some(request) = self.model.with_tools(|tools| tools.save(&name)) {
            self.model.tool(request);
        }
        cx.notify();
    }

    fn rename(&mut self, cx: &mut Context<Self>) {
        let name = self.tools.rename_to.read(cx).value().to_string();
        let id = self.selected_preset(cx);
        if let Some(request) = self
            .model
            .with_tools(|tools| tools.rename(id.as_deref(), &name))
        {
            self.model.tool(request);
        }
        cx.notify();
    }

    fn focus_on(&mut self, focus: Focus, window: &mut Window, cx: &mut Context<Self>) {
        match focus {
            Focus::Panel(Panel::Copy) => window.focus(&self.tools.copy_first, cx),
            Focus::Panel(Panel::Presets) => self
                .tools
                .save_name
                .update(cx, |input, cx| input.focus(window, cx)),
            Focus::Opener(Panel::Copy) => window.focus(&self.tools.copy_opener, cx),
            Focus::Opener(Panel::Presets) => window.focus(&self.tools.presets_opener, cx),
            Focus::Stay => {}
        }
        cx.notify();
    }

    fn toggle(&mut self, panel: Panel, window: &mut Window, cx: &mut Context<Self>) {
        let (focus, load) = self.model.with_tools(|tools| tools.toggle(panel));
        if let Some(load) = load {
            self.model.tool(load);
        }
        self.focus_on(focus, window, cx);
    }

    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = self.model.with_tools(|tools| tools.escape());
        self.focus_on(focus, window, cx);
    }

    /// Delete pressed: it asks first, the confirming Delete works after the pause, and it disarms
    /// itself after a while; Keep takes Delete's place and the focus.
    fn arm_delete(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let generation = self
            .model
            .with_tools(|tools| tools.arm(&id, Instant::now()));
        window.focus(&self.tools.keep, cx);
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor().timer(DELETE_DELAY).await;
            drop(this.update(cx, |_, cx| cx.notify()));
        })
        .detach();
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor().timer(ARMED_FOR).await;
            drop(this.update(cx, |this, cx| {
                if this.model.with_tools(|tools| tools.expire(generation)) {
                    cx.notify();
                }
            }));
        })
        .detach();
        cx.notify();
    }

    /// The tools row, its mirror note and the open panel.
    pub(super) fn tools_row(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(data) = self.model.tools().data().cloned() else {
            return div().into_any_element();
        };
        let open = self.model.tools().open();
        let this = cx.weak_entity();
        let toggle = |panel: Panel| {
            let this = this.clone();
            move |window: &mut Window, cx: &mut App| {
                drop(this.update(cx, |this, cx| this.toggle(panel, window, cx)));
            }
        };
        let row = row()
            .justify_end()
            .child(label("Mirror".to_owned()))
            .child(
                div()
                    .id("mirror-described")
                    .w(px(180.0))
                    .aria_description(SharedString::from(
                        data.mirror.note.clone().unwrap_or_default(),
                    ))
                    .child(select(
                        &self.tools.mirror,
                        format!("Mirror: whose mix {}'s follows", data.name),
                    )),
            )
            .child(
                ToolButton::new("open-copy", "Copy mix to\u{2026}", Look::Plain)
                    .focus(&self.tools.copy_opener)
                    .expanded(open == Some(Panel::Copy))
                    .on_press(toggle(Panel::Copy)),
            )
            .child(
                ToolButton::new("open-presets", "Presets", Look::Plain)
                    .focus(&self.tools.presets_opener)
                    .expanded(open == Some(Panel::Presets))
                    .on_press(toggle(Panel::Presets)),
            );
        let panel = match open {
            Some(Panel::Copy) => Some(self.copy_panel(&data, cx)),
            Some(Panel::Presets) => Some(self.presets_panel(&data, cx)),
            None => None,
        };
        div()
            .relative()
            .flex()
            .flex_col()
            .items_end()
            .flex_none()
            .ml_auto()
            .child(row)
            .when_some(data.mirror.note.clone(), |tools, note| {
                tools.child(
                    div()
                        .mt(px(6.0))
                        .text_right()
                        .text_size(px(text::SM))
                        .text_color(color(colors::TEXT_SECONDARY))
                        .child(note),
                )
            })
            .when_some(panel, |tools, panel| {
                tools.child(deferred(
                    div()
                        .absolute()
                        .top(relative(1.0))
                        .right_0()
                        .mt(px(6.0))
                        .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" {
                                cx.stop_propagation();
                                this.escape(window, cx);
                            }
                        }))
                        .child(panel),
                ))
            })
            .into_any_element()
    }

    /// The ticks of `list`, the first taking the panel's focus when `first` is given.
    fn ticks(
        &self,
        list: TickList,
        data: &MixToolsView,
        first: Option<&FocusHandle>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let targets = match list {
            TickList::Copy => &data.copy_targets,
            TickList::Apply => &data.preset_targets,
        };
        let tools = self.model.tools();
        targets
            .iter()
            .enumerate()
            .map(|(index, target)| {
                let this = cx.weak_entity();
                let address = target.address.clone();
                let prefix = match list {
                    TickList::Copy => "copy",
                    TickList::Apply => "apply",
                };
                checkbox(
                    SharedString::from(format!("{prefix}-{address}")),
                    &target.label,
                    target.note.as_deref(),
                    tools.is_ticked(list, &target.address),
                    target.disabled,
                    if index == 0 { first } else { None },
                    move |checked, _, cx| {
                        drop(this.update(cx, |this, cx| {
                            this.model
                                .with_tools(|tools| tools.tick(list, &address, checked));
                            cx.notify();
                        }));
                    },
                )
                .into_any_element()
            })
            .collect()
    }

    fn run(&self, request: Option<ToolRequest>) {
        if let Some(request) = request {
            self.model.tool(request);
        }
    }

    fn copy_panel(&mut self, data: &MixToolsView, cx: &mut Context<Self>) -> AnyElement {
        let ready = self.model.tools().copy_ready();
        let this = cx.weak_entity();
        let copy = {
            let this = this.clone();
            move |_: &mut Window, cx: &mut App| {
                drop(this.update(cx, |this, cx| {
                    let request = this.model.tools().copy();
                    this.run(request);
                    cx.notify();
                }));
            }
        };
        let close = move |window: &mut Window, cx: &mut App| {
            drop(this.update(cx, |this, cx| this.escape(window, cx)));
        };
        let first = data
            .copy_targets
            .is_empty()
            .then_some(&self.tools.copy_first);
        let ticks = self.ticks(
            TickList::Copy,
            data,
            Some(&self.tools.copy_first.clone()),
            cx,
        );
        panel_box("copy-panel", "Copy")
            .child(title(format!("Copy {}'s mix", data.name)))
            .child(fieldset("Copy to", ticks))
            .child(hint(
                "Channels, pans, mutes, solos and group levels. Never their master, their phone's cap or your limit and locks.",
            ))
            .when(!data.can_use, |panel| {
                panel.child(hint("Start the engine to copy a mix."))
            })
            .child(
                row()
                    .child({
                        let button = ToolButton::new("copy", "Copy", Look::Primary)
                            .inert(!ready)
                            .on_press(copy);
                        match first {
                            Some(first) => button.focus(first),
                            None => button,
                        }
                    })
                    .child(ToolButton::new("copy-close", "Close", Look::Plain).on_press(close)),
            )
            .into_any_element()
    }

    fn presets_panel(&mut self, data: &MixToolsView, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.weak_entity();
        let id = self.selected_preset(cx).or_else(|| {
            data.presets
                .iter()
                .flatten()
                .next()
                .map(|preset| preset.id.clone())
        });
        let name = data
            .presets
            .iter()
            .flatten()
            .find(|preset| Some(&preset.id) == id.as_ref())
            .map(|preset| preset.name.clone())
            .unwrap_or_default();
        let (apply_ready, delete) = {
            let tools = self.model.tools();
            (
                tools.apply_ready(id.as_deref()),
                id.as_deref()
                    .map(|id| tools.delete_state(id, Instant::now()))
                    .unwrap_or_default(),
            )
        };
        let act = |run: fn(&mut MixScreen, &mut Window, &mut Context<MixScreen>)| {
            let this = this.clone();
            move |window: &mut Window, cx: &mut App| {
                drop(this.update(cx, |this, cx| run(this, window, cx)));
            }
        };
        let save_ready = data.can_use;
        let mut panel = panel_box("presets-panel", "Presets")
            .child(title("Presets".to_owned()))
            .child(
                row()
                    .child(label(format!("Save {}'s mix as", data.name)))
                    .child(div().flex_1().min_w(px(160.0)).child(text_input(
                        &self.tools.save_name,
                        format!("Save {}'s mix as", data.name),
                    )))
                    .child(
                        ToolButton::new("save-preset", "Save preset", Look::Plain)
                            .inert(!save_ready)
                            .on_press(act(|this, _, cx| this.save(cx))),
                    ),
            )
            .when(!data.can_use, |panel| {
                panel.child(hint("Start the engine to save or apply presets."))
            });
        match &data.presets {
            None => {
                panel = panel.child(row().child(hint("The presets could not be read.")).child(
                    ToolButton::new("presets-load", "Read them again", Look::Plain).on_press(act(
                        |this, _, cx| {
                            this.model.tool(ToolRequest::LoadPresets);
                            cx.notify();
                        },
                    )),
                ));
            }
            Some(list) if list.is_empty() => {
                panel = panel.child(hint("No presets yet."));
            }
            Some(_) => {
                let ticks = self.ticks(TickList::Apply, data, None, cx);
                let apply_id = id.clone();
                let apply = {
                    let this = this.clone();
                    move |_: &mut Window, cx: &mut App| {
                        let id = apply_id.clone();
                        drop(this.update(cx, |this, cx| {
                            let request = this.model.tools().apply(id.as_deref());
                            this.run(request);
                            cx.notify();
                        }));
                    }
                };
                panel = panel
                    .child(
                        row().child(label("Preset".to_owned())).child(
                            div()
                                .w(px(220.0))
                                .child(select(&self.tools.preset, "Preset")),
                        ),
                    )
                    .child(fieldset("Apply to", ticks))
                    .child(
                        row().child(
                            ToolButton::new("apply-preset", "Apply", Look::Primary)
                                .named(format!("Apply {name}"))
                                .inert(!apply_ready)
                                .on_press(apply),
                        ),
                    )
                    .child(
                        row()
                            .child(label("Rename to".to_owned()))
                            .child(div().flex_1().min_w(px(160.0)).child(text_input(
                                &self.tools.rename_to,
                                format!("New name for {name}"),
                            )))
                            .child(
                                ToolButton::new("rename-preset", "Rename", Look::Plain)
                                    .on_press(act(|this, _, cx| this.rename(cx))),
                            ),
                    )
                    .child(self.delete_slot(id.clone(), &name, delete.armed, delete.ready, cx));
            }
        }
        panel
            .child(
                row().child(
                    ToolButton::new("presets-close", "Close", Look::Plain)
                        .on_press(act(|this, window, cx| this.escape(window, cx))),
                ),
            )
            .into_any_element()
    }

    /// Delete preset, or, once pressed, Keep (where Delete was), the question and the confirming
    /// Delete, which works only after a short pause.
    fn delete_slot(
        &self,
        id: Option<String>,
        name: &str,
        armed: bool,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(id) = id else {
            return div().into_any_element();
        };
        let this = cx.weak_entity();
        if !armed {
            return row()
                .child(
                    ToolButton::new("delete-preset", "Delete preset", Look::Danger)
                        .named(format!("Delete the preset {name}"))
                        .on_press(move |window, cx| {
                            let id = id.clone();
                            drop(this.update(cx, |this, cx| this.arm_delete(id, window, cx)));
                        }),
                )
                .into_any_element();
        }
        let keep = {
            let this = this.clone();
            move |_: &mut Window, cx: &mut App| {
                drop(this.update(cx, |this, cx| {
                    this.model.with_tools(|tools| tools.keep());
                    cx.notify();
                }));
            }
        };
        let confirm = move |_: &mut Window, cx: &mut App| {
            let id = id.clone();
            drop(this.update(cx, |this, cx| {
                let request = this
                    .model
                    .with_tools(|tools| tools.confirm_delete(&id, Instant::now()));
                this.run(request);
                cx.notify();
            }));
        };
        row()
            .child(
                ToolButton::new("keep-preset", "Keep", Look::Plain)
                    .focus(&self.tools.keep)
                    .on_press(keep),
            )
            .child(
                div()
                    .text_size(px(text::MD))
                    .child(format!("Delete {name}? No musician's mix changes.")),
            )
            .child(
                ToolButton::new("confirm-delete", "Delete", Look::Danger)
                    .inert(!ready)
                    .on_press(confirm),
            )
            .into_any_element()
    }
}
