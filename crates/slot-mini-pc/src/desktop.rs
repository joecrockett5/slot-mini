//! What only the desktop can do: native file dialogs and opening a page
//! in the browser. On Windows these are the system's own (through `rfd`);
//! elsewhere (the Linux build is for tests and development) there are no
//! dialogs and the answers are "nothing chosen".

use std::path::{Path, PathBuf};

/// Ask for a folder, starting in `start` when it exists.
pub fn pick_folder(title: &str, start: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if start.is_dir() {
            dialog = dialog.set_directory(start);
        }
        dialog.pick_folder()
    }
    #[cfg(not(windows))]
    {
        eprintln!(
            "slot-mini-pc: no folder dialog here ({title}, {})",
            start.display()
        );
        None
    }
}

/// Ask for the file called `name` (`retroarch.exe`).
pub fn pick_file(title: &str, name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let ext = name.rsplit('.').next().unwrap_or("exe");
        rfd::FileDialog::new()
            .set_title(title)
            .set_file_name(name)
            .add_filter(name, &[ext])
            .pick_file()
    }
    #[cfg(not(windows))]
    {
        eprintln!("slot-mini-pc: no file dialog here ({title}, {name})");
        None
    }
}

/// Open a web page in the player's browser. Only `https` pages: the URL
/// comes from GitHub's answer, and nothing else should be run.
pub fn open_url(url: &str) {
    if !url.starts_with("https://") || url.contains(['"', ' ', '&', '|', '^']) {
        eprintln!("slot-mini-pc: not opening {url}");
        return;
    }
    let result = if cfg!(windows) {
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if let Err(e) = result {
        eprintln!("slot-mini-pc: {url}: {e}");
    }
}
