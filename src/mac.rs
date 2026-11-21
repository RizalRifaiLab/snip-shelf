//! Small bridges to macOS: the pasteboard change counter, file links on the clipboard,
//! and hiding/showing the app so focus goes back to the app you paste into.
#![allow(unused_unsafe)]

#[cfg(target_os = "macos")]
mod imp {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSPasteboard, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSString, NSURL};
    use std::path::{Path, PathBuf};

    pub fn change_count() -> isize {
        unsafe { NSPasteboard::generalPasteboard().changeCount() }
    }

    pub fn types() -> Vec<String> {
        unsafe {
            let pb = NSPasteboard::generalPasteboard();
            match pb.types() {
                Some(arr) => arr.iter().map(|s| s.to_string()).collect(),
                None => Vec::new(),
            }
        }
    }

    /// The first file copied (e.g. in Finder), if any.
    pub fn copied_file() -> Option<PathBuf> {
        unsafe {
            let pb = NSPasteboard::generalPasteboard();
            let s = pb.stringForType(&NSString::from_str("public.file-url"))?;
            let url = NSURL::URLWithString(&s)?;
            let path = url.path()?;
            Some(PathBuf::from(path.to_string()))
        }
    }

    /// Put a file on the clipboard, so Command+V pastes the file itself.
    pub fn set_file(path: &Path) -> bool {
        unsafe {
            let pb = NSPasteboard::generalPasteboard();
            pb.clearContents();
            let url: Retained<NSURL> =
                NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
            let obj: Retained<ProtocolObject<dyn NSPasteboardWriting>> =
                ProtocolObject::from_retained(url);
            let arr = NSArray::from_retained_slice(&[obj]);
            pb.writeObjects(&arr)
        }
    }

    pub fn hide_app() {
        if let Some(mtm) = MainThreadMarker::new() {
            let app = NSApplication::sharedApplication(mtm);
            unsafe { app.hide(None) };
        }
    }

    #[allow(deprecated)]
    pub fn show_app() {
        if let Some(mtm) = MainThreadMarker::new() {
            let app = NSApplication::sharedApplication(mtm);
            unsafe {
                app.unhide(None);
                app.activateIgnoringOtherApps(true);
                for w in app.windows().iter() {
                    if w.isMiniaturized() {
                        w.deminiaturize(None);
                    }
                }
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::path::{Path, PathBuf};
    pub fn change_count() -> isize {
        0
    }
    pub fn types() -> Vec<String> {
        Vec::new()
    }
    pub fn copied_file() -> Option<PathBuf> {
        None
    }
    pub fn set_file(_path: &Path) -> bool {
        false
    }
    pub fn hide_app() {}
    pub fn show_app() {}
}

pub use imp::*;
