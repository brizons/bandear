//! The strip's parts (`processing-screen.js`): the slider rows and On switches, the levels, the
//! gate and compressor cards with their reduction meters and the compressor's transfer curve, and
//! the effect sends.

use bandear_desktop_console::console::Console;
use bandear_desktop_console::types::{CompressorSettings, Effect};
use bandear_desktop_kit::a11y::seen;
use bandear_desktop_kit::gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, PathBuilder, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, canvas, div, point,
    prelude::FluentBuilder, px,
};
use bandear_desktop_kit::meters::{LevelMeter, ReductionMeter};
use bandear_desktop_kit::slider::parameter;
use bandear_desktop_kit::theme::{FONT_MONO, color, colors, radius, text, weight};

use super::ProcessingScreen;
use super::atoms::{checkbox, link};
use crate::fields::{
    COMPRESSOR_REDUCTION_SCALE_DB, Field, GATE_REDUCTION_SCALE_DB, Part, SLIDER_STEPS, enabled_of,
    field_position, field_value, transfer_label, transfer_points, value_of,
};
use crate::model::{EditorView, Model};
use crate::words::format_send;

/// The transfer curve's box, drawn at 96 x 96 px.
const TRANSFER_SIZE: f32 = 96.0;

/// A card of the strip editor, with its title.
pub(super) fn card(
    id: &'static str,
    title: &'static str,
) -> bandear_desktop_kit::gpui::Stateful<bandear_desktop_kit::gpui::Div> {
    div()
        .id(id)
        .role(Role::Region)
        .aria_label(title)
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(16.0))
        .rounded(px(radius::LG))
        .border_1()
        .border_color(color(colors::BORDER))
        .bg(color(colors::SURFACE))
}

/// A card's title (`h2`).
pub(super) fn title(id: &'static str, words: &'static str) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Heading)
        .aria_level(2)
        .aria_label(words)
        .text_size(px(text::LG))
        .font_weight(weight::BOLD)
        .child(seen(words))
}

/// The highest of the strip's sides in `list` (0-based by input).
fn peak<T: Copy + Into<f64>>(list: &[T], sides: &[usize]) -> f64 {
    sides
        .iter()
        .filter_map(|side| list.get(side - 1).copied().map(Into::into))
        .fold(0.0, f64::max)
}

/// The levels of the open strip: its input and its processed signal.
pub(super) fn levels(view: &EditorView, console: &Console) -> AnyElement {
    let meters = console.meters();
    let (input, processed) = meters.as_ref().map_or((0.0, 0.0), |meters| {
        let processed = if meters.processed.is_empty() {
            &meters.channels
        } else {
            &meters.processed
        };
        (
            peak(&meters.channels, &view.sides),
            peak(processed, &view.sides),
        )
    });
    let level = |id: &'static str, words: &'static str, value: f64, name: &'static str| {
        div()
            .flex()
            .flex_1()
            .min_w(px(240.0))
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .w(px(72.0))
                    .text_size(px(text::SM))
                    .text_color(color(colors::TEXT_SECONDARY))
                    .child(words),
            )
            .child(
                div()
                    .flex_1()
                    .child(LevelMeter::new(id, value, name).horizontal()),
            )
    };
    div()
        .id("processing-levels")
        .role(Role::Region)
        .aria_label("Levels")
        .flex()
        .flex_wrap()
        .gap(px(16.0))
        .px(px(14.0))
        .py(px(10.0))
        .rounded(px(radius::LG))
        .border_1()
        .border_color(color(colors::BORDER))
        .bg(color(colors::SURFACE))
        .child(level("processing-input", "Input", input, "Input level"))
        .child(level(
            "processing-processed",
            "Processed",
            processed,
            "Processed level",
        ))
        .into_any_element()
}

