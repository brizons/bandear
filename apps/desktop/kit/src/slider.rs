//! The sliders: a fader (vertical, 0-100), a pan (horizontal, with its centre detent) and a
//! parameter slider (horizontal over any number of positions, with its value written beside it).
//!
//! All three share one element over the console's [SliderGesture] (C39): a press on the track
//! jumps there, a press on the thumb keeps the grab point, a drag follows the pointer anywhere in
//! the window until it lets go, a double-click puts the slider back to its reset value, and the
//! arrows, Page Up/Down, Home and End step it (V7). `on_input` hears every value the gesture goes
//! through, `on_change` the one it ends on (the one to send). A value the engine sends during a drag
//! waits until the drag ends. Each is a `Slider` to assistive technology, with its label, its
//! value in words and its numbers.

use std::rc::Rc;

use bandear_desktop_console::slider::{SliderEvent, SliderGesture};
use bandear_desktop_console::values::{FADER, PAN, Range, SliderKey, pointer_fraction};
use gpui::{
    AnyElement, App, Bounds, CursorStyle, DispatchPhase, ElementId, FocusHandle,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Orientation, ParentElement, Pixels, Point, RenderOnce, Role, SharedString,
    StatefulInteractiveElement, Styled, Window, canvas, div, prelude::FluentBuilder, px, relative,
};

use crate::theme::{FONT_MONO, color, colors, radius, text, thumb_shadow};

/// What a slider tells its owner: the value, as the gesture reports it.
pub type OnValue = Rc<dyn Fn(i32, &mut Window, &mut App)>;

/// The direction a slider moves in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Bottom to top, as a fader.
    Vertical,
    /// Left to right, as a pan.
    Horizontal,
}

/// A slider's state between frames: its gesture, where its track was drawn and its focus.
struct SliderState {
    gesture: SliderGesture,
    bounds: Bounds<Pixels>,
    focus: FocusHandle,
}

/// The shared slider element: the interaction, the accessibility node, and a look drawn from the
/// value shown and where it sits (0 to 1).
#[derive(IntoElement)]
pub struct Slider {
    id: ElementId,
    range: Range,
    value: f64,
    axis: Axis,
    detent: bool,
    thumb: f32,
    label: SharedString,
    value_text: Option<Rc<dyn Fn(i32) -> SharedString>>,
    disabled: bool,
    on_input: Option<OnValue>,
    on_change: Option<OnValue>,
    look: Box<dyn FnOnce(i32, f64) -> AnyElement>,
}

impl Slider {
    /// A slider over `range` at `value` (the engine's), labelled `label`, drawn by `look` from the
    /// value shown and its fraction. `thumb` is the thumb's length along the track, which the
    /// track's ends leave room for.
    pub fn new(
        id: impl Into<ElementId>,
        range: Range,
        value: f64,
        axis: Axis,
        thumb: f32,
        label: impl Into<SharedString>,
        look: impl FnOnce(i32, f64) -> AnyElement + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            range,
            value,
            axis,
            detent: false,
            thumb,
            label: label.into(),
            value_text: None,
            disabled: false,
            on_input: None,
            on_change: None,
            look: Box::new(look),
        }
    }

    /// Drags land on the range's centre within its detent, as a pan's do.
    pub fn detent(mut self) -> Self {
        self.detent = true;
        self
    }

    /// The value in words for assistive technology (`aria-valuetext`), the same whatever the
    /// value shown.
    pub fn value_text(mut self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.value_text = Some(Rc::new(move |_| text.clone()));
        self
    }

    /// The value shown in words for assistive technology (`aria-valuetext`), as `describe` words
    /// it; it follows a drag.
    pub fn describe(mut self, describe: impl Fn(i32) -> SharedString + 'static) -> Self {
        self.value_text = Some(Rc::new(describe));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Hears every value the gesture goes through.
    pub fn on_input(mut self, listener: impl Fn(i32, &mut Window, &mut App) + 'static) -> Self {
        self.on_input = Some(Rc::new(listener));
        self
    }

    /// Hears the value a gesture ends on.
    pub fn on_change(mut self, listener: impl Fn(i32, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(listener));
        self
    }
}

