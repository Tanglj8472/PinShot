use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../../assets/pinshot.ico");

    #[cfg(windows)]
    {
        let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
        let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"))
            .join("../../../assets/pinshot.ico")
            .canonicalize()
            .expect("pinshot.ico");
        let version = env::var("CARGO_PKG_VERSION").expect("package version");
        let parts: Vec<u32> = version
            .split('.')
            .map(|part| part.parse().expect("numeric package version"))
            .chain(std::iter::repeat(0))
            .take(4)
            .collect();
        let icon = icon.to_string_lossy().replace('\\', "\\\\");
        let rc = format!(
            r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {a},{b},{c},{d}
PRODUCTVERSION {a},{b},{c},{d}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "PinShot Project\0"
      VALUE "FileDescription", "PinShot screen capture and annotation tool\0"
      VALUE "FileVersion", "{version}\0"
      VALUE "InternalName", "pinshot.exe\0"
      VALUE "OriginalFilename", "pinshot.exe\0"
      VALUE "ProductName", "PinShot\0"
      VALUE "ProductVersion", "{version}\0"
      VALUE "LegalCopyright", "Copyright (c) 2026 PinShot Project\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#,
            a = parts[0], b = parts[1], c = parts[2], d = parts[3]
        );
        let rc_path = out.join("pinshot-version.rc");
        fs::write(&rc_path, rc).expect("write Windows resource");
        let _ = embed_resource::compile(rc_path, embed_resource::NONE);
    }
}
