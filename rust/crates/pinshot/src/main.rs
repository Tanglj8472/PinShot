//! PinShot binary. Release Windows builds hide the console (`windows_subsystem`);
//! `--debug` / `PINSHOT_DEBUG` attach one and also log to the data dir.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--settings-window") {
        pinshot::run_preferences();
        return;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        pinshot::logutil_ensure();
        println!(
            "PinShot {} ({})",
            pinshot_core::VERSION,
            pinshot::platform_tag()
        );
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        pinshot::logutil_ensure();
        println!(
            "PinShot {} ({})\n  --snip  --pin-clipboard  --prefs  --translate  --debug  --version",
            pinshot_core::VERSION,
            pinshot::platform_tag()
        );
        return;
    }

    if let Err(e) = pinshot::run(args) {
        eprintln!("PinShot: {e}");
        std::process::exit(1);
    }
}