impl ProcessingScreen {
    /// A slider row of `part`'s `field`: its label, the slider and its value in words; in a band
    /// panel (`stacked`) the label and value share a line with the slider below.
    pub(super) fn field_row(
        &self,
        view: &EditorView,
        part: Part,
        field: &'static Field,
        stacked: bool,
    ) -> AnyElement {
        let value = value_of(&view.settings, part, field.key);
        let words = (field.format)(value);
        let id = SharedString::from(format!(
            "processing-{}-{}",
            part.name().to_lowercase().replace(' ', "-"),
            field.label.to_lowercase()
        ));
        let name = format!("{} {}", part.name(), field.label.to_lowercase());
        let model = self.model.clone();
        let slider = parameter(
            id.clone(),
            SLIDER_STEPS,
            f64::from(field_position(field, value)),
            field_position(field, value),
            name,
            words.clone(),
        )
        .describe(move |position| (field.format)(field_value(field, position)).into())
        .disabled(view.bypassed)
        .on_input(move |position, _, _| {
            let edit = model.edit_number(part, field.key, field_value(field, position));
            model.console().spawn(edit);
            if part.is_eq() {
                model.request_curve();
            }
        });
        let label = div()
            .w(px(82.0))
            .flex_none()
            .text_size(px(text::SM))
            .text_color(color(colors::TEXT_SECONDARY))
            .child(seen(field.label));
        let output = div()
            .w(px(72.0))
            .flex_none()
            .font_family(FONT_MONO)
            .text_size(px(text::SM))
            .text_color(color(colors::TEXT))
            .child(seen(words));
        if stacked {
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(div().flex().justify_between().child(label).child(output))
                .child(slider)
                .into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(label)
                .child(div().flex_1().child(slider))
                .child(output)
                .into_any_element()
        }
    }

    /// The On switch of `part`.
    pub(super) fn on_switch(&self, view: &EditorView, part: Part) -> AnyElement {
        let on = enabled_of(&view.settings, part);
        let model = self.model.clone();
        let bypassed = view.bypassed;
        checkbox(
            SharedString::from(format!(
                "processing-{}-on",
                part.name().to_lowercase().replace(' ', "-")
            )),
            on,
            format!("{} on", part.name()),
            "On",
            None,
            move |on, _, _| {
                if bypassed {
                    return;
                }
                let edit = model.edit_enabled(part, on);
                model.console().spawn(edit);
                if part.is_eq() {
                    model.request_curve();
                }
            },
        )
    }

