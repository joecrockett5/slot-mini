//! Make a library of made-up games to try the app on:
//! `cargo run -p slot-mini-pc --example fake_library -- <folder>`.

fn main() {
    let Some(dir) = std::env::args_os().nth(1) else {
        eprintln!("usage: fake_library <folder>");
        std::process::exit(2);
    };
    let dir = std::path::PathBuf::from(dir);
    slot_mini_pc::pcapp::make_library(&dir);
    if let Err(e) = slot_mini::fixture::build(&dir, true) {
        eprintln!("{}: {e}", dir.display());
        std::process::exit(1);
    }
    println!("{}", dir.display());
}
