use log::{debug, warn};
use winit::raw_window_handle::RawDisplayHandle;

use tankterm_terminal::term::ClipboardType;

#[cfg(any(target_os = "macos", windows))]
use copypasta::ClipboardContext;
use copypasta::ClipboardProvider;
use copypasta::nop_clipboard::NopClipboardContext;
#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use copypasta::wayland_clipboard;
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use x11::X11Clipboard;

pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
    selection: Option<Box<dyn ClipboardProvider>>,
}

impl Clipboard {
    pub unsafe fn new(display: RawDisplayHandle) -> Self {
        match display {
            #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
            RawDisplayHandle::Wayland(display) => {
                let (selection, clipboard) = unsafe {
                    wayland_clipboard::create_clipboards_from_external(display.display.as_ptr())
                };
                Self { clipboard: Box::new(clipboard), selection: Some(Box::new(selection)) }
            },
            _ => Self::default(),
        }
    }

    /// Used for tests, to handle missing clipboard provider when built without the `x11`
    /// feature, and as default clipboard value.
    pub fn new_nop() -> Self {
        Self { clipboard: Box::new(NopClipboardContext::new().unwrap()), selection: None }
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        #[cfg(any(target_os = "macos", windows))]
        return Self { clipboard: Box::new(ClipboardContext::new().unwrap()), selection: None };

        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
        return match (X11Clipboard::clipboard(), X11Clipboard::primary()) {
            (Ok(clipboard), Ok(selection)) => {
                Self { clipboard: Box::new(clipboard), selection: Some(Box::new(selection)) }
            },
            (Err(err), _) | (_, Err(err)) => {
                warn!("Unable to open the X11 clipboard: {err}");
                Self::new_nop()
            },
        };

        #[cfg(not(any(feature = "x11", target_os = "macos", windows)))]
        return Self::new_nop();
    }
}

impl Clipboard {
    pub fn store(&mut self, ty: ClipboardType, text: impl Into<String>) {
        let clipboard = match (ty, &mut self.selection) {
            (ClipboardType::Selection, Some(provider)) => provider,
            (ClipboardType::Selection, None) => return,
            _ => &mut self.clipboard,
        };

        clipboard.set_contents(text.into()).unwrap_or_else(|err| {
            warn!("Unable to store text in clipboard: {err}");
        });
    }

    pub fn load(&mut self, ty: ClipboardType) -> String {
        let clipboard = match (ty, &mut self.selection) {
            (ClipboardType::Selection, Some(provider)) => provider,
            _ => &mut self.clipboard,
        };

        match clipboard.get_contents() {
            Err(err) => {
                debug!("Unable to load text from clipboard: {err}");
                String::new()
            },
            Ok(text) => text,
        }
    }
}

/// X11 clipboard, using `arboard`.
///
/// This replaces `copypasta`'s X11 backend: the released `x11-clipboard` crate it uses checks
/// selection ownership right after storing, which can make copying fail with "Unable to store
/// text in clipboard" (alacritty/alacritty#6978).
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
mod x11 {
    use std::error::Error;

    use arboard::{GetExtLinux, LinuxClipboardKind, SetExtLinux};
    use copypasta::ClipboardProvider;

    pub struct X11Clipboard {
        clipboard: arboard::Clipboard,
        kind: LinuxClipboardKind,
    }

    impl X11Clipboard {
        /// The `CLIPBOARD` selection, used for explicit copy and paste.
        pub fn clipboard() -> Result<Self, arboard::Error> {
            Self::new(LinuxClipboardKind::Clipboard)
        }

        /// The `PRIMARY` selection, used for copy on select and middle-click paste.
        pub fn primary() -> Result<Self, arboard::Error> {
            Self::new(LinuxClipboardKind::Primary)
        }

        fn new(kind: LinuxClipboardKind) -> Result<Self, arboard::Error> {
            Ok(Self { clipboard: arboard::Clipboard::new()?, kind })
        }
    }

    impl ClipboardProvider for X11Clipboard {
        fn get_contents(&mut self) -> Result<String, Box<dyn Error + Send + Sync>> {
            Ok(self.clipboard.get().clipboard(self.kind).text()?)
        }

        fn set_contents(&mut self, text: String) -> Result<(), Box<dyn Error + Send + Sync>> {
            Ok(self.clipboard.set().clipboard(self.kind).text(text)?)
        }
    }
}
