//! The BandEar desktop's Mix screen (parity inventory area B, task T4): the musicians' mixes: the
//! tabs, the channel and group strips, the master strip with its limiter and returns, the Mix tools,
//! and the top bar (the kit draws it).
//!
//! It depends on the console (state, commands, lanes) and the kit (controls, theme) only. The pure
//! modules work out what the screen shows and words (the web console's `mix-view.js`, `mix.js`,
//! `mixes.js`, `mix-tools.js`, `limiter.js` and the returns of `effects.js`); the [model] runs the
//! screen's own flows and lanes over the console (the Mix part of `console-app.js`); the [view]
//! draws it. Its heading is visually hidden (the tabs say where you are) but still read and still
//! focused.

pub mod limiter;
pub mod mix_view;
pub mod model;
pub mod request;
pub mod tools;
pub mod view;

#[cfg(test)]
mod testing;

pub use view::MixScreen;

/// What a command of the screen did, in words: `ok` when the desk did it, and the sentence that says
/// so or says why not (the web console's `{ ok, message }`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub ok: bool,
    pub message: String,
}

impl Said {
    pub fn done(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: message.into(),
        }
    }

    pub fn refused(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
        }
    }
}
