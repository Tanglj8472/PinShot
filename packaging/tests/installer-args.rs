//! Record auto-update installer arguments without modifying a real installation.
fn main() {
    let exe = std::env::current_exe().unwrap();
    std::fs::write(
        exe.with_extension("args.txt"),
        std::env::args().skip(1).collect::<Vec<_>>().join("\n"),
    )
    .unwrap();
}
