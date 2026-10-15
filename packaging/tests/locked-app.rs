//! Simulate an old installed version that holds its executable and an OCR DLL.
//! It deliberately ignores the new shutdown flag, reproducing upgrades from
//! versions that cannot implement the installer's process cleanup protocol.
#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> isize;
    #[cfg(restart_app)]
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    #[cfg(restart_app)]
    fn TerminateProcess(handle: isize, code: u32) -> i32;
    #[cfg(restart_app)]
    fn CloseHandle(handle: isize) -> i32;
}

fn main() {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap();
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--quit-for-install") {
        #[cfg(restart_app)]
        {
            if let Ok(pid) = std::fs::read_to_string(dir.join("fixture-pid.txt")) {
                let pid: u32 = pid.trim().parse().unwrap();
                unsafe {
                    let handle = OpenProcess(1, 0, pid);
                    if handle != 0 {
                        TerminateProcess(handle, 0);
                        CloseHandle(handle);
                    }
                }
            }
            return;
        }
        #[cfg(not(restart_app))]
        {
            std::fs::write(dir.join("old-helper-was-called.txt"), "unsupported flag").unwrap();
            #[cfg(fail_shutdown)]
            std::process::exit(23);
            return;
        }
    }
    #[cfg(windows)]
    unsafe {
        let dll = dir.join("webgpu_dawn.dll");
        let name: Vec<_> = dll
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        assert_ne!(
            LoadLibraryW(name.as_ptr()),
            0,
            "could not lock OCR runtime DLL"
        );
    }
    std::fs::write(dir.join("fixture-ready.txt"), "running").unwrap();
    std::fs::write(dir.join("fixture-pid.txt"), std::process::id().to_string()).unwrap();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
