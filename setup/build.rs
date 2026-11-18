use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let payload = std::env::var("PRISMDISK_PAYLOAD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../target/release/PrismDisk.exe"));
    println!("cargo:rerun-if-env-changed=PRISMDISK_PAYLOAD");
    println!("cargo:rerun-if-changed={}", payload.display());
    println!("cargo:rerun-if-changed=setup.manifest");

    let (bytes, len) = match std::fs::read(&payload) {
        Ok(data) => {
            let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
            enc.write_all(&data).unwrap();
            (enc.finish().unwrap(), data.len() as u64)
        }
        Err(_) => {
            println!("cargo:warning=PrismDisk payload not found at {}, so the setup is built without an app (install will fail). Run build.ps1 for a real installer.", payload.display());
            (Vec::new(), 0)
        }
    };
    std::fs::write(out.join("payload.gz"), bytes).unwrap();
    std::fs::write(out.join("payload_len.rs"), format!("{len}u64")).unwrap();

    println!("cargo:rerun-if-changed=setup.rc");
    embed_resource::compile("setup.rc", embed_resource::NONE).manifest_optional().unwrap();
}
