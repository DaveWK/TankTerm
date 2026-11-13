//! Parsing what programs write to the terminal.
//!
//! `vte`'s processor handles the escape sequences, but drops the shell integration sequences:
//! OSC 7 and OSC 133. [`Parser`] looks for those beside it, and hands each to the terminal once
//! the processor has handled the output before it, so prompt marks land where the shell put
//! them.

use crate::event::EventListener;
use crate::term::Term;
use crate::term::shell_integration::ShellSequence;
use crate::vte::ansi::{Processor, StdSyncHandler, Timeout};

/// Escape character.
const ESC: u8 = 0x1b;

/// Bell, which ends an OSC.
const BEL: u8 = 0x07;

/// Cancel and substitute, which end any escape sequence.
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

/// Longest OSC payload looked at; longer ones aren't shell integration sequences.
const MAX_PAYLOAD: usize = 0x4000;

/// Begin synchronized update.
const BSU: &[u8] = b"\x1b[?2026h";

/// The escape sequence parser of a terminal.
#[derive(Default)]
pub struct Parser {
    processor: Processor,
    scanner: Scanner,
}

impl Parser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse output of the program in the terminal.
    pub fn advance<T: EventListener>(&mut self, term: &mut Term<T>, bytes: &[u8]) {
        let mut rest = bytes;
        while !rest.is_empty() {
            let (length, sequence) = self.scanner.scan(rest);
            self.processor.advance(term, &rest[..length]);
            if let Some(sequence) = sequence {
                self.apply(term, sequence);
            }
            rest = &rest[length..];
        }
    }

    /// Hand a shell integration sequence to the terminal.
    fn apply<T: EventListener>(&mut self, term: &mut Term<T>, sequence: ShellSequence) {
        // In a synchronized update the processor holds the output back. The output before the
        // sequence has to reach the terminal first; the update goes on after it.
        let synchronized = self.processor.sync_timeout().pending_timeout();
        if synchronized {
            self.processor.stop_sync(term);
        }

        term.shell_sequence(sequence);

        if synchronized {
            self.processor.advance(term, BSU);
        }
    }

    /// When the synchronized update in progress ends at the latest.
    pub fn sync_timeout(&self) -> &StdSyncHandler {
        self.processor.sync_timeout()
    }

    /// End the synchronized update in progress.
    pub fn stop_sync<T: EventListener>(&mut self, term: &mut Term<T>) {
        self.processor.stop_sync(term);
    }

    /// Number of bytes held back by a synchronized update.
    pub fn sync_bytes_count(&self) -> usize {
        self.processor.sync_bytes_count()
    }
}

/// Finds shell integration sequences, even when they are split across reads.
///
/// It follows the escape sequence parser where OSCs are concerned: an ESC starts a new escape
/// sequence anywhere, and an OSC ends at BEL, CAN, SUB or ESC (`ESC \` is its usual end).
#[derive(Debug, Default)]
struct Scanner {
    state: State,
    /// The payload of the OSC being read, while it could be a shell integration sequence.
    payload: Vec<u8>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    /// After an ESC.
    Escape,
    /// In an OSC that could be a shell integration sequence.
    Osc,
    /// In another OSC.
    OtherOsc,
}

impl Scanner {
    /// Look through `bytes` up to the end of the first shell integration sequence.
    ///
    /// Returns how many bytes were looked at, and the sequence that ends with the last of them,
    /// if one does.
    fn scan(&mut self, bytes: &[u8]) -> (usize, Option<ShellSequence>) {
        let mut index = 0;
        while index < bytes.len() {
            let byte = bytes[index];
            index += 1;

            match self.state {
                State::Ground => match bytes[index - 1..].iter().position(|byte| *byte == ESC) {
                    Some(offset) => {
                        index += offset;
                        self.state = State::Escape;
                    },
                    None => return (bytes.len(), None),
                },
                State::Escape => {
                    self.state = match byte {
                        b']' => {
                            self.payload.clear();
                            State::Osc
                        },
                        ESC => State::Escape,
                        _ => State::Ground,
                    };
                },
                State::Osc | State::OtherOsc => match byte {
                    BEL | CAN | SUB | ESC => {
                        let ours = self.state == State::Osc;
                        self.state = if byte == ESC { State::Escape } else { State::Ground };
                        let sequence = ours.then(|| ShellSequence::parse(&self.payload)).flatten();
                        if sequence.is_some() {
                            return (index, sequence);
                        }
                    },
                    // The parser leaves other control characters out of the payload.
                    0x00..=0x1f => (),
                    _ if self.state == State::Osc => {
                        self.payload.push(byte);
                        if !could_be_ours(&self.payload) {
                            self.state = State::OtherOsc;
                        }
                    },
                    _ => (),
                },
            }
        }

        (bytes.len(), None)
    }
}

/// Whether an OSC payload, as far as it has been read, could be a shell integration sequence.
fn could_be_ours(payload: &[u8]) -> bool {
    payload.len() <= MAX_PAYLOAD
        && [&b"7;"[..], b"133;"]
            .iter()
            .any(|prefix| prefix.starts_with(payload) || payload.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    use crate::term::shell_integration::PromptMark;

    /// The sequences found in `chunks`, fed one after the other, with the bytes read up to each.
    fn scan(chunks: &[&[u8]]) -> Vec<(usize, ShellSequence)> {
        let mut scanner = Scanner::default();
        let mut found = Vec::new();
        let mut read = 0;
        for chunk in chunks {
            let mut rest = *chunk;
            while !rest.is_empty() {
                let (length, sequence) = scanner.scan(rest);
                read += length;
                found.extend(sequence.map(|sequence| (read, sequence)));
                rest = &rest[length..];
            }
        }
        found
    }

    fn prompt(mark: PromptMark) -> ShellSequence {
        ShellSequence::Prompt(mark)
    }

    #[test]
    fn finds_sequences_with_every_terminator() {
        let found = scan(&[b"ab\x1b]133;A\x07$ \x1b]133;B\x1b\\ls\x1b]133;C\x18"]);
        assert_eq!(found, vec![
            (10, prompt(PromptMark::PromptStart)),
            (20, prompt(PromptMark::CommandStart)),
            (31, prompt(PromptMark::OutputStart)),
        ]);
    }

    #[test]
    fn finds_sequences_split_across_reads() {
        let found =
            scan(&[b"\x1b", b"]13", b"3;D;", b"1", b"\x07", b"\x1b]7;file:///tmp\x1b", b"\\"]);
        assert_eq!(found, vec![
            (10, prompt(PromptMark::CommandEnd(Some(1)))),
            (26, ShellSequence::WorkingDirectory(PathBuf::from("/tmp"))),
        ]);
    }

    #[test]
    fn skips_other_sequences() {
        let found = scan(&[
            b"\x1b]8;;https://example.org/\x07link\x1b]8;;\x07",
            b"\x1b]0;133;A\x07\x1b[7;133H\x1b]1337;D\x07",
            // An OSC cut short by another escape sequence ends there.
            b"\x1b]133;\x1b]133;B\x07",
        ]);
        assert_eq!(found, vec![(77, prompt(PromptMark::CommandStart))]);
    }

    #[test]
    fn skips_long_payloads() {
        let long = [&b"\x1b]7;file:///"[..], &[b'a'; MAX_PAYLOAD], b"\x07"].concat();
        assert_eq!(scan(&[&long]), vec![]);
    }
}
