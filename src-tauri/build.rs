fn main() {
    // The app icon is embedded at compile time; without this a changed icon keeps
    // the old one in incremental builds.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
