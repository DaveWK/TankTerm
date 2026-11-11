//! Key encoding for the kitty keyboard protocol.
//!
//! <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>
//!
//! When the application has not enabled any of the protocol's flags, keys are
//! encoded by the legacy binding table in [`crate::bindings`] exactly as
//! before; this module only takes over while one of
//! [`TermMode::KITTY_KEYBOARD_PROTOCOL`] is active.
//!
//! The encoder is ported from Alacritty's `alacritty/src/input/keyboard.rs`
//! (Copyright 2020 The Alacritty Project, Apache-2.0), as carried in TankTerm's
//! `tankterm/src/input/keyboard.rs`, adapted from winit's `KeyEvent` to iced's
//! keyboard events. The mapping between the two is:
//!
//! | winit                                  | iced                       |
//! | -------------------------------------- | -------------------------- |
//! | `KeyEvent::logical_key`                | `modified_key`             |
//! | `KeyEvent::key_without_modifiers()`    | `key`                      |
//! | `KeyEvent::text_with_all_modifiers()`  | `text` (press only)        |
//! | `KeyEvent::location`                   | `location`                 |
//! | `KeyEvent::repeat` / `state`           | `repeat` / press vs release |

use iced_core::keyboard::key::Named;
use iced_core::keyboard::{Key, Location, Modifiers};
use tankterm_terminal::term::TermMode;

/// Whether the key went down, auto-repeated, or went up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyState {
    Pressed,
    Repeat,
    Released,
}

/// A key event, borrowed from an iced [`iced_core::keyboard::Event`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct KeyInput<'a> {
    /// The key without modifiers applied (iced's `key`).
    pub key: &'a Key,
    /// The key with all modifiers but Ctrl applied (iced's `modified_key`).
    pub modified_key: &'a Key,
    pub location: Location,
    pub modifiers: Modifiers,
    /// The text the key produces with all modifiers applied; never present
    /// on release.
    pub text: Option<&'a str>,
    pub state: KeyState,
}

/// What to send for a key while the kitty keyboard protocol is active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KittyOutput {
    /// Write these bytes to the PTY.
    Write(Vec<u8>),
    /// The protocol keeps this key's legacy encoding: use the binding table
    /// (and the key's text) as when the protocol is off.
    Legacy,
    /// Send nothing.
    Nothing,
}

/// Encode `input` for the active kitty keyboard `mode`.
///
/// Returns `None` when no kitty flag is set, i.e. the caller should use the
/// legacy encoding unchanged (and send nothing for a release).
pub(crate) fn encode(input: &KeyInput, mode: TermMode) -> Option<KittyOutput> {
    if !mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL) {
        return None;
    }

    Some(match input.state {
        KeyState::Released => encode_release(input, mode),
        KeyState::Pressed | KeyState::Repeat => encode_press(input, mode),
    })
}

fn encode_press(input: &KeyInput, mode: TermMode) -> KittyOutput {
    let text = input.text.unwrap_or_default();

    // Mask `Alt` from the modifiers when it wouldn't send an escape.
    let mods = if alt_send_esc(input, text) {
        input.modifiers
    } else {
        input.modifiers - Modifiers::ALT
    };

    // Unmodified cursor keys keep their application cursor (DECCKM) SS3
    // encoding, like Alacritty's `APP_CURSOR` bindings.
    if mode.contains(TermMode::APP_CURSOR)
        && mods.is_empty()
        && input.location != Location::Numpad
        && matches!(
            input.modified_key,
            Key::Named(
                Named::Home
                    | Named::End
                    | Named::ArrowUp
                    | Named::ArrowDown
                    | Named::ArrowLeft
                    | Named::ArrowRight
            )
        )
    {
        return KittyOutput::Legacy;
    }

    if should_build_sequence(input, text, mode, mods) {
        return write_or_nothing(build_sequence(input, mods, mode));
    }

    match input.modified_key {
        // Named keys with a textual representation (Enter, Tab, Space,
        // Backspace, Escape) keep their legacy bytes.
        Key::Named(_) => KittyOutput::Legacy,
        _ => {
            let mut bytes = Vec::with_capacity(text.len() + 1);
            if mods.alt() {
                bytes.push(b'\x1b');
            }
            bytes.extend_from_slice(text.as_bytes());
            write_or_nothing(bytes)
        },
    }
}

fn encode_release(input: &KeyInput, mode: TermMode) -> KittyOutput {
    if !mode.contains(TermMode::REPORT_EVENT_TYPES) {
        return KittyOutput::Nothing;
    }

    // Enter, Tab and Backspace only report releases when all keys are
    // reported as escape codes, as their presses are plain bytes otherwise.
    if !mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC)
        && matches!(
            input.modified_key,
            Key::Named(Named::Enter | Named::Tab | Named::Backspace)
        )
    {
        return KittyOutput::Nothing;
    }

    // iced provides no text on release: use the key itself to decide whether
    // Alt would have sent an escape.
    let text = match input.modified_key {
        Key::Character(c) => c.as_str(),
        _ => "",
    };
    let mods = if alt_send_esc(input, text) {
        input.modifiers
    } else {
        input.modifiers - Modifiers::ALT
    };

    write_or_nothing(build_sequence(input, mods, mode))
}

