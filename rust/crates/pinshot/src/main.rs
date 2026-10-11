//! PinShot Windows binary. Non-Windows builds print a message and exit.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!(
            "PinShot {} (windows-rust)",
            pinshot_core::VERSION
        );
        return;
    }

    #[cfg(windows)]
    {
        if let Err(e) = pinshot::run(args) {
            eprintln!("PinShot: {e}");
            std::process::exit(1);
        }
    }

    #[cfg(not(windows))]
    {
        let _ = args;
        eprintln!("PinShot Rust 目前仅支持 Windows 10/11。请在 Windows 上构建：");
        eprintln!("  cd rust && cargo build --release");
        std::process::exit(1);
    }
}
