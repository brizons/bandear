//! The link history chart (`link-health.js` `#drawChart`, `#showSeq`; inventory E section 8): the
//! panels, gap bands, lines and dots, the event lanes, the reconnect markers and the time axis,
//! drawn on a canvas from [health_layout]; a crosshair and a readout for the second under the
//! pointer or picked with the keys (N30, B6). The chart is an image to assistive technology, named
//! by the panel's title and described by its summary; the table holds every value.

use std::cell::Cell;
use std::rc::Rc;

use bandear_desktop_kit::a11y::seen;
use bandear_desktop_kit::gpui::{
    App, Bounds, Context, FocusHandle, Hsla, InteractiveElement, IntoElement, KeyDownEvent,
    MouseMoveEvent, ParentElement, PathBuilder, Pixels, Role, SharedString,
    StatefulInteractiveElement, Styled, TextAlign, TextRun, Window, canvas, div, fill, font, point,
    prelude::FluentBuilder, px, size,
};
use bandear_desktop_kit::theme::{FONT_MONO, FONT_SANS, color, colors, radius, text, weight};

use super::NetworkScreen;
use crate::model::link_chart::{Anchor, Layout, health_layout, nearest_index};
use crate::model::link_view::{PanelView, sample_tooltip};

/// What the chart keeps between frames: its focus, the second shown, its width as last laid out
/// and where it sits in the window.
pub struct ChartState {
    pub focus: FocusHandle,
    pub hover: Option<u64>,
    width: Rc<Cell<f32>>,
    origin: Rc<Cell<(f32, f32)>>,
}

impl ChartState {
    pub fn new(cx: &mut Context<NetworkScreen>) -> Self {
        Self {
            focus: cx.focus_handle(),
            hover: None,
            width: Rc::new(Cell::new(640.0)),
            origin: Rc::new(Cell::new((0.0, 0.0))),
        }
    }
}

/// The readout's width, for placing it on the side with room.
const READOUT_WIDTH: f32 = 170.0;

fn text_line(
    words: &str,
    x: f64,
    y: f64,
    anchor: Anchor,
    family: &'static str,
    size_px: f32,
    tone: Hsla,
    medium: bool,
    window: &mut Window,
    cx: &mut App,
    origin: bandear_desktop_kit::gpui::Point<Pixels>,
) {
    let mut face = font(family);
    if medium {
        face.weight = weight::MEDIUM;
    }
    let words = SharedString::from(words.to_owned());
    let run = TextRun {
        len: words.len(),
        font: face,
        color: tone,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line(words, px(size_px), &[run], None);
    let width = f64::from(f32::from(shaped.width));
    let left = match anchor {
        Anchor::Start => x,
        Anchor::Middle => x - width / 2.0,
        Anchor::End => x - width,
    };
    // An SVG text sits on its baseline; a line here is placed by its top.
    let top = y - f64::from(size_px) * 0.82;
    let at = point(origin.x + px(left as f32), origin.y + px(top as f32));
    drop(shaped.paint(at, px(size_px * 1.2), TextAlign::Left, None, window, cx));
}

fn stroke(
    points: &[(f64, f64)],
    width: f32,
    dashed: bool,
    tone: Hsla,
    origin: bandear_desktop_kit::gpui::Point<Pixels>,
    window: &mut Window,
) {
    let mut path = PathBuilder::stroke(px(width));
    if dashed {
        path = path.dash_array(&[px(4.0), px(3.0)]);
    }
    for (index, (x, y)) in points.iter().enumerate() {
        let at = point(origin.x + px(*x as f32), origin.y + px(*y as f32));
        if index == 0 {
            path.move_to(at);
        } else {
            path.line_to(at);
        }
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, tone);
    }
}

fn rect(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    tone: Hsla,
    round: f32,
    origin: bandear_desktop_kit::gpui::Point<Pixels>,
    window: &mut Window,
) {
    let bounds = Bounds::new(
        point(origin.x + px(x as f32), origin.y + px(y as f32)),
        size(px(w as f32), px(h as f32)),
    );
    window.paint_quad(fill(bounds, tone).corner_radii(px(round)));
}