fn write_or_nothing(bytes: Vec<u8>) -> KittyOutput {
    if bytes.is_empty() {
        KittyOutput::Nothing
    } else {
        KittyOutput::Write(bytes)
    }
}

/// Whether `Alt` acts as a modifier for this key, i.e. would send an escape.
///
/// On macOS Option composes characters, so it is only a modifier for keys
/// without text (arrows, function keys, ...).
fn alt_send_esc(input: &KeyInput, text: &str) -> bool {
    let alt = input.modifiers.alt();
    let alt_composes = cfg!(target_os = "macos");

    match input.modified_key {
        Key::Named(named) => {
            if named_to_text(*named).is_some() {
                alt && !alt_composes
            } else {
                // Treat `Alt` as modifier for named keys without text, like
                // ArrowUp.
                alt
            }
        },
        _ => alt && !alt_composes && text.chars().count() == 1,
    }
}

/// winit's `NamedKey::to_text`.
fn named_to_text(named: Named) -> Option<&'static str> {
    match named {
        Named::Enter => Some("\r"),
        Named::Backspace => Some("\x08"),
        Named::Tab => Some("\t"),
        Named::Space => Some(" "),
        Named::Escape => Some("\x1b"),
        _ => None,
    }
}

/// Whether the key needs an escape sequence rather than its text.
fn should_build_sequence(
    input: &KeyInput,
    text: &str,
    mode: TermMode,
    mods: Modifiers,
) -> bool {
    if mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC) {
        return true;
    }

    let disambiguate = mode.contains(TermMode::DISAMBIGUATE_ESC_CODES)
        && (*input.modified_key == Key::Named(Named::Escape)
            || input.location == Location::Numpad
            || (!mods.is_empty()
                && (mods != Modifiers::SHIFT
                    || matches!(
                        input.modified_key,
                        Key::Named(
                            Named::Tab | Named::Enter | Named::Backspace
                        )
                    ))));

    match input.modified_key {
        _ if disambiguate => true,
        // Exclude all the named keys unless they have textual representation.
        Key::Named(named) => named_to_text(*named).is_none(),
        _ => text.is_empty(),
    }
}

/// Build a key's escape sequence for the given `mods` and `mode`.
fn build_sequence(
    input: &KeyInput,
    mods: Modifiers,
    mode: TermMode,
) -> Vec<u8> {
    let mut modifiers = SequenceModifiers::from(mods);

    let kitty_seq = mode.intersects(
        TermMode::REPORT_ALL_KEYS_AS_ESC
            | TermMode::DISAMBIGUATE_ESC_CODES
            | TermMode::REPORT_EVENT_TYPES,
    );

    let kitty_encode_all = mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC);
    // The default event type is 1 (press), so it can be omitted.
    let kitty_event_type = mode.contains(TermMode::REPORT_EVENT_TYPES)
        && input.state != KeyState::Pressed;

    let context = SequenceBuilder {
        mode,
        modifiers,
        kitty_seq,
        kitty_encode_all,
        kitty_event_type,
    };

    let associated_text = input.text.filter(|text| {
        mode.contains(TermMode::REPORT_ASSOCIATED_TEXT)
            && input.state != KeyState::Released
            && !text.is_empty()
            && !is_control_character(text)
    });

    let sequence_base = context
        .try_build_numpad(input)
        .or_else(|| context.try_build_named_kitty(input))
        .or_else(|| {
            context.try_build_named_normal(input, associated_text.is_some())
        })
        .or_else(|| {
            context.try_build_control_char_or_mod(input, &mut modifiers)
        })
        .or_else(|| context.try_build_textual(input, associated_text));

    let SequenceBase {
        payload,
        terminator,
    } = match sequence_base {
        Some(base) => base,
        None => return Vec::new(),
    };

    let mut payload = format!("\x1b[{payload}");

    // Add modifiers information.
    if kitty_event_type || !modifiers.is_empty() || associated_text.is_some() {
        payload.push_str(&format!(";{}", modifiers.encode_esc_sequence()));
    }

    // Push event type.
    if kitty_event_type {
        payload.push(':');
        payload.push(match input.state {
            KeyState::Pressed => '1',
            KeyState::Repeat => '2',
            KeyState::Released => '3',
        });
    }

    if let Some(text) = associated_text {
        let mut codepoints = text.chars().map(u32::from);
        if let Some(codepoint) = codepoints.next() {
            payload.push_str(&format!(";{codepoint}"));
        }
        for codepoint in codepoints {
            payload.push_str(&format!(":{codepoint}"));
        }
    }

    payload.push(terminator.encode_esc_sequence());

    payload.into_bytes()
}

