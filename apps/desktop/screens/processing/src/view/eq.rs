//! The equaliser card (`eq-graph.js`): the response graph with its grid, curve and five handles
//! (dragged, wheeled for Q, or moved with the keys), then a panel per part with its On switch and
//! sliders.

use bandear_desktop_kit::a11y::seen;
use bandear_desktop_kit::gpui::{
    AnyElement, Bounds, Context, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, PathBuilder, Pixels, Role,
    ScrollEvent, SharedString, StatefulInteractiveElement, Styled, Window, canvas, div, point,
    prelude::FluentBuilder, px, relative,
};
use bandear_desktop_kit::theme::{FONT_MONO, color, colors, radius, text, weight};
use bandear_desktop_kit::tooltip::tooltip;

use super::ProcessingScreen;
use super::parts::{card, title};
use crate::fields::Part;
use crate::graph::{
    BAND_NAMES, GRAPH_HEIGHT, GRAPH_WIDTH, HandleKey, Target, band_handles, curve_description,
    curve_points, drag_handle, grid_lines, handle_key, nudge_q,
};
use crate::model::EditorView;

/// The graph's height on screen; its width is the card's.
const GRAPH_PX: f32 = 200.0;

const HANDLE_TITLE: &str = "Drag to move; wheel or Page Up and Page Down for Q; arrows move it \
                            (Shift for bigger steps), Home sets 0 dB, Enter turns it on or off";

impl ProcessingScreen {
    /// Moves the handle `target` to (`x`, `y`) in the graph's box, sends the strip and asks for
    /// the curve.
    fn drag_to(&mut self, target: Target, x: f64, y: f64) {
        let eq = drag_handle(
            &self.model.shown_eq(),
            target,
            x,
            y,
            GRAPH_WIDTH,
            GRAPH_HEIGHT,
        );
        let edit = self.model.edit_eq(eq);
        self.model.console().spawn(edit);
        self.model.request_curve();
    }

