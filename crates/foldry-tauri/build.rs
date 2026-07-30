fn main() {
    for icon in ["icons/icon.icns", "icons/icon.ico", "icons/icon.png"] {
        println!("cargo:rerun-if-changed={icon}");
    }
    tauri_build::build();
}