/// Helper to build escape sequence payloads from a [`KeyInput`].
struct SequenceBuilder {
    mode: TermMode,
    /// The emitted sequence should follow the kitty keyboard protocol.
    kitty_seq: bool,
    /// Encode all the keys according to the protocol.
    kitty_encode_all: bool,
    /// Report event types.
    kitty_event_type: bool,
    modifiers: SequenceModifiers,
}

impl SequenceBuilder {
    /// Try building sequence from the event's emitting text.
    fn try_build_textual(
        &self,
        input: &KeyInput,
        associated_text: Option<&str>,
    ) -> Option<SequenceBase> {
        let character = match input.modified_key {
            Key::Character(character) if self.kitty_seq => character.as_str(),
            _ => return None,
        };

        if character.chars().count() == 1 {
            let shift = self.modifiers.contains(SequenceModifiers::SHIFT);

            let ch = character.chars().next().unwrap();
            let unshifted_ch = if shift {
                ch.to_lowercase().next().unwrap()
            } else {
                ch
            };

            let alternate_key_code = u32::from(ch);
            let mut unicode_key_code = u32::from(unshifted_ch);

            // Try to get the base for keys which change based on modifier,
            // like `1` for `!`.
            //
            // However it should only be performed when `SHIFT` is pressed.
            if shift && alternate_key_code == unicode_key_code {
                if let Key::Character(unmodded) = input.key {
                    unicode_key_code = u32::from(
                        unmodded.chars().next().unwrap_or(unshifted_ch),
                    );
                }
            }

            // NOTE: Base layouts are ignored, since iced doesn't expose this
            // information.
            let payload = if self.mode.contains(TermMode::REPORT_ALTERNATE_KEYS)
                && alternate_key_code != unicode_key_code
            {
                format!("{unicode_key_code}:{alternate_key_code}")
            } else {
                unicode_key_code.to_string()
            };

            Some(SequenceBase::new(payload, SequenceTerminator::Kitty))
        } else if self.kitty_encode_all && associated_text.is_some() {
            // Fallback when need to report text, but we don't have any key
            // associated with this text.
            Some(SequenceBase::new("0", SequenceTerminator::Kitty))
        } else {
            None
        }
    }

    /// Try building from numpad key.
    ///
    /// `None` is returned when the key is neither known nor numpad.
    fn try_build_numpad(&self, input: &KeyInput) -> Option<SequenceBase> {
        if !self.kitty_seq || input.location != Location::Numpad {
            return None;
        }

        let base = match input.modified_key.as_ref() {
            Key::Character("0") => "57399",
            Key::Character("1") => "57400",
            Key::Character("2") => "57401",
            Key::Character("3") => "57402",
            Key::Character("4") => "57403",
            Key::Character("5") => "57404",
            Key::Character("6") => "57405",
            Key::Character("7") => "57406",
            Key::Character("8") => "57407",
            Key::Character("9") => "57408",
            Key::Character(".") => "57409",
            Key::Character("/") => "57410",
            Key::Character("*") => "57411",
            Key::Character("-") => "57412",
            Key::Character("+") => "57413",
            Key::Character("=") => "57415",
            Key::Named(named) => match named {
                Named::Enter => "57414",
                Named::ArrowLeft => "57417",
                Named::ArrowRight => "57418",
                Named::ArrowUp => "57419",
                Named::ArrowDown => "57420",
                Named::PageUp => "57421",
                Named::PageDown => "57422",
                Named::Home => "57423",
                Named::End => "57424",
                Named::Insert => "57425",
                Named::Delete => "57426",
                _ => return None,
            },
            _ => return None,
        };

        Some(SequenceBase::new(base, SequenceTerminator::Kitty))
    }

