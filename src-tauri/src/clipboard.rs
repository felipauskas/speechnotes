use crate::errors::AppResult;

pub struct ClipboardManager;

impl ClipboardManager {
    #[cfg(target_os = "macos")]
    pub fn write_text(text: &str) -> AppResult<()> {
        use objc2_app_kit::NSPasteboard;
        use objc2_foundation::NSString;

        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        let ns_str = NSString::from_str(text);
        unsafe {
            let _ = pb.setString_forType(&ns_str, objc2_app_kit::NSPasteboardTypeString);
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn write_text(_text: &str) -> AppResult<()> {
        Ok(())
    }
}