/// Paints `layout` (the draw order of the web chart: per panel its title, ticks, gap bands, line
/// and dots, then the lanes, the reconnects and the axis labels), then the crosshair at `cross`.
fn paint(
    layout: &Layout,
    dense: bool,
    cross: Option<f64>,
    origin: bandear_desktop_kit::gpui::Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let plot = layout.plot;
    let right = plot.left + plot.width;
    for panel in &layout.panels {
        text_line(
            panel.title,
            plot.left,
            panel.title_y,
            Anchor::Start,
            FONT_SANS,
            text::XS,
            color(colors::TEXT_SECONDARY),
            true,
            window,
            cx,
            origin,
        );
        for (index, tick) in panel.ticks.iter().enumerate() {
            let tone = color(if index == 0 {
                colors::BORDER
            } else {
                colors::BORDER_SUBTLE
            });
            stroke(
                &[(plot.left, tick.y), (right, tick.y)],
                1.0,
                false,
                tone,
                origin,
                window,
            );
            text_line(
                &tick.label,
                plot.left - 8.0,
                tick.y + 4.0,
                Anchor::End,
                FONT_MONO,
                text::XXS,
                color(colors::TEXT_MUTED),
                false,
                window,
                cx,
                origin,
            );
        }
        for band in &layout.gaps {
            rect(
                band.x,
                panel.top,
                band.width,
                panel.height,
                color(colors::SURFACE_RAISED),
                0.0,
                origin,
                window,
            );
        }
        for line in &panel.lines {
            let points: Vec<(f64, f64)> = line.iter().map(|point| (point.x, point.y)).collect();
            stroke(
                &points,
                if dense { 1.25 } else { 2.0 },
                false,
                color(colors::BLUE),
                origin,
                window,
            );
        }
        for dot in &panel.dots {
            rect(
                dot.x - 5.0,
                dot.y - 5.0,
                10.0,
                10.0,
                color(colors::SURFACE),
                5.0,
                origin,
                window,
            );
            rect(
                dot.x - 4.0,
                dot.y - 4.0,
                8.0,
                8.0,
                color(colors::BLUE),
                4.0,
                origin,
                window,
            );
        }
    }
    for lane in &layout.lanes {
        let middle = lane.top + lane.height / 2.0;
        text_line(
            lane.label,
            plot.left - 8.0,
            middle + 4.0,
            Anchor::End,
            FONT_SANS,
            text::XS,
            color(colors::TEXT_MUTED),
            false,
            window,
            cx,
            origin,
        );
        stroke(
            &[(plot.left, middle), (right, middle)],
            1.0,
            false,
            color(colors::BORDER_SUBTLE),
            origin,
            window,
        );
        for mark in &lane.marks {
            rect(
                mark.x - 1.0,
                middle - 6.0,
                2.0,
                12.0,
                color(colors::TEXT_SECONDARY),
                1.0,
                origin,
                window,
            );
        }
    }
    for reconnect in &layout.reconnects {
        stroke(
            &[(reconnect.x, plot.top - 2.0), (reconnect.x, plot.bottom)],
            1.0,
            true,
            color(colors::TEXT_MUTED),
            origin,
            window,
        );
        if let Some(label) = reconnect.label {
            let x = if reconnect.anchor == Anchor::End {
                reconnect.x - 4.0
            } else {
                reconnect.x + 4.0
            };
            text_line(
                label,
                x,
                plot.top - 4.0,
                reconnect.anchor,
                FONT_SANS,
                text::XS,
                color(colors::TEXT_SECONDARY),
                true,
                window,
                cx,
                origin,
            );
        }
    }
    for tick in &layout.time_ticks {
        text_line(
            &tick.label,
            tick.x,
            plot.axis_y,
            tick.anchor,
            FONT_MONO,
            text::XXS,
            color(colors::TEXT_MUTED),
            false,
            window,
            cx,
            origin,
        );
    }
    if let Some(x) = cross {
        stroke(
            &[(x, plot.top), (x, plot.bottom)],
            1.0,
            false,
            color(colors::TEXT),
            origin,
            window,
        );
    }
}

impl NetworkScreen {
    /// The second shown moved `by` seconds (or to the newest with nothing shown yet).
    fn step(&mut self, panel: &PanelView, by: i64) {
        let samples = &panel.window.samples;
        let Some(newest) = samples.last() else {
            return;
        };
        let at = samples
            .iter()
            .position(|sample| Some(sample.seq) == self.chart.hover);
        self.chart.hover = Some(match at {
            None => newest.seq,
            Some(at) => {
                let next = (at as i64 + by).clamp(0, samples.len() as i64 - 1) as usize;
                samples[next].seq
            }
        });
    }

