use nextcloud_appinfo::get_appinfo;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=appinfo/info.xml");

    let appinfo_path: PathBuf = "".into();
    let appinfo = get_appinfo(&appinfo_path).expect("Failed to load appinfo");
    println!("cargo:rustc-env=NOTIFY_PUSH_VERSION={}", appinfo.version());
    println!("cargo:rustc-env=CARGO_PKG_VERSION={}", appinfo.version());
}
