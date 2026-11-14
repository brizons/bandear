//! Opens the screen alone over the simulated desk, with its engine running on the sample
//! interface and musicians: no engine starts, no input opens and nothing goes on the network.
//! `MIX_PREVIEW=stopped` opens it over a desk whose engine is stopped instead, to see the notice.

use bandear_desktop_console::testing::FakeDesk;

fn main() {
    let desk = match std::env::var("MIX_PREVIEW").as_deref() {
        Ok("stopped") => FakeDesk::stopped(),
        _ => FakeDesk::running(),
    };
    bandear_desktop_preview::run::<bandear_desktop_mix::view::MixScreen>(desk);
}
