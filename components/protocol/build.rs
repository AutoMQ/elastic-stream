use flatc_rust;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=fbs/");
    let out_dir = Path::new("src/generated/");
    if !out_dir.exists() {
        std::fs::create_dir_all(out_dir).unwrap();
    }

    flatc_rust::run(flatc_rust::Args {
        inputs: &[Path::new("fbs/rpc.fbs"), Path::new("fbs/model.fbs")],
        out_dir,
        extra: &["--gen-object-api"],
        ..Default::default()
    })
    .expect("flatc");

    println!("cargo:rerun-if-changed=fbs/");
    let out_dir = Path::new("../../sdks/java/lib/src/main/java");
    if !out_dir.exists() {
        std::fs::create_dir_all(out_dir);
    }
    flatc_rust::run(flatc_rust::Args {
        lang: "java",
        inputs: &[Path::new("fbs/rpc.fbs"), Path::new("fbs/model.fbs")],
        out_dir,
        extra: &[
            "--gen-object-api",
            "--java-package-prefix",
            "sdk.elastic.storage.flatc",
        ],
        ..Default::default()
    })
    .expect("flatc");
}