    /// Try building from a [`Named`] key using the kitty keyboard protocol
    /// encoding for functional keys.
    fn try_build_named_kitty(&self, input: &KeyInput) -> Option<SequenceBase> {
        let named = match input.modified_key {
            Key::Named(named) if self.kitty_seq => *named,
            _ => return None,
        };

        let (base, terminator) = match named {
            // F3 in kitty protocol diverges from alacritty's terminfo.
            Named::F3 => ("13", SequenceTerminator::Normal('~')),
            Named::F13 => ("57376", SequenceTerminator::Kitty),
            Named::F14 => ("57377", SequenceTerminator::Kitty),
            Named::F15 => ("57378", SequenceTerminator::Kitty),
            Named::F16 => ("57379", SequenceTerminator::Kitty),
            Named::F17 => ("57380", SequenceTerminator::Kitty),
            Named::F18 => ("57381", SequenceTerminator::Kitty),
            Named::F19 => ("57382", SequenceTerminator::Kitty),
            Named::F20 => ("57383", SequenceTerminator::Kitty),
            Named::F21 => ("57384", SequenceTerminator::Kitty),
            Named::F22 => ("57385", SequenceTerminator::Kitty),
            Named::F23 => ("57386", SequenceTerminator::Kitty),
            Named::F24 => ("57387", SequenceTerminator::Kitty),
            Named::F25 => ("57388", SequenceTerminator::Kitty),
            Named::F26 => ("57389", SequenceTerminator::Kitty),
            Named::F27 => ("57390", SequenceTerminator::Kitty),
            Named::F28 => ("57391", SequenceTerminator::Kitty),
            Named::F29 => ("57392", SequenceTerminator::Kitty),
            Named::F30 => ("57393", SequenceTerminator::Kitty),
            Named::F31 => ("57394", SequenceTerminator::Kitty),
            Named::F32 => ("57395", SequenceTerminator::Kitty),
            Named::F33 => ("57396", SequenceTerminator::Kitty),
            Named::F34 => ("57397", SequenceTerminator::Kitty),
            Named::F35 => ("57398", SequenceTerminator::Kitty),
            Named::ScrollLock => ("57359", SequenceTerminator::Kitty),
            Named::PrintScreen => ("57361", SequenceTerminator::Kitty),
            Named::Pause => ("57362", SequenceTerminator::Kitty),
            Named::ContextMenu => ("57363", SequenceTerminator::Kitty),
            Named::MediaPlay => ("57428", SequenceTerminator::Kitty),
            Named::MediaPause => ("57429", SequenceTerminator::Kitty),
            Named::MediaPlayPause => ("57430", SequenceTerminator::Kitty),
            Named::MediaStop => ("57432", SequenceTerminator::Kitty),
            Named::MediaFastForward => ("57433", SequenceTerminator::Kitty),
            Named::MediaRewind => ("57434", SequenceTerminator::Kitty),
            Named::MediaTrackNext => ("57435", SequenceTerminator::Kitty),
            Named::MediaTrackPrevious => ("57436", SequenceTerminator::Kitty),
            Named::MediaRecord => ("57437", SequenceTerminator::Kitty),
            Named::AudioVolumeDown => ("57438", SequenceTerminator::Kitty),
            Named::AudioVolumeUp => ("57439", SequenceTerminator::Kitty),
            Named::AudioVolumeMute => ("57440", SequenceTerminator::Kitty),
            _ => return None,
        };

        Some(SequenceBase::new(base, terminator))
    }

    /// Try building from a [`Named`] key.
    fn try_build_named_normal(
        &self,
        input: &KeyInput,
        has_associated_text: bool,
    ) -> Option<SequenceBase> {
        let named = match input.modified_key {
            Key::Named(named) => *named,
            _ => return None,
        };

        // The default parameter is 1, so we can omit it.
        let one_based = if self.modifiers.is_empty()
            && !self.kitty_event_type
            && !has_associated_text
        {
            ""
        } else {
            "1"
        };
        let (base, terminator) = match named {
            Named::PageUp => ("5", SequenceTerminator::Normal('~')),
            Named::PageDown => ("6", SequenceTerminator::Normal('~')),
            Named::Insert => ("2", SequenceTerminator::Normal('~')),
            Named::Delete => ("3", SequenceTerminator::Normal('~')),
            Named::Home => (one_based, SequenceTerminator::Normal('H')),
            Named::End => (one_based, SequenceTerminator::Normal('F')),
            Named::ArrowLeft => (one_based, SequenceTerminator::Normal('D')),
            Named::ArrowRight => (one_based, SequenceTerminator::Normal('C')),
            Named::ArrowUp => (one_based, SequenceTerminator::Normal('A')),
            Named::ArrowDown => (one_based, SequenceTerminator::Normal('B')),
            Named::F1 => (one_based, SequenceTerminator::Normal('P')),
            Named::F2 => (one_based, SequenceTerminator::Normal('Q')),
            Named::F3 => (one_based, SequenceTerminator::Normal('R')),
            Named::F4 => (one_based, SequenceTerminator::Normal('S')),
            Named::F5 => ("15", SequenceTerminator::Normal('~')),
            Named::F6 => ("17", SequenceTerminator::Normal('~')),
            Named::F7 => ("18", SequenceTerminator::Normal('~')),
            Named::F8 => ("19", SequenceTerminator::Normal('~')),
            Named::F9 => ("20", SequenceTerminator::Normal('~')),
            Named::F10 => ("21", SequenceTerminator::Normal('~')),
            Named::F11 => ("23", SequenceTerminator::Normal('~')),
            Named::F12 => ("24", SequenceTerminator::Normal('~')),
            Named::F13 => ("25", SequenceTerminator::Normal('~')),
            Named::F14 => ("26", SequenceTerminator::Normal('~')),
            Named::F15 => ("28", SequenceTerminator::Normal('~')),
            Named::F16 => ("29", SequenceTerminator::Normal('~')),
            Named::F17 => ("31", SequenceTerminator::Normal('~')),
            Named::F18 => ("32", SequenceTerminator::Normal('~')),
            Named::F19 => ("33", SequenceTerminator::Normal('~')),
            Named::F20 => ("34", SequenceTerminator::Normal('~')),
            _ => return None,
        };

        Some(SequenceBase::new(base, terminator))
    }