/// Where `position` sits along the track drawn in `bounds`, the thumb's half length in from each
/// end, from 0 to 1.
fn fraction_at(axis: Axis, bounds: Bounds<Pixels>, thumb: f32, position: Point<Pixels>) -> f64 {
    let half = f64::from(thumb) / 2.0;
    match axis {
        Axis::Vertical => pointer_fraction(
            f64::from(f32::from(position.y)),
            f64::from(f32::from(bounds.top())) + half,
            f64::from(f32::from(bounds.size.height)) - 2.0 * half,
            true,
        ),
        Axis::Horizontal => pointer_fraction(
            f64::from(f32::from(position.x)),
            f64::from(f32::from(bounds.left())) + half,
            f64::from(f32::from(bounds.size.width)) - 2.0 * half,
            false,
        ),
    }
}

/// Whether `position` is on the thumb at `fraction` along the track.
fn on_thumb(
    axis: Axis,
    bounds: Bounds<Pixels>,
    thumb: f32,
    fraction: f64,
    position: Point<Pixels>,
) -> bool {
    let half = thumb / 2.0;
    let (start, length, at) = match axis {
        Axis::Vertical => (
            f32::from(bounds.bottom()) - half,
            -(f32::from(bounds.size.height) - thumb),
            f32::from(position.y),
        ),
        Axis::Horizontal => (
            f32::from(bounds.left()) + half,
            f32::from(bounds.size.width) - thumb,
            f32::from(position.x),
        ),
    };
    let centre = start + length * fraction as f32;
    (at - centre).abs() <= half
}

/// Tells the owner what the gesture reported.
fn report(
    events: Vec<SliderEvent>,
    on_input: &Option<OnValue>,
    on_change: &Option<OnValue>,
    window: &mut Window,
    cx: &mut App,
) {
    for event in events {
        match event {
            SliderEvent::Input(value) => {
                if let Some(listener) = on_input {
                    listener(value, window, cx);
                }
            }
            SliderEvent::Change(value) => {
                if let Some(listener) = on_change {
                    listener(value, window, cx);
                }
            }
        }
    }
}

impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Slider {
            id,
            range,
            value,
            axis,
            detent,
            thumb,
            label,
            value_text,
            disabled,
            on_input,
            on_change,
            look,
        } = self;
        let state = window.use_keyed_state(id.clone(), cx, |_, cx| {
            let gesture = SliderGesture::new(range, value);
            SliderState {
                gesture: if detent {
                    gesture.with_detent()
                } else {
                    gesture
                },
                bounds: Bounds::default(),
                focus: cx.focus_handle(),
            }
        });
        // The engine's value: shown at once, or once a drag ends.
        state.update(cx, |state, _| {
            state.gesture.set(value);
        });
        let (shown, fraction, focus, dragging) = {
            let state = state.read(cx);
            let shown = state.gesture.value();
            (
                shown,
                state.gesture.fraction_of(shown),
                state.focus.clone(),
                state.gesture.dragging(),
            )
        };
        let text = match &value_text {
            Some(describe) => describe(shown),
            None => SharedString::from(shown.to_string()),
        };

        let press = {
            let state = state.clone();
            let (on_input, on_change) = (on_input.clone(), on_change.clone());
            move |event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                if disabled {
                    return;
                }
                let events = state.update(cx, |state, cx| {
                    window.focus(&state.focus, cx);
                    let events = if event.click_count >= 2 {
                        state.gesture.reset(range.reset)
                    } else {
                        let at = state.gesture.fraction_of(state.gesture.value());
                        let grabbed = on_thumb(axis, state.bounds, thumb, at, event.position);
                        let fraction = fraction_at(axis, state.bounds, thumb, event.position);
                        state.gesture.press(fraction, grabbed)
                    };
                    cx.notify();
                    events
                });
                report(events, &on_input, &on_change, window, cx);
            }
        };
        let key = {
            let state = state.clone();
            let (on_input, on_change) = (on_input.clone(), on_change.clone());
            move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                if disabled {
                    return;
                }
                let Some(key) = SliderKey::from_gpui(&event.keystroke.key) else {
                    return;
                };
                cx.stop_propagation();
                let events = state.update(cx, |state, cx| {
                    cx.notify();
                    state.gesture.key(key)
                });
                report(events, &on_input, &on_change, window, cx);
            }
        };
        // While a drag goes on, the pointer is followed anywhere in the window until it lets go,
        // as the web console's pointer capture did.
        let track = {
            let state = state.clone();
            canvas(
                move |bounds, _, _| bounds,
                move |bounds, _, window, cx| {
                    state.update(cx, |state, _| state.bounds = bounds);
                    if !dragging {
                        return;
                    }
                    {
                        let state = state.clone();
                        let on_input = on_input.clone();
                        let on_change = on_change.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase != DispatchPhase::Bubble {
                                return;
                            }
                            let events = state.update(cx, |state, cx| {
                                let fraction =
                                    fraction_at(axis, state.bounds, thumb, event.position);
                                cx.notify();
                                state.gesture.move_to(fraction)
                            });
                            report(events, &on_input, &on_change, window, cx);
                        });
                    }
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                        if phase != DispatchPhase::Bubble {
                            return;
                        }
                        let events = state.update(cx, |state, cx| {
                            cx.notify();
                            state.gesture.release()
                        });
                        report(events, &on_input, &on_change, window, cx);
                    });
                },
            )
            .absolute()
            .size_full()
        };

        div()
            .id(id)
            .role(Role::Slider)
            .aria_label(label)
            .aria_value(text)
            .aria_numeric_value(f64::from(shown))
            .aria_min_numeric_value(f64::from(range.min))
            .aria_max_numeric_value(f64::from(range.max))
            .aria_numeric_value_step(f64::from(range.step))
            .aria_orientation(match axis {
                Axis::Vertical => Orientation::Vertical,
                Axis::Horizontal => Orientation::Horizontal,
            })
            .track_focus(&focus.tab_stop(!disabled))
            .relative()
            .rounded(px(radius::SM))
            .focus_visible(|style| style.border_2().border_color(color(colors::FOCUS)))
            .when(disabled, |slider| slider.opacity(0.5))
            .when(!disabled, |slider| {
                slider.cursor(match axis {
                    Axis::Vertical => CursorStyle::ResizeUpDown,
                    Axis::Horizontal => CursorStyle::ResizeLeftRight,
                })
            })
            .on_mouse_down(MouseButton::Left, press)
            .on_key_down(key)
            .child(look(shown, fraction))
            .child(track)
    }
}