    /// The equaliser card, spanning the whole width.
    pub(super) fn equaliser(
        &self,
        view: &EditorView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let _ = window;
        let panels = Part::ALL.iter().filter(|part| part.is_eq()).map(|part| {
            let name = match part {
                Part::Band(index) => BAND_NAMES[*index],
                _ => "High-pass",
            };
            div()
                .flex_1()
                .min_w(px(140.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .p(px(10.0))
                .rounded(px(radius::MD))
                .border_1()
                .border_color(color(colors::BORDER_SUBTLE))
                .bg(color(colors::SURFACE_SUNKEN))
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "processing-{}-title",
                            name.to_lowercase().replace(' ', "-")
                        )))
                        .role(Role::Heading)
                        .aria_level(3)
                        .aria_label(name)
                        .text_size(px(text::MD))
                        .font_weight(weight::BOLD)
                        .child(seen(name)),
                )
                .child(self.on_switch(view, *part))
                .children(
                    part.fields()
                        .iter()
                        .map(|field| self.field_row(view, *part, field, true)),
                )
        });
        card("processing-eq", "Equaliser")
            .w_full()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_baseline()
                    .gap_x(px(12.0))
                    .child(title("processing-eq-title", "Equaliser"))
                    .child(
                        div()
                            .text_size(px(text::SM))
                            .text_color(color(colors::TEXT_MUTED))
                            .child(
                                "High-pass, then a low shelf, two peaks and a high shelf. Drag a \
                                 handle; wheel for Q.",
                            ),
                    ),
            )
            .child(self.graph(view, cx))
            .child(div().flex().flex_wrap().gap(px(10.0)).children(panels))
            .into_any_element()
    }

    /// The response graph: the grid, the 0 dB line, the curve, and the handles over them.
    fn graph(&self, view: &EditorView, cx: &mut Context<Self>) -> AnyElement {
        let eq = view.settings.eq;
        let inert = view.bypassed;
        let points = curve_points(&view.curve.points, GRAPH_WIDTH, GRAPH_HEIGHT);
        let (freqs, gains) = grid_lines(GRAPH_WIDTH, GRAPH_HEIGHT);
        let lines: Vec<f64> = freqs.iter().map(|line| line.at).collect();
        let levels: Vec<f64> = gains.iter().map(|line| line.at).collect();
        let kept = self.graph.clone();
        let dragging = self.dragging.filter(|_| !inert);
        let screen = cx.entity().downgrade();
        let drawing = canvas(
            move |bounds, _, _| bounds,
            move |bounds: Bounds<Pixels>, _, window, _| {
                kept.set(bounds);
                let (width, height) = (f64::from(bounds.size.width), f64::from(bounds.size.height));
                let at = |x: f64, y: f64| {
                    point(
                        bounds.origin.x + px((x / GRAPH_WIDTH * width) as f32),
                        bounds.origin.y + px((y / GRAPH_HEIGHT * height) as f32),
                    )
                };
                let stroke = |from: (f64, f64), to: (f64, f64), weight: f32| {
                    let mut line = PathBuilder::stroke(px(weight));
                    line.move_to(at(from.0, from.1));
                    line.line_to(at(to.0, to.1));
                    line.build().ok()
                };
                for x in &lines {
                    if let Some(path) = stroke((*x, 0.0), (*x, GRAPH_HEIGHT), 1.0) {
                        window.paint_path(path, color(colors::BORDER_SUBTLE));
                    }
                }
                for y in &levels {
                    if let Some(path) = stroke((0.0, *y), (GRAPH_WIDTH, *y), 1.0) {
                        window.paint_path(path, color(colors::BORDER_SUBTLE));
                    }
                }
                if let Some(path) = stroke(
                    (0.0, GRAPH_HEIGHT / 2.0),
                    (GRAPH_WIDTH, GRAPH_HEIGHT / 2.0),
                    1.0,
                ) {
                    window.paint_path(path, color(colors::BORDER));
                }
                let mut curve = PathBuilder::stroke(px(2.0));
                for (index, (x, y)) in points.iter().enumerate() {
                    if index == 0 {
                        curve.move_to(at(*x, *y));
                    } else {
                        curve.line_to(at(*x, *y));
                    }
                }
                if let Ok(path) = curve.build() {
                    window.paint_path(path, color(colors::ACCENT));
                }
                // While a handle is dragged the pointer is followed anywhere in the window until
                // it lets go, as the web console's pointer capture did.
                let Some(target) = dragging else {
                    return;
                };
                let follow = screen.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase != bandear_desktop_kit::gpui::DispatchPhase::Bubble {
                        return;
                    }
                    let x = f64::from(event.position.x - bounds.origin.x) / width * GRAPH_WIDTH;
                    let y = f64::from(event.position.y - bounds.origin.y) / height * GRAPH_HEIGHT;
                    drop(follow.update(cx, |this, cx| {
                        this.drag_to(target, x, y);
                        cx.notify();
                    }));
                });
                let release = screen.clone();
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase != bandear_desktop_kit::gpui::DispatchPhase::Bubble {
                        return;
                    }
                    drop(release.update(cx, |this, cx| {
                        this.dragging = None;
                        cx.notify();
                    }));
                });
            },
        )
        .absolute()
        .size_full();
        let labels_x = freqs.iter().map(|line| {
            div()
                .absolute()
                .bottom(px(2.0))
                .left(relative((line.at / GRAPH_WIDTH) as f32))
                .ml(px(-20.0))
                .w(px(40.0))
                .flex()
                .justify_center()
                .font_family(FONT_MONO)
                .text_size(px(text::XXS))
                .text_color(color(colors::TEXT_MUTED))
                .child(seen(line.label.clone()))
        });
        let labels_y = gains.iter().filter(|line| line.label != "0").map(|line| {
            div()
                .absolute()
                .left(px(4.0))
                .top(relative((line.at / GRAPH_HEIGHT) as f32))
                .mt(px(-14.0))
                .font_family(FONT_MONO)
                .text_size(px(text::XXS))
                .text_color(color(colors::TEXT_MUTED))
                .child(seen(line.label.clone()))
        });
        let handles = band_handles(&eq, GRAPH_WIDTH, GRAPH_HEIGHT)
            .into_iter()
            .enumerate()
            .map(|(position, handle)| {
                let target = handle.target;
                let focus = self.handles[position].clone();
                let enabled = handle.enabled;
                div()
                    .id(SharedString::from(format!(
                        "processing-handle-{}",
                        target.mark()
                    )))
                    .role(Role::Button)
                    .aria_label(handle.label.clone())
                    .when(inert, |handle| {
                        handle.aria_description("Unavailable while bypassed")
                    })
                    .track_focus(&focus)
                    .absolute()
                    .left(relative((handle.x / GRAPH_WIDTH) as f32))
                    .top(relative((handle.y / GRAPH_HEIGHT) as f32))
                    .ml(px(-14.0))
                    .mt(px(-14.0))
                    .size(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
                    .when(!inert, |element| element.cursor_grab())
                    .tooltip(|window, cx| tooltip(HANDLE_TITLE.into(), window, cx))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                            if this.model.view().bypassed {
                                return;
                            }
                            window.focus(&this.handles[position], cx);
                            this.dragging = Some(target);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_scroll(cx.listener(move |this, event: &ScrollEvent, _, cx| {
                        if this.model.view().bypassed {
                            return;
                        }
                        let dy = event.delta.pixel_delta(px(16.0)).y;
                        let steps = if dy < px(0.0) {
                            1
                        } else if dy > px(0.0) {
                            -1
                        } else {
                            0
                        };
                        if steps != 0 {
                            let eq = nudge_q(&this.model.shown_eq(), target, steps);
                            let edit = this.model.edit_eq(eq);
                            this.model.console().spawn(edit);
                            this.model.request_curve();
                        }
                        cx.stop_propagation();
                    }))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        if this.model.view().bypassed {
                            return;
                        }
                        let Some(key) = HandleKey::from_gpui(&event.keystroke.key) else {
                            return;
                        };
                        let shift = event.keystroke.modifiers.shift;
                        if let Some(eq) = handle_key(&this.model.shown_eq(), target, key, shift) {
                            let edit = this.model.edit_eq(eq);
                            this.model.console().spawn(edit);
                            this.model.request_curve();
                        }
                        cx.stop_propagation();
                    }))
                    .child(
                        div()
                            .size(px(18.0))
                            .rounded_full()
                            .border_2()
                            .border_color(color(if enabled {
                                colors::TEXT
                            } else {
                                colors::TEXT_MUTED
                            }))
                            .when(!enabled, |dot| dot.border_dashed())
                            .bg(color(colors::SURFACE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .font_family(FONT_MONO)
                            .text_size(px(9.0))
                            .font_weight(weight::BOLD)
                            .text_color(color(colors::TEXT))
                            .child(seen(target.mark())),
                    )
            });
        div()
            .id("processing-graph")
            .role(Role::Image)
            .aria_label("Equaliser response curve")
            .aria_description(curve_description(&eq))
            .relative()
            .w_full()
            .h(px(GRAPH_PX))
            .rounded(px(radius::MD))
            .border_1()
            .border_color(color(colors::BORDER))
            .bg(color(colors::SURFACE_SUNKEN))
            .child(drawing)
            .children(labels_x)
            .children(labels_y)
            .child(
                div()
                    .id("processing-handles")
                    .role(Role::Group)
                    .aria_label("Equaliser handles")
                    .absolute()
                    .size_full()
                    .children(handles),
            )
            .into_any_element()
    }
}