    /// Try building escape from control characters (e.g. Enter) and
    /// modifiers.
    fn try_build_control_char_or_mod(
        &self,
        input: &KeyInput,
        mods: &mut SequenceModifiers,
    ) -> Option<SequenceBase> {
        if !self.kitty_encode_all && !self.kitty_seq {
            return None;
        }

        let named = match input.modified_key {
            Key::Named(named) => *named,
            _ => return None,
        };

        let base = match named {
            Named::Tab => "9",
            Named::Enter => "13",
            Named::Escape => "27",
            Named::Space => "32",
            Named::Backspace => "127",
            _ => "",
        };

        // Fail when the key is not a named control character and the active
        // mode prohibits us from encoding modifier keys.
        if !self.kitty_encode_all && base.is_empty() {
            return None;
        }

        let left = input.location == Location::Left;
        let base = match named {
            Named::Shift if left => "57441",
            Named::Control if left => "57442",
            Named::Alt if left => "57443",
            Named::Super if left => "57444",
            Named::Hyper if left => "57445",
            Named::Meta if left => "57446",
            Named::Shift => "57447",
            Named::Control => "57448",
            Named::Alt => "57449",
            Named::Super => "57450",
            Named::Hyper => "57451",
            Named::Meta => "57452",
            Named::CapsLock => "57358",
            Named::NumLock => "57360",
            _ => base,
        };

        // NOTE: Kitty's protocol mandates that the modifier state is applied
        // before key press, however the windowing layer reports it after the
        // key press, so for modifiers itself apply the state based on the key
        // and not the _actual_ modifiers state, which is how kitty is doing
        // so and what is suggested in such case.
        let press = input.state != KeyState::Released;
        match named {
            Named::Shift => mods.set(SequenceModifiers::SHIFT, press),
            Named::Control => mods.set(SequenceModifiers::CONTROL, press),
            Named::Alt => mods.set(SequenceModifiers::ALT, press),
            Named::Super => mods.set(SequenceModifiers::SUPER, press),
            _ => (),
        }

        if base.is_empty() {
            None
        } else {
            Some(SequenceBase::new(base, SequenceTerminator::Kitty))
        }
    }
}

struct SequenceBase {
    /// The base of the payload, which is the `number` and optionally an alt
    /// base from the kitty spec.
    payload: String,
    terminator: SequenceTerminator,
}

