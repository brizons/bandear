//! Opens the screen alone over the simulated desk, with its engine running on the sample
//! interface and musicians: no engine starts, no input opens and nothing goes on the network.

use bandear_desktop_console::testing::FakeDesk;

fn main() {
    bandear_desktop_preview::run::<bandear_desktop_network::view::NetworkScreen>(
        FakeDesk::running(),
    );
}
