use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::io::IsTerminal;

pub fn interactive(tty: bool, no_progress: bool, verbose: bool, dumb: bool) -> bool {
    tty && !no_progress && !verbose && !dumb
}

pub fn total(frame_count: Option<u64>, max_frames: u64) -> Option<u64> {
    frame_count.filter(|&n| n > 0).map(|n| {
        if max_frames == 0 {
            n
        } else {
            n.min(max_frames)
        }
    })
}

/// One CLI-owned line, cleared on every return path, including commit failure.
pub struct TerminalProgress {
    bar: ProgressBar,
}

fn update_position(bar: &ProgressBar, frames: u64) {
    bar.set_position(bar.length().map_or(frames, |n| frames.min(n)));
}

impl TerminalProgress {
    pub fn new(no_progress: bool, verbose: bool, total: Option<u64>) -> anyhow::Result<Self> {
        let enabled = interactive(
            std::io::stderr().is_terminal(),
            no_progress,
            verbose,
            std::env::var_os("TERM").is_some_and(|term| term == "dumb"),
        );
        let target = if enabled {
            ProgressDrawTarget::stderr_with_hz(10)
        } else {
            ProgressDrawTarget::hidden()
        };
        Self::with_target(total, target)
    }

    fn with_target(total: Option<u64>, target: ProgressDrawTarget) -> anyhow::Result<Self> {
        let bar = ProgressBar::with_draw_target(total, target);
        let template = if total.is_some() {
            "Processing [{bar:20}] {percent:>3}% {pos}/{len} frames | {fps} fps | ETA {eta_precise}"
        } else {
            "Processing {pos} frames | {fps} fps | elapsed {elapsed_precise}"
        };
        bar.set_style(
            ProgressStyle::with_template(template)?
                .progress_chars("=> ")
                .with_key(
                    "fps",
                    |state: &indicatif::ProgressState, writer: &mut dyn std::fmt::Write| {
                        let seconds = state.elapsed().as_secs_f64();
                        let fps = if seconds > 0.0 {
                            state.pos() as f64 / seconds
                        } else {
                            0.0
                        };
                        let _ = write!(writer, "{fps:.1}");
                    },
                ),
        );
        Ok(Self { bar })
    }

    pub fn observer(&self) -> Option<impl Fn(u64) + Send + 'static> {
        let bar = self.bar.clone();
        (!bar.is_hidden()).then_some(move |frames| {
            update_position(&bar, frames);
        })
    }

    pub fn clear(&self) {
        self.bar.finish_and_clear();
    }
}

impl Drop for TerminalProgress {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_and_totals_do_not_invent_length() {
        assert!(interactive(true, false, false, false));
        for args in [
            (false, false, false, false),
            (true, true, false, false),
            (true, false, true, false),
            (true, false, false, true),
        ] {
            assert!(!interactive(args.0, args.1, args.2, args.3));
        }
        assert_eq!(total(Some(300), 0), Some(300));
        assert_eq!(total(Some(300), 50), Some(50));
        assert_eq!(total(Some(30), 50), Some(30));
        assert_eq!(total(None, 50), None);
        assert_eq!(total(Some(0), 0), None);
    }

    #[test]
    fn progress_clamps_and_clears_on_finish_and_early_return() {
        let progress =
            TerminalProgress::with_target(Some(3), ProgressDrawTarget::hidden()).unwrap();
        // Hidden mode exposes no producer hook, so plain runs incur no UI updates.
        assert!(progress.observer().is_none());
        update_position(&progress.bar, 2);
        assert_eq!(progress.bar.position(), 2);
        update_position(&progress.bar, 7);
        assert_eq!(progress.bar.position(), 3);
        progress.clear();
        assert!(progress.bar.is_finished());
        let unknown = TerminalProgress::with_target(None, ProgressDrawTarget::hidden()).unwrap();
        assert_eq!(unknown.bar.length(), None);
        update_position(&unknown.bar, 7);
        assert_eq!(unknown.bar.position(), 7);
        let bar = unknown.bar.clone();
        drop(unknown);
        assert!(bar.is_finished());
    }
}