    fn on_chart_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(panel) = self.view.health.clone() else {
            return;
        };
        let samples = &panel.window.samples;
        match event.keystroke.key.as_str() {
            "left" => self.step(&panel, -1),
            "right" => self.step(&panel, 1),
            "pageup" => self.step(&panel, -10),
            "pagedown" => self.step(&panel, 10),
            "home" => self.chart.hover = samples.first().map(|sample| sample.seq),
            "end" => self.chart.hover = samples.last().map(|sample| sample.seq),
            "escape" => self.chart.hover = None,
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// The chart for `panel`, with its crosshair and readout.
    pub(super) fn render_chart(
        &self,
        panel: &PanelView,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let width = f64::from(self.chart.width.get().max(200.0));
        let layout = health_layout(
            &panel.window,
            width,
            panel.extras,
            panel.range_ms,
            panel.zone,
        );
        let dense = panel.window.samples.len() as f64 > layout.plot.width / 2.0;
        let shown = self
            .chart
            .hover
            .and_then(|seq| panel.window.samples.iter().find(|sample| sample.seq == seq))
            .cloned();
        let cross = shown
            .as_ref()
            .map(|sample| layout.x_of(sample.mono_ms as f64));
        let measured = Rc::clone(&self.chart.width);
        let placed = Rc::clone(&self.chart.origin);
        let painted = layout.clone();
        let hover_window = panel.window.clone();
        let hover_layout = layout.clone();
        let origin = Rc::clone(&self.chart.origin);
        div()
            .id("chart")
            .role(Role::Image)
            .aria_label(panel.title.clone())
            .aria_description(panel.description.clone())
            .track_focus(&self.chart.focus)
            .relative()
            .w_full()
            .h(px(layout.height as f32))
            .focus_visible(|style| style.border_1().border_color(color(colors::FOCUS)))
            .on_key_down(
                cx.listener(|this, event: &KeyDownEvent, _, cx| this.on_chart_key(event, cx)),
            )
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                let (left, _) = origin.get();
                let x = f64::from(f32::from(event.position.x) - left);
                let index = nearest_index(&hover_window, &hover_layout, x);
                let seq = index.map(|index| hover_window.samples[index].seq);
                if seq != this.chart.hover {
                    this.chart.hover = seq;
                    cx.notify();
                }
            }))
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                // Leaving hides the readout, unless the keys show it.
                if !hovered && !this.chart.focus.is_focused(window) {
                    this.chart.hover = None;
                    cx.notify();
                }
            }))
            .child(
                canvas(
                    move |bounds, window, _| {
                        let width = f32::from(bounds.size.width).round();
                        placed.set((f32::from(bounds.origin.x), f32::from(bounds.origin.y)));
                        if width > 0.0 && (width - measured.get()).abs() >= 1.0 {
                            measured.set(width);
                            window.refresh();
                        }
                        bounds.origin
                    },
                    move |_, origin, window, cx| paint(&painted, dense, cross, origin, window, cx),
                )
                .size_full(),
            )
            .when_some(shown.zip(cross), |chart, (sample, x)| {
                let tip = sample_tooltip(&sample, panel.extras, panel.zone);
                let x = x as f32;
                let left = if x + 12.0 + READOUT_WIDTH > layout.width as f32 {
                    (x - 12.0 - READOUT_WIDTH).max(0.0)
                } else {
                    x + 12.0
                };
                chart.child(
                    div()
                        .absolute()
                        .top(px(layout.plot.top as f32))
                        .left(px(left))
                        .min_w(px(150.0))
                        .w(px(READOUT_WIDTH))
                        .py(px(8.0))
                        .px(px(10.0))
                        .rounded(px(radius::MD))
                        .border_1()
                        .border_color(color(colors::BORDER))
                        .bg(color(colors::SURFACE_SUNKEN))
                        .shadow_lg()
                        .text_size(px(text::SM))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .gap(px(8.0))
                                .child(div().font_weight(weight::BOLD).child(seen(tip.title)))
                                .child(
                                    div()
                                        .font_family(FONT_MONO)
                                        .text_color(color(colors::TEXT_MUTED))
                                        .child(seen(tip.time)),
                                ),
                        )
                        .children(tip.lines.into_iter().map(|(name, value)| {
                            div()
                                .flex()
                                .justify_between()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_color(color(colors::TEXT_SECONDARY))
                                        .child(seen(name)),
                                )
                                .child(
                                    div()
                                        .font_family(FONT_MONO)
                                        .font_weight(weight::BOLD)
                                        .child(seen(value)),
                                )
                        }))
                        .when_some(tip.note, |box_, note| {
                            box_.child(
                                div()
                                    .text_color(color(colors::TEXT_MUTED))
                                    .child(seen(note)),
                            )
                        }),
                )
            })
    }

    /// The chart took the focus: the readout shows the newest second, if nothing shows yet.
    pub(super) fn chart_focused(&mut self, cx: &mut Context<Self>) {
        if self.chart.hover.is_none()
            && let Some(newest) = self
                .view
                .health
                .as_ref()
                .and_then(|panel| panel.window.samples.last())
        {
            self.chart.hover = Some(newest.seq);
            cx.notify();
        }
    }

    /// The chart lost the focus: the readout hides.
    pub(super) fn chart_blurred(&mut self, cx: &mut Context<Self>) {
        self.chart.hover = None;
        cx.notify();
    }
}
