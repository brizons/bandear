//! The sliders under a real pointer: GPUI's test window lays each control out as the app does and
//! takes simulated presses and drags, so these tests check that a press lands where it is drawn.

use std::cell::RefCell;
use std::rc::Rc;

use bandear_desktop_kit::slider::{FADER_THUMB, FaderTone, fader, pan};
use gpui::{
    Context, IntoElement, Modifiers, MouseButton, ParentElement, Render, Styled, TestAppContext,
    Window, div, point, px,
};

/// What a slider reported: its inputs and its changes, in order.
#[derive(Default)]
struct Heard {
    inputs: Vec<i32>,
    changes: Vec<i32>,
}

/// The fader's height in the test window, and where its thumb's centre travels: half a thumb in
/// from each end.
const FADER_HEIGHT: f32 = 400.0;
const TRAVEL_TOP: f32 = FADER_THUMB / 2.0;
const TRAVEL: f32 = FADER_HEIGHT - FADER_THUMB;

/// The y at which a fader shows `value` (0-100): 100 at the top of the travel, 0 at its bottom.
fn fader_y(value: f32) -> f32 {
    TRAVEL_TOP + TRAVEL * (1.0 - value / 100.0)
}

/// One fader at 50, stretched to a fixed height at the window's top left, as a strip stretches it.
struct OneFader {
    heard: Rc<RefCell<Heard>>,
}

impl Render for OneFader {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (inputs, changes) = (self.heard.clone(), self.heard.clone());
        div().flex().h(px(FADER_HEIGHT)).child(
            fader("fader", 50.0, "Test level", FaderTone::Default, false)
                .on_input(move |value, _, _| inputs.borrow_mut().inputs.push(value))
                .on_change(move |value, _, _| changes.borrow_mut().changes.push(value)),
        )
    }
}

/// One pan at the centre, at the window's top left.
struct OnePan {
    heard: Rc<RefCell<Heard>>,
}

impl Render for OnePan {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let inputs = self.heard.clone();
        div().flex().child(
            pan("pan", 50.0, "Test pan")
                .on_input(move |value, _, _| inputs.borrow_mut().inputs.push(value)),
        )
    }
}

#[gpui::test]
fn a_press_on_a_faders_track_jumps_to_the_value_drawn_there(cx: &mut TestAppContext) {
    let heard = Rc::new(RefCell::new(Heard::default()));
    let view_heard = heard.clone();
    let (_, cx) = cx.add_window_view(|_, _| OneFader { heard: view_heard });
    cx.run_until_parked();

    // A quarter of the way up the track, well away from the thumb at 50.
    let at = point(px(20.0), px(fader_y(25.0)));
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());

    let heard = heard.borrow();
    assert_eq!(heard.inputs, [25]);
    assert_eq!(heard.changes, [25]);
}

#[gpui::test]
fn a_fader_follows_a_drag_from_its_thumb(cx: &mut TestAppContext) {
    let heard = Rc::new(RefCell::new(Heard::default()));
    let view_heard = heard.clone();
    let (_, cx) = cx.add_window_view(|_, _| OneFader { heard: view_heard });
    cx.run_until_parked();

    // Grab the thumb where it is drawn (50), drag it up to 80, then down to 20, and let go.
    let grab = point(px(20.0), px(fader_y(50.0)));
    cx.simulate_mouse_down(grab, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(point(px(20.0), px(fader_y(80.0))), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(point(px(20.0), px(fader_y(20.0))), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(point(px(20.0), px(fader_y(20.0))), MouseButton::Left, Modifiers::none());

    let heard = heard.borrow();
    // A press on the thumb does not move it; the drag does, and the release sends where it ended.
    assert_eq!(heard.inputs, [80, 20]);
    assert_eq!(heard.changes, [20]);
}

#[gpui::test]
fn a_press_on_a_pans_track_jumps_to_the_value_drawn_there(cx: &mut TestAppContext) {
    let heard = Rc::new(RefCell::new(Heard::default()));
    let view_heard = heard.clone();
    let (_, cx) = cx.add_window_view(|_, _| OnePan { heard: view_heard });
    cx.run_until_parked();

    // The pan's track is 64 px wide with a 6 px thumb: a quarter of its travel is 3 + 58 / 4.
    let at = point(px(3.0 + 58.0 / 4.0), px(11.0));
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());

    assert_eq!(heard.borrow().inputs, [25]);
}
