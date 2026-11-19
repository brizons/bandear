//! The gesture behind every slider of the console (`slider-gesture.js`): faders, pans and the
//! parameter sliders. Pure, so a view only feeds it pointer fractions and keys.
//!
//! A press on the track jumps there; a press on the thumb keeps the grab point, so the value never
//! jumps under the pointer. Each move reports an input when the value changes, and the release one
//! change with the dragged value when it differs from where the drag began. A value set from
//! outside during a drag waits and shows once the drag ended. A key step and a reset are an input
//! and a change of their own.

use crate::values::{Range, SliderKey, clamp_in, key_value, snap_to_centre};

/// What a gesture reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SliderEvent {
    /// The value changed while the gesture goes on.
    Input(i32),
    /// The gesture ended on a new value: the one to send.
    Change(i32),
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    start: i32,
    offset: f64,
}

/// A slider's gesture over `range`.
#[derive(Clone, Debug)]
pub struct SliderGesture {
    range: Range,
    detent: bool,
    value: i32,
    drag: Option<Drag>,
    held: Option<i32>,
}

impl SliderGesture {
    /// A gesture over `range` at `value` (clamped into it).
    pub fn new(range: Range, value: f64) -> Self {
        Self {
            range,
            detent: false,
            value: clamp_in(value, range.min, range.max),
            drag: None,
            held: None,
        }
    }

    /// The same gesture whose drags land on the range's centre within its detent, as a pan's do.
    pub fn with_detent(self) -> Self {
        Self {
            detent: true,
            ..self
        }
    }

    /// The value shown.
    pub fn value(&self) -> i32 {
        self.value
    }

    /// True while the pointer drags.
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Where `value` sits along the track, from 0 to 1.
    pub fn fraction_of(&self, value: i32) -> f64 {
        let span = f64::from(self.range.max - self.range.min);
        if span <= 0.0 {
            return 0.0;
        }
        f64::from(value.clamp(self.range.min, self.range.max) - self.range.min) / span
    }

    /// The value from outside (the engine's): shown at once, or once a drag ends. True when the
    /// shown value changed.
    pub fn set(&mut self, value: f64) -> bool {
        let next = self.clamp(value);
        if self.drag.is_some() {
            self.held = Some(next);
            return false;
        }
        if next == self.value {
            return false;
        }
        self.value = next;
        true
    }

    /// A press at `fraction` along the track, on the thumb or on the track.
    pub fn press(&mut self, fraction: f64, on_thumb: bool) -> Vec<SliderEvent> {
        let offset = if on_thumb {
            self.fraction_of(self.value) - fraction
        } else {
            0.0
        };
        self.drag = Some(Drag {
            start: self.value,
            offset,
        });
        self.held = None;
        self.drag_to(fraction)
    }

    /// The pointer moved to `fraction` along the track.
    pub fn move_to(&mut self, fraction: f64) -> Vec<SliderEvent> {
        if self.drag.is_some() {
            self.drag_to(fraction)
        } else {
            Vec::new()
        }
    }

    /// The pointer let go (or the drag was lost).
    pub fn release(&mut self) -> Vec<SliderEvent> {
        let Some(drag) = self.drag.take() else {
            return Vec::new();
        };
        let dragged = self.value;
        if let Some(held) = self.held.take() {
            self.value = held;
        }
        if dragged == drag.start {
            Vec::new()
        } else {
            vec![SliderEvent::Change(dragged)]
        }
    }

    /// A key the slider answers.
    pub fn key(&mut self, key: SliderKey) -> Vec<SliderEvent> {
        let next = key_value(key, self.value, &self.range);
        self.commit(next)
    }

    /// A double-click: back to `value`.
    pub fn reset(&mut self, value: i32) -> Vec<SliderEvent> {
        let next = value.clamp(self.range.min, self.range.max);
        self.commit(next)
    }
}

impl SliderGesture {
    fn clamp(&self, value: f64) -> i32 {
        clamp_in(value, self.range.min, self.range.max)
    }

    /// The value at `fraction` along the track, snapped to the centre for a pan.
    fn value_at(&self, fraction: f64) -> i32 {
        let span = f64::from(self.range.max - self.range.min);
        let value = self.clamp(f64::from(self.range.min) + fraction * span);
        if self.detent {
            snap_to_centre(value, &self.range)
        } else {
            value
        }
    }

    fn drag_to(&mut self, fraction: f64) -> Vec<SliderEvent> {
        let offset = self.drag.map_or(0.0, |drag| drag.offset);
        let next = self.value_at(fraction + offset);
        if next == self.value {
            return Vec::new();
        }
        self.value = next;
        vec![SliderEvent::Input(next)]
    }

    fn commit(&mut self, next: i32) -> Vec<SliderEvent> {
        if next == self.value {
            return Vec::new();
        }
        self.value = next;
        vec![SliderEvent::Input(next), SliderEvent::Change(next)]
    }
}

#[cfg(test)]
mod tests {
    use super::SliderEvent::{Change, Input};
    use super::*;
    use crate::values::{FADER, PAN};

