use std::{env, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=fbs/");

    if env::var("SKIP_FLATC").unwrap_or_default() == "1" {
        return;
    }

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
    let out_dir = Path::new("../../sdks/frontend-java/client/src/main/generated/");
    let gen_path_buf = out_dir.join("com/automq/elasticstream/client/flatc/");
    let gen_dir = gen_path_buf.as_path();
    // clean up the directory
    if gen_dir.exists() {
        std::fs::remove_dir_all(gen_dir).expect("Failed to remove directory");
    }
    std::fs::create_dir_all(gen_dir).expect("Failed to create directory");

    flatc_rust::run(flatc_rust::Args {
        lang: "java",
        inputs: &[Path::new("fbs/rpc.fbs"), Path::new("fbs/model.fbs")],
        out_dir,
        extra: &[
            "--gen-object-api",
            "--java-package-prefix",
            "com.automq.elasticstream.client.flatc",
        ],
        ..Default::default()
    })
    .expect("flatc");
}