    /// The gate and compressor cards, beside each other when there is room.
    pub(super) fn dynamics(&self, view: &EditorView, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let _ = cx;
        let meters = self.model.console().meters();
        let reduction = |list: Option<&Vec<f32>>| list.map_or(0.0, |list| peak(list, &view.sides));
        let gate_db = reduction(meters.as_ref().map(|meters| &meters.gate_reduction_db));
        let compressor_db = reduction(
            meters
                .as_ref()
                .map(|meters| &meters.compressor_reduction_db),
        );
        let head = |id: &'static str, words: &'static str, part: Part| {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(title(id, words))
                .child(div().flex_1())
                .child(self.on_switch(view, part))
        };
        let reduction_row = |id: &'static str, db: f64, scale: f64, name: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .w(px(82.0))
                        .text_size(px(text::SM))
                        .text_color(color(colors::TEXT_SECONDARY))
                        .child("Reduction"),
                )
                .child(
                    div()
                        .flex_1()
                        .child(ReductionMeter::new(id, db, scale, name)),
                )
        };
        let rows = |part: Part| -> Vec<AnyElement> {
            part.fields()
                .iter()
                .map(|field| self.field_row(view, part, field, false))
                .collect()
        };
        let gate = card("processing-gate", "Gate")
            .flex_1()
            .min_w(px(320.0))
            .child(head("processing-gate-title", "Gate", Part::Gate))
            .children(rows(Part::Gate))
            .child(reduction_row(
                "processing-gate-reduction",
                gate_db,
                GATE_REDUCTION_SCALE_DB,
                "Gate gain reduction",
            ));
        let compressor = card("processing-compressor", "Compressor")
            .flex_1()
            .min_w(px(320.0))
            .child(head(
                "processing-compressor-title",
                "Compressor",
                Part::Compressor,
            ))
            .children(rows(Part::Compressor))
            .child(reduction_row(
                "processing-compressor-reduction",
                compressor_db,
                COMPRESSOR_REDUCTION_SCALE_DB,
                "Compressor gain reduction",
            ))
            .child(transfer(&view.settings.compressor));
        vec![gate.into_any_element(), compressor.into_any_element()]
    }

    /// The effect sends of the open strip (outside the processing controls: they stay enabled
    /// while bypassed), and the link to the Effects screen.
    pub(super) fn sends(&self, view: &EditorView, cx: &mut Context<Self>) -> AnyElement {
        let _ = cx;
        let hub = self.hub.clone();
        let send = |effect: Effect, words: &'static str, level: u8, model: Model| {
            let id = SharedString::from(format!("processing-send-{}", words.to_lowercase()));
            let text = format_send(level);
            let slider = parameter(
                id,
                100,
                f64::from(level),
                0,
                format!("{} {} send", view.label(), words.to_lowercase()),
                text.clone(),
            )
            .describe(|level| format_send(level.clamp(0, 100) as u8).into())
            .on_input(move |level, _, _| {
                let send = model.edit_send(effect, f64::from(level));
                model.console().spawn(send);
            });
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .w(px(82.0))
                        .text_size(px(text::SM))
                        .text_color(color(colors::TEXT_SECONDARY))
                        .child(words),
                )
                .child(div().flex_1().child(slider))
                .child(
                    div()
                        .w(px(72.0))
                        .font_family(FONT_MONO)
                        .text_size(px(text::SM))
                        .child(seen(text)),
                )
        };
        card("processing-sends", "Effect sends")
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(title("processing-sends-title", "Effect sends"))
                    .child(div().flex_1())
                    .child(link(
                        "processing-effects-link",
                        "Effects\u{2026}",
                        "Effects\u{2026}",
                        None,
                        move |_, cx| {
                            hub.update(cx, |hub, cx| {
                                hub.navigate(
                                    bandear_desktop_console::router::Route::from(
                                        bandear_desktop_console::router::Section::Effects,
                                    ),
                                    cx,
                                )
                            });
                        },
                    )),
            )
            .child(
                div()
                    .text_size(px(text::SM))
                    .text_color(color(colors::TEXT_SECONDARY))
                    .child(
                        "To the shared reverb and delay, after the strip and before every mix's \
                         fader. Each mix returns them on its master strip.",
                    ),
            )
            .child(send(
                Effect::Reverb,
                "Reverb",
                view.sends.reverb,
                self.model.clone(),
            ))
            .child(send(
                Effect::Delay,
                "Delay",
                view.sends.delay,
                self.model.clone(),
            ))
            .into_any_element()
    }

    /// The processing controls: the equaliser, then the gate and the compressor; dimmed and
    /// inert while bypassed.
    pub(super) fn sections(
        &self,
        view: &EditorView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("processing-sections")
            .role(Role::Group)
            .aria_label("Processing controls")
            .flex()
            .flex_wrap()
            .gap(px(16.0))
            .when(view.bypassed, |sections| sections.opacity(0.7))
            .child(self.equaliser(view, window, cx))
            .children(self.dynamics(view, cx))
            .into_any_element()
    }
}

/// The compressor's static curve over a dashed unity line, with its caption.
fn transfer(compressor: &CompressorSettings) -> AnyElement {
    let points = transfer_points(compressor, f64::from(TRANSFER_SIZE), 60);
    let curve = canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let at = |x: f64, y: f64| {
                point(
                    bounds.origin.x + px(x as f32),
                    bounds.origin.y + px(y as f32),
                )
            };
            let mut unity = PathBuilder::stroke(px(1.0)).dash_array(&[px(3.0), px(3.0)]);
            unity.move_to(at(0.0, f64::from(TRANSFER_SIZE)));
            unity.line_to(at(f64::from(TRANSFER_SIZE), 0.0));
            if let Ok(path) = unity.build() {
                window.paint_path(path, color(colors::BORDER));
            }
            let mut line = PathBuilder::stroke(px(2.0));
            for (index, (x, y)) in points.iter().enumerate() {
                if index == 0 {
                    line.move_to(at(*x, *y));
                } else {
                    line.line_to(at(*x, *y));
                }
            }
            if let Ok(path) = line.build() {
                window.paint_path(path, color(colors::ACCENT));
            }
        },
    )
    .size(px(TRANSFER_SIZE));
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(
            div()
                .id("processing-transfer")
                .role(Role::Image)
                .aria_label(transfer_label(compressor))
                .size(px(TRANSFER_SIZE))
                .rounded(px(radius::SM))
                .bg(color(colors::SURFACE_SUNKEN))
                .child(curve),
        )
        .child(
            div()
                .text_size(px(text::XS))
                .text_color(color(colors::TEXT_MUTED))
                .child("Output against input, \u{2212}60 to 0 dB"),
        )
        .into_any_element()
}