/// The thumb a fader's track leaves room for at each end.
pub const FADER_THUMB: f32 = 20.0;
/// A pan's thumb.
pub const PAN_THUMB: f32 = 6.0;

/// A fader's fill: the default grey, the accent (a master), or dimmed (muted).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaderTone {
    Default,
    Accent,
    Muted,
}

/// A fader: a vertical slider 0-100, 40 px wide (32 compact), unity at the top and a double-click
/// back to unity; its rail, its fill up to the value and its thumb with a centre line.
pub fn fader(
    id: impl Into<ElementId>,
    value: f64,
    label: impl Into<SharedString>,
    tone: FaderTone,
    compact: bool,
) -> Slider {
    let fill = match tone {
        FaderTone::Default => colors::FADER_FILL,
        FaderTone::Accent => colors::ACCENT,
        FaderTone::Muted => colors::FADER_FILL_MUTED,
    };
    let width = if compact { crate::theme::TARGET } else { 40.0 };
    Slider::new(
        id,
        FADER,
        value,
        Axis::Vertical,
        FADER_THUMB,
        label,
        move |_, fraction| {
            let travel = relative(fraction as f32);
            div()
                .w(px(width))
                .h_full()
                .min_h(px(160.0))
                .relative()
                .child(
                    // The rail, inset by half a thumb at each end.
                    div()
                        .absolute()
                        .left(relative(0.5))
                        .ml(px(-3.0))
                        .w(px(6.0))
                        .top(px(FADER_THUMB / 2.0))
                        .bottom(px(FADER_THUMB / 2.0))
                        .rounded(px(3.0))
                        .bg(color(colors::TRACK))
                        .child(
                            div()
                                .absolute()
                                .bottom_0()
                                .w_full()
                                .h(travel)
                                .rounded(px(3.0))
                                .bg(color(fill)),
                        ),
                )
                .child(
                    // The thumb travels the rail's length: its bottom edge goes from 0 to the rail's.
                    div()
                        .absolute()
                        .top(px(FADER_THUMB / 2.0))
                        .bottom(px(FADER_THUMB / 2.0))
                        .left_0()
                        .right_0()
                        .child(
                            div()
                                .absolute()
                                .left(px(3.0))
                                .right(px(3.0))
                                .bottom(travel)
                                .mb(px(-FADER_THUMB / 2.0))
                                .h(px(FADER_THUMB))
                                .rounded(px(radius::SM))
                                .bg(color(colors::THUMB))
                                .shadow(thumb_shadow())
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(7.0))
                                        .right(px(7.0))
                                        .top(relative(0.5))
                                        .mt(px(-1.0))
                                        .h(px(2.0))
                                        .rounded(px(1.0))
                                        .bg(color(colors::SURFACE_RAISED)),
                                ),
                        ),
                )
                .into_any_element()
        },
    )
}