    fn fader(value: f64) -> SliderGesture {
        SliderGesture::new(FADER, value)
    }

    fn key(name: &str) -> SliderKey {
        SliderKey::from_dom(name).expect("a slider key")
    }

    #[test]
    fn a_press_on_the_track_jumps_there_a_drag_follows_and_one_change_goes_out_at_the_end() {
        let mut gesture = fader(50.0);
        assert_eq!(gesture.press(0.8, false), [Input(80)]);
        assert!(gesture.dragging());
        assert_eq!(gesture.move_to(0.85), [Input(85)]);
        assert_eq!(
            gesture.move_to(0.851),
            [],
            "no event when the value does not change"
        );
        assert_eq!(gesture.move_to(0.9), [Input(90)]);
        assert_eq!(gesture.release(), [Change(90)]);
        assert_eq!(gesture.value(), 90);
        assert!(!gesture.dragging());
    }

    #[test]
    fn grabbing_the_thumb_keeps_the_grab_point_so_the_value_never_jumps_under_the_pointer() {
        let mut gesture = fader(50.0);
        assert_eq!(gesture.press(0.53, true), []);
        assert_eq!(gesture.move_to(0.63), [Input(60)]);
        assert_eq!(gesture.release(), [Change(60)]);
    }

    #[test]
    fn a_drag_that_ends_where_it_started_reports_no_change() {
        let mut gesture = fader(50.0);
        gesture.press(0.5, true);
        gesture.move_to(0.7);
        gesture.move_to(0.5);
        assert_eq!(gesture.release(), []);
    }

    #[test]
    fn moving_or_releasing_without_a_press_does_nothing() {
        let mut gesture = fader(50.0);
        assert_eq!(gesture.move_to(0.9), []);
        assert_eq!(gesture.release(), []);
        assert_eq!(gesture.value(), 50);
    }

    #[test]
    fn a_value_set_from_outside_during_a_drag_waits_and_shows_before_the_drags_change_goes_out() {
        let mut gesture = fader(50.0);
        gesture.press(0.6, false);
        assert!(!gesture.set(20.0), "the pointer wins while it drags");
        assert_eq!(gesture.value(), 60);
        gesture.move_to(0.7);
        assert_eq!(
            gesture.release(),
            [Change(70)],
            "the change is the dragged value"
        );
        assert_eq!(
            gesture.value(),
            20,
            "the value set during the drag shows once it has ended"
        );
        assert!(
            gesture.set(70.0),
            "after the drag, a value from outside shows at once"
        );
        assert_eq!(gesture.value(), 70);
    }

    #[test]
    fn a_value_set_from_outside_is_clamped_and_setting_the_same_value_changes_nothing() {
        let mut gesture = fader(50.0);
        assert!(gesture.set(140.0));
        assert_eq!(gesture.value(), 100);
        assert!(!gesture.set(100.0));
    }

    #[test]
    fn each_key_step_is_an_input_and_a_change_of_its_own() {
        let mut gesture = fader(62.0);
        assert_eq!(gesture.key(key("ArrowUp")), [Input(63), Change(63)]);
        assert_eq!(gesture.key(key("PageDown")), [Input(53), Change(53)]);
        assert_eq!(gesture.key(key("Home")), [Input(0), Change(0)]);
        assert_eq!(gesture.key(key("End")), [Input(100), Change(100)]);
        assert_eq!(
            gesture.key(key("ArrowUp")),
            [],
            "at the top a step changes nothing"
        );
        assert_eq!(
            SliderKey::from_dom("Enter"),
            None,
            "a key the slider does not use is left to the page"
        );
    }

    #[test]
    fn a_double_click_resets_to_the_default_and_is_quiet_when_the_value_is_already_there() {
        let mut gesture = fader(40.0);
        assert_eq!(gesture.reset(100), [Input(100), Change(100)]);
        assert_eq!(gesture.reset(100), []);
    }

    #[test]
    fn a_pan_drag_lands_on_the_centre_within_the_detent_and_the_keyboard_stops_there() {
        let mut gesture = SliderGesture::new(PAN, 30.0).with_detent();
        assert_eq!(gesture.press(0.51, false), [Input(50)]);
        assert_eq!(
            gesture.move_to(0.53),
            [Input(53)],
            "outside the detent the drag is free"
        );
        assert_eq!(gesture.release(), [Change(53)]);
        gesture.set(45.0);
        assert_eq!(gesture.key(key("PageUp")), [Input(50), Change(50)]);
    }

    #[test]
    fn a_parameter_slider_spans_its_own_positions() {
        let range = Range {
            min: 0,
            max: 1000,
            step: 1,
            page: 100,
            reset: 500,
            centre: None,
            detent: 0,
        };
        let mut gesture = SliderGesture::new(range, 250.0);
        assert_eq!(gesture.fraction_of(250), 0.25);
        assert_eq!(gesture.press(0.5, false), [Input(500)]);
        assert_eq!(gesture.release(), [Change(500)]);
        assert_eq!(gesture.key(key("PageUp")), [Input(600), Change(600)]);
    }
}
