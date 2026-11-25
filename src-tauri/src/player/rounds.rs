//! The render thread's rounds (`docs/player.md` §13): each one tells the engine how far the output
//! has got and renders until the lead is full ([`Shared::pump`]), then looks after the output — it
//! reopens one that failed, and pauses it while nothing plays.
//!
//! The output is a [`Sink`] so the rounds can be tested without a sound card: the app's is a cpal
//! stream ([`super::output::Output`]).

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use prosody_player::Event;

use super::host::{Shared, ToAudio};

/// How long the render thread sleeps between rounds while there is something to play and nothing
/// wakes it: well inside the lead, and the output's report moves on at least this often.
pub(crate) const ROUND: Duration = Duration::from_millis(5);

/// Rounds with nothing to play before the output is paused and the thread sleeps until a command
/// wakes it: about a second, long past a stop's fade, so a Stop then a Play does not pause and
/// restart the output in between. Idle, an open `AAudio` stream keeps the phone's audio path powered
/// and a 5 ms loop keeps waking a core, for nothing.
pub(crate) const IDLE: u32 = 200;

/// Rounds between attempts to reopen an output that failed and would not reopen: about a second,
/// while the audio server restarts or a route settles.
pub(crate) const RETRY: u32 = 200;

/// The output, as the rounds drive it.
pub(crate) trait Sink {
    /// Replaces a failed output with a new one at the same rate, fed from a new channel
    /// ([`Shared::reconnect`]).
    ///
    /// # Errors
    ///
    /// No output could be opened.
    fn reopen(
        &mut self,
        shared: &Arc<Shared>,
        outbox: &mut VecDeque<ToAudio>,
    ) -> Result<(), String>;

    /// Stops asking for buffers until [`Sink::resume`].
    ///
    /// # Errors
    ///
    /// The output would not pause; it then goes on playing silence.
    fn pause(&mut self) -> Result<(), String>;

    /// Asks for buffers again.
    ///
    /// # Errors
    ///
    /// The output would not start; it is then treated as failed, and reopened.
    fn resume(&mut self) -> Result<(), String>;
}

/// What the thread does after a round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wait {
    /// Sleeps a [`ROUND`], or until a command wakes it.
    Round,
    /// Sleeps until a command wakes it: nothing plays.
    Asleep,
}

/// The render thread's state between rounds.
#[derive(Debug, Default)]
pub(crate) struct Rounds {
    /// Finished blocks' buffers, to render into.
    spare: Vec<Vec<f32>>,
    /// What the channel had no room for.
    outbox: VecDeque<ToAudio>,
    /// The losses last logged.
    said: (u64, u64),
    /// Rounds in a row with nothing to play.
    idle: u32,
    /// Whether the output is paused.
    paused: bool,
    /// Whether the output failed and has not been reopened.
    broken: bool,
    /// Rounds until the next attempt to reopen it.
    retry: u32,
    /// Whether the page has been told it would not reopen, since it failed.
    told: bool,
}

impl Rounds {
    /// One round; says how long to sleep after it.
    pub(crate) fn round(
        &mut self,
        shared: &Arc<Shared>,
        spent: &Receiver<Vec<f32>>,
        sink: &mut impl Sink,
    ) -> Wait {
        if shared.take_lost() {
            self.broken = true;
            self.retry = 0;
        }
        self.spare.extend(spent.try_iter());
        shared.pump(&mut self.spare, &mut self.outbox);
        self.say_losses(shared);
        if shared.idle(&self.outbox) {
            self.idle = self.idle.saturating_add(1);
            // The next Play tries to reopen a failed output at once.
            self.retry = 0;
        } else {
            self.idle = 0;
            if self.broken {
                // Reopened only when there is something to play: a phone's route changing while
                // nothing plays opens nothing.
                self.mend(shared, sink);
            } else if self.paused {
                self.paused = false;
                if let Err(error) = sink.resume() {
                    log::error!("{error}");
                    shared.lose();
                }
            }
        }
        if self.idle < IDLE {
            return Wait::Round;
        }
        if self.idle == IDLE && !self.broken && !self.paused {
            match sink.pause() {
                Ok(()) => self.paused = true,
                Err(error) => log::warn!("{error}; it plays silence while nothing plays"),
            }
        }
        Wait::Asleep
    }

    /// Reopens the failed output, or waits [`RETRY`] rounds before trying again; the page is told
    /// once that it would not reopen, and the system's log each time.
    fn mend(&mut self, shared: &Arc<Shared>, sink: &mut impl Sink) {
        if self.retry > 0 {
            self.retry -= 1;
            return;
        }
        match sink.reopen(shared, &mut self.outbox) {
            Ok(()) => {
                log::warn!("the audio output failed and was reopened");
                self.broken = false;
                self.paused = false;
                self.told = false;
            }
            Err(error) => {
                log::error!("the audio output failed and would not reopen: {error}");
                self.retry = RETRY;
                if !self.told {
                    self.told = true;
                    shared.tell(
                        Event::Error(format!(
                            "the audio output failed and would not reopen ({error}); trying again"
                        ))
                        .json(),
                    );
                }
            }
        }
    }

    /// The losses in the system's log too, when they change: the page may be asleep, and a phone's
    /// log is where a dropout with the screen off can be found afterwards.
    fn say_losses(&mut self, shared: &Shared) {
        let now = shared.status.losses();
        if now != self.said {
            log::warn!(
                "the native player ran dry {} time(s), the output glitched {} time(s), {} frames queued",
                now.0,
                now.1,
                shared.status.queued()
            );
            self.said = now;
        }
    }
}

#[cfg(test)]
mod tests;