/// A pan: a horizontal slider, 0 hard left to 100 hard right, landing on the centre within 2 of it
/// and stopping there from the keyboard; 64 px wide, its readout (`C`, `L20`, `R30`) under it.
pub fn pan(id: impl Into<ElementId>, value: f64, label: impl Into<SharedString>) -> Slider {
    use bandear_desktop_console::values::{pan_label, pan_value_text};

    Slider::new(
        id,
        PAN,
        value,
        Axis::Horizontal,
        PAN_THUMB,
        label,
        move |shown, fraction| {
            div()
                .w(px(64.0))
                .min_h(px(crate::theme::TARGET))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(2.0))
                .pb(px(2.0))
                .child(
                    div()
                        .relative()
                        .w_full()
                        .h(px(22.0))
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .top(relative(0.5))
                                .mt(px(-3.0))
                                .h(px(6.0))
                                .rounded(px(3.0))
                                .bg(color(colors::TRACK)),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(relative(0.5))
                                .top(px(3.0))
                                .bottom(px(3.0))
                                .w(px(1.0))
                                .bg(color(colors::TEXT_MUTED)),
                        )
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .right(px(PAN_THUMB))
                                .top_0()
                                .bottom_0()
                                .child(
                                    div()
                                        .absolute()
                                        .left(relative(fraction as f32))
                                        .top(relative(0.5))
                                        .mt(px(-7.0))
                                        .w(px(PAN_THUMB))
                                        .h(px(14.0))
                                        .rounded(px(2.0))
                                        .bg(color(colors::THUMB))
                                        .shadow(thumb_shadow()),
                                ),
                        ),
                )
                .child(
                    div()
                        .font_family(FONT_MONO)
                        .text_size(px(text::XXS))
                        .line_height(px(12.0))
                        .text_color(color(colors::TEXT_MUTED))
                        .child(crate::a11y::seen(pan_label(f64::from(shown)))),
                )
                .into_any_element()
        },
    )
    .detent()
    .describe(|shown| pan_value_text(f64::from(shown)).into())
}

/// A parameter slider: a horizontal slider over `positions` (0 to `positions`), as a range input
/// is; a page is a tenth of the way. Its label and its value in words are drawn by the owner beside
/// it (the processing and effects rows, the limiter).
pub fn parameter(
    id: impl Into<ElementId>,
    positions: i32,
    position: f64,
    reset: i32,
    label: impl Into<SharedString>,
    value_text: impl Into<SharedString>,
) -> Slider {
    let range = Range {
        min: 0,
        max: positions,
        step: 1,
        page: (positions / 10).max(1),
        reset,
        centre: None,
        detent: 0,
    };
    let thumb = 14.0;
    Slider::new(
        id,
        range,
        position,
        Axis::Horizontal,
        thumb,
        label,
        move |_, fraction| {
            div()
                .relative()
                .w_full()
                .min_w(px(80.0))
                .h(px(crate::theme::TARGET))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(relative(0.5))
                        .mt(px(-3.0))
                        .h(px(6.0))
                        .rounded(px(3.0))
                        .bg(color(colors::TRACK))
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(relative(fraction as f32))
                                .rounded(px(3.0))
                                .bg(color(colors::FADER_FILL)),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right(px(thumb))
                        .top_0()
                        .bottom_0()
                        .child(
                            div()
                                .absolute()
                                .left(relative(fraction as f32))
                                .top(relative(0.5))
                                .mt(px(-thumb / 2.0))
                                .size(px(thumb))
                                .rounded_full()
                                .bg(color(colors::THUMB))
                                .shadow(thumb_shadow()),
                        ),
                )
                .into_any_element()
        },
    )
    .value_text(value_text)
}