impl SequenceBase {
    fn new(payload: impl Into<String>, terminator: SequenceTerminator) -> Self {
        Self {
            payload: payload.into(),
            terminator,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SequenceTerminator {
    /// The normal key esc sequence terminator defined by xterm/dec.
    Normal(char),
    /// The terminator is for kitty escape sequence.
    Kitty,
}

impl SequenceTerminator {
    fn encode_esc_sequence(self) -> char {
        match self {
            SequenceTerminator::Normal(char) => char,
            SequenceTerminator::Kitty => 'u',
        }
    }
}

/// The modifiers encoding for escape sequence.
///
/// NOTE: Kitty protocol defines additional modifiers to what is present here,
/// like Hyper, Meta, CapsLock and NumLock, but iced doesn't report them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SequenceModifiers(u8);

impl SequenceModifiers {
    const SHIFT: Self = Self(0b0000_0001);
    const ALT: Self = Self(0b0000_0010);
    const CONTROL: Self = Self(0b0000_0100);
    const SUPER: Self = Self(0b0000_1000);

    fn empty() -> Self {
        Self(0)
    }

    fn is_empty(self) -> bool {
        self.0 == 0
    }

    fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    fn set(&mut self, other: Self, value: bool) {
        if value {
            self.0 |= other.0;
        } else {
            self.0 &= !other.0;
        }
    }

    /// Get the value which should be passed to escape sequence.
    fn encode_esc_sequence(self) -> u8 {
        self.0 + 1
    }
}

impl From<Modifiers> for SequenceModifiers {
    fn from(mods: Modifiers) -> Self {
        let mut modifiers = Self::empty();
        modifiers.set(Self::SHIFT, mods.shift());
        modifiers.set(Self::ALT, mods.alt());
        modifiers.set(Self::CONTROL, mods.control());
        modifiers.set(Self::SUPER, mods.logo());
        modifiers
    }
}

/// Check whether the `text` is `0x7f`, `C0` or `C1` control code.
fn is_control_character(text: &str) -> bool {
    // 0x7f (DEL) is included here since it has a dedicated control code (`^?`)
    // which generally does not match the reported text (`^H`), despite not
    // technically being part of C0 or C1.
    let codepoint = text.bytes().next().unwrap();
    text.len() == 1 && (codepoint < 0x20 || (0x7f..=0x9f).contains(&codepoint))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISAMBIGUATE: TermMode = TermMode::DISAMBIGUATE_ESC_CODES;
    const EVENT_TYPES: TermMode = TermMode::REPORT_EVENT_TYPES;
    const ALTERNATE: TermMode = TermMode::REPORT_ALTERNATE_KEYS;
    const ALL_AS_ESC: TermMode = TermMode::REPORT_ALL_KEYS_AS_ESC;
    const TEXT: TermMode = TermMode::REPORT_ASSOCIATED_TEXT;

    /// A key event as iced reports it.
    struct Ev {
        key: Key,
        modified_key: Key,
        location: Location,
        modifiers: Modifiers,
        text: Option<String>,
        state: KeyState,
    }

    impl Ev {
        fn named(named: Named) -> Self {
            Self {
                key: Key::Named(named),
                modified_key: Key::Named(named),
                location: Location::Standard,
                modifiers: Modifiers::empty(),
                text: named_to_text(named).map(str::to_owned),
                state: KeyState::Pressed,
            }
        }

        /// A character key: `key` is the unmodified character, `modified`
        /// the one with Shift (but not Ctrl) applied, `text` what it types.
        fn character(key: &str, modified: &str, text: Option<&str>) -> Self {
            Self {
                key: Key::Character(key.into()),
                modified_key: Key::Character(modified.into()),
                location: Location::Standard,
                modifiers: Modifiers::empty(),
                text: text.map(str::to_owned),
                state: KeyState::Pressed,
            }
        }

        fn mods(mut self, modifiers: Modifiers) -> Self {
            self.modifiers = modifiers;
            self
        }

        fn location(mut self, location: Location) -> Self {
            self.location = location;
            self
        }

        fn repeat(mut self) -> Self {
            self.state = KeyState::Repeat;
            self
        }

        fn release(mut self) -> Self {
            self.state = KeyState::Released;
            self.text = None;
            self
        }

        fn encode(&self, mode: TermMode) -> Option<KittyOutput> {
            encode(
                &KeyInput {
                    key: &self.key,
                    modified_key: &self.modified_key,
                    location: self.location,
                    modifiers: self.modifiers,
                    text: self.text.as_deref(),
                    state: self.state,
                },
                mode,
            )
        }
    }

    fn write(bytes: &str) -> Option<KittyOutput> {
        Some(KittyOutput::Write(bytes.as_bytes().to_vec()))
    }

    const LEGACY: Option<KittyOutput> = Some(KittyOutput::Legacy);
    const NOTHING: Option<KittyOutput> = Some(KittyOutput::Nothing);

    #[test]
    fn no_kitty_mode_is_left_to_the_legacy_encoding() {
        let mode = TermMode::APP_CURSOR | TermMode::BRACKETED_PASTE;
        assert_eq!(Ev::named(Named::Escape).encode(mode), None);
        assert_eq!(
            Ev::character("i", "i", Some("\t"))
                .mods(Modifiers::CTRL)
                .encode(mode),
            None
        );
        assert_eq!(Ev::named(Named::ArrowUp).release().encode(mode), None);
    }

    #[test]
    fn disambiguate_escape_and_control_keys() {
        assert_eq!(
            Ev::named(Named::Escape).encode(DISAMBIGUATE),
            write("\x1b[27u")
        );
        // Enter, Tab, Backspace and Space keep their legacy bytes...
        for named in [Named::Enter, Named::Tab, Named::Backspace, Named::Space]
        {
            assert_eq!(Ev::named(named).encode(DISAMBIGUATE), LEGACY);
        }
        // ...unless modified.
        assert_eq!(
            Ev::named(Named::Enter)
                .mods(Modifiers::SHIFT)
                .encode(DISAMBIGUATE),
            write("\x1b[13;2u")
        );
        assert_eq!(
            Ev::named(Named::Tab)
                .mods(Modifiers::SHIFT)
                .encode(DISAMBIGUATE),
            write("\x1b[9;2u")
        );
        assert_eq!(
            Ev::named(Named::Backspace)
                .mods(Modifiers::CTRL)
                .encode(DISAMBIGUATE),
            write("\x1b[127;5u")
        );
        // On macOS Option composes text, so it isn't a modifier for keys
        // with text.
        if !cfg!(target_os = "macos") {
            assert_eq!(
                Ev::named(Named::Escape)
                    .mods(Modifiers::ALT)
                    .encode(DISAMBIGUATE),
                write("\x1b[27;3u")
            );
        }
    }

    #[test]
    fn disambiguate_ctrl_i_from_tab() {
        let ctrl_i = Ev::character("i", "i", Some("\t")).mods(Modifiers::CTRL);
        assert_eq!(ctrl_i.encode(DISAMBIGUATE), write("\x1b[105;5u"));
        assert_eq!(Ev::named(Named::Tab).encode(DISAMBIGUATE), LEGACY);
    }

    #[test]
    fn disambiguate_text_keys() {
        // Plain and shifted text is still sent as text.
        assert_eq!(
            Ev::character("a", "a", Some("a")).encode(DISAMBIGUATE),
            write("a")
        );
        assert_eq!(
            Ev::character("a", "A", Some("A"))
                .mods(Modifiers::SHIFT)
                .encode(DISAMBIGUATE),
            write("A")
        );
        // Alt, Ctrl and combinations are encoded.
        if !cfg!(target_os = "macos") {
            assert_eq!(
                Ev::character("a", "a", Some("a"))
                    .mods(Modifiers::ALT)
                    .encode(DISAMBIGUATE),
                write("\x1b[97;3u")
            );
            assert_eq!(
                Ev::character("a", "a", Some("\x01"))
                    .mods(Modifiers::CTRL | Modifiers::ALT)
                    .encode(DISAMBIGUATE),
                write("\x1b[97;7u")
            );
        }
        assert_eq!(
            Ev::character("c", "c", Some("\x03"))
                .mods(Modifiers::CTRL)
                .encode(DISAMBIGUATE),
            write("\x1b[99;5u")
        );
        assert_eq!(
            Ev::character("a", "A", Some("\x01"))
                .mods(Modifiers::CTRL | Modifiers::SHIFT)
                .encode(DISAMBIGUATE),
            write("\x1b[97;6u")
        );
    }

    #[test]
    fn functional_keys() {
        assert_eq!(
            Ev::named(Named::F5).encode(DISAMBIGUATE),
            write("\x1b[15~")
        );
        assert_eq!(
            Ev::named(Named::F5)
                .mods(Modifiers::CTRL)
                .encode(DISAMBIGUATE),
            write("\x1b[15;5~")
        );
        assert_eq!(Ev::named(Named::F1).encode(DISAMBIGUATE), write("\x1b[P"));
        // F3 uses `CSI 13 ~` in the kitty protocol.
        assert_eq!(
            Ev::named(Named::F3).encode(DISAMBIGUATE),
            write("\x1b[13~")
        );
        assert_eq!(
            Ev::named(Named::F13).encode(DISAMBIGUATE),
            write("\x1b[57376u")
        );
        assert_eq!(
            Ev::named(Named::PageUp).encode(DISAMBIGUATE),
            write("\x1b[5~")
        );
        assert_eq!(
            Ev::named(Named::Delete)
                .mods(Modifiers::SHIFT)
                .encode(DISAMBIGUATE),
            write("\x1b[3;2~")
        );
    }

    #[test]
    fn arrows_with_modifiers() {
        assert_eq!(
            Ev::named(Named::ArrowUp).encode(DISAMBIGUATE),
            write("\x1b[A")
        );
        assert_eq!(
            Ev::named(Named::ArrowUp)
                .mods(Modifiers::SHIFT)
                .encode(DISAMBIGUATE),
            write("\x1b[1;2A")
        );
        assert_eq!(
            Ev::named(Named::ArrowLeft)
                .mods(Modifiers::CTRL | Modifiers::ALT)
                .encode(DISAMBIGUATE),
            write("\x1b[1;7D")
        );
        assert_eq!(
            Ev::named(Named::End)
                .mods(Modifiers::LOGO)
                .encode(DISAMBIGUATE),
            write("\x1b[1;9F")
        );
        // Application cursor mode keeps SS3 for unmodified cursor keys only.
        let app = DISAMBIGUATE | TermMode::APP_CURSOR;
        assert_eq!(Ev::named(Named::ArrowUp).encode(app), LEGACY);
        assert_eq!(Ev::named(Named::Home).encode(app), LEGACY);
        assert_eq!(
            Ev::named(Named::ArrowUp).mods(Modifiers::CTRL).encode(app),
            write("\x1b[1;5A")
        );
    }

    #[test]
    fn numpad_keys() {
        let kp =
            |c: &str| Ev::character(c, c, Some(c)).location(Location::Numpad);
        assert_eq!(kp("1").encode(DISAMBIGUATE), write("\x1b[57400u"));
        assert_eq!(kp("+").encode(DISAMBIGUATE), write("\x1b[57413u"));
        assert_eq!(
            Ev::named(Named::Enter)
                .location(Location::Numpad)
                .encode(DISAMBIGUATE),
            write("\x1b[57414u")
        );
    }

    #[test]
    fn event_types() {
        let mode = DISAMBIGUATE | EVENT_TYPES;
        // A press is the default event type and isn't spelled out.
        assert_eq!(Ev::named(Named::Escape).encode(mode), write("\x1b[27u"));
        assert_eq!(
            Ev::named(Named::Escape).repeat().encode(mode),
            write("\x1b[27;1:2u")
        );
        assert_eq!(
            Ev::named(Named::Escape).release().encode(mode),
            write("\x1b[27;1:3u")
        );
        assert_eq!(
            Ev::named(Named::ArrowUp)
                .mods(Modifiers::SHIFT)
                .release()
                .encode(mode),
            write("\x1b[1;2:3A")
        );
        assert_eq!(
            Ev::named(Named::F5).release().encode(mode),
            write("\x1b[15;1:3~")
        );
        // A text key's press stays text, its release is reported.
        let a = Ev::character("a", "a", Some("a"));
        assert_eq!(a.encode(mode), write("a"));
        assert_eq!(a.release().encode(mode), write("\x1b[97;1:3u"));
        // Enter, Tab and Backspace releases need REPORT_ALL_KEYS_AS_ESC.
        assert_eq!(Ev::named(Named::Enter).release().encode(mode), NOTHING);
        assert_eq!(
            Ev::named(Named::Enter).release().encode(mode | ALL_AS_ESC),
            write("\x1b[13;1:3u")
        );
        // Without REPORT_EVENT_TYPES releases are dropped.
        assert_eq!(
            Ev::named(Named::Escape).release().encode(DISAMBIGUATE),
            NOTHING
        );
    }

    #[test]
    fn report_all_keys_as_escape_codes() {
        let mode = DISAMBIGUATE | ALL_AS_ESC;
        assert_eq!(Ev::named(Named::Enter).encode(mode), write("\x1b[13u"));
        assert_eq!(Ev::named(Named::Tab).encode(mode), write("\x1b[9u"));
        assert_eq!(
            Ev::named(Named::Backspace).encode(mode),
            write("\x1b[127u")
        );
        assert_eq!(Ev::named(Named::Space).encode(mode), write("\x1b[32u"));
        assert_eq!(
            Ev::character("a", "a", Some("a")).encode(mode),
            write("\x1b[97u")
        );
        assert_eq!(
            Ev::character("a", "A", Some("A"))
                .mods(Modifiers::SHIFT)
                .encode(mode),
            write("\x1b[97;2u")
        );
        // Modifier keys themselves, with their own state applied.
        assert_eq!(
            Ev::named(Named::Shift)
                .location(Location::Left)
                .mods(Modifiers::empty())
                .encode(mode),
            write("\x1b[57441;2u")
        );
        assert_eq!(
            Ev::named(Named::Control)
                .location(Location::Right)
                .mods(Modifiers::CTRL)
                .release()
                .encode(mode | EVENT_TYPES),
            write("\x1b[57448;1:3u")
        );
        // Without REPORT_ALL_KEYS_AS_ESC modifier keys send nothing.
        assert_eq!(
            Ev::named(Named::Shift)
                .location(Location::Left)
                .encode(DISAMBIGUATE),
            NOTHING
        );
    }

    #[test]
    fn alternate_keys() {
        let mode = DISAMBIGUATE | ALL_AS_ESC | ALTERNATE;
        assert_eq!(
            Ev::character("a", "A", Some("A"))
                .mods(Modifiers::SHIFT)
                .encode(mode),
            write("\x1b[97:65;2u")
        );
        // The base key of a shifted symbol comes from the unmodified key.
        assert_eq!(
            Ev::character("1", "!", Some("!"))
                .mods(Modifiers::SHIFT)
                .encode(mode),
            write("\x1b[49:33;2u")
        );
        assert_eq!(
            Ev::character("a", "a", Some("a")).encode(mode),
            write("\x1b[97u")
        );
    }

    #[test]
    fn associated_text() {
        let mode = DISAMBIGUATE | ALL_AS_ESC | TEXT;
        assert_eq!(
            Ev::character("a", "a", Some("a")).encode(mode),
            write("\x1b[97;1;97u")
        );
        assert_eq!(
            Ev::character("a", "A", Some("A"))
                .mods(Modifiers::SHIFT)
                .encode(mode),
            write("\x1b[97;2;65u")
        );
        // Control characters are not reported as text.
        assert_eq!(
            Ev::character("c", "c", Some("\x03"))
                .mods(Modifiers::CTRL)
                .encode(mode),
            write("\x1b[99;5u")
        );
        // Nor is text on release.
        assert_eq!(
            Ev::character("a", "a", None)
                .release()
                .encode(mode | EVENT_TYPES),
            write("\x1b[97;1:3u")
        );
    }

    #[test]
    fn event_types_without_disambiguate() {
        // Only REPORT_EVENT_TYPES: plain text and control codes keep their
        // bytes on press, Alt prefixes an escape.
        let mode = EVENT_TYPES;
        assert_eq!(
            Ev::character("c", "c", Some("\x03"))
                .mods(Modifiers::CTRL)
                .encode(mode),
            write("\x03")
        );
        let alt_x = Ev::character("x", "x", Some("x")).mods(Modifiers::ALT);
        if cfg!(target_os = "macos") {
            assert_eq!(alt_x.encode(mode), write("x"));
        } else {
            assert_eq!(alt_x.encode(mode), write("\x1bx"));
        }
        assert_eq!(Ev::named(Named::Escape).encode(mode), LEGACY);
        assert_eq!(Ev::named(Named::ArrowDown).encode(mode), write("\x1b[B"));
    }
}
