//! PinShot binary. Release Windows builds hide the console (`windows_subsystem`);
//! `--debug` / `PINSHOT_DEBUG` attach one and also log to the data dir.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--settings-window") {
        pinshot::run_preferences();
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--quit-for-install") {
        let install_dir = args.get(i + 1).map(std::path::Path::new);
        if let Err(e) = pinshot::update_run::quit_for_install(install_dir) {
            eprintln!("PinShot quit-for-install: {e}");
            std::process::exit(1);
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--install-after-quit") {
        if let Some(path) = args.get(i + 1) {
            pinshot::logutil_ensure();
            if let Err(e) = pinshot::update_run::run_install_after_quit(std::path::Path::new(path))
            {
                eprintln!("PinShot install-after-quit: {e}");
                std::process::exit(1);
            }
            return;
        }
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
            "PinShot {} ({})\n  --snip  --pin-clipboard  --prefs  --translate  --debug  --version\n  --quit-for-install  --install-after-quit <setup.exe>",
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
