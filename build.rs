fn main() {
    let target = std::env::var("TARGET").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let file_name = if target.contains("windows") {
        "linguaspark.dll"
    } else if target.contains("apple") {
        "liblinguaspark.dylib"
    } else {
        "liblinguaspark.so"
    };
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let dynlib_path = format!("{}/linguaspark/build/", manifest_dir);
    if std::path::PathBuf::from(&dynlib_path)
        .join(file_name)
        .exists()
    {
        println!("cargo:rustc-link-search={}", dynlib_path);
    } else {
        let archive_name = match target.as_str() {
            target if target.starts_with("x86_64") && target.contains("linux") => {
                "linguaspark-x86_64-linux.tar.gz"
            }
            target if target.starts_with("aarch64") && target.contains("linux") => {
                "linguaspark-aarch64-linux.tar.gz"
            }
            _ => panic!("Please build the dynamic library for target {target}"),
        };

        let url =
            format!("https://github.com/LinguaSpark/core/releases/download/latest/{archive_name}");

        println!("cargo:warning=Downloading {}", url);
        let resp = minreq::get(url).send().unwrap();
        if resp.status_code != 200 {
            panic!("Download failed: {}", resp.status_code);
        }

        let archive_path = std::path::PathBuf::from(&out_dir).join(archive_name);
        std::fs::write(&archive_path, resp.as_bytes()).unwrap();

        println!("cargo:warning=Unpacking {}", archive_path.display());
        let status = std::process::Command::new("tar")
            .arg("xzf")
            .arg(&archive_path)
            .current_dir(&out_dir)
            .status()
            .expect("Failed to execute tar command");

        if !status.success() {
            panic!("Failed to unpack the archive");
        }

        std::fs::remove_file(archive_path).unwrap_or_else(|e| {
            println!("cargo:warning=Failed to remove archive file: {}", e);
        });

        let extracted_lib_path = std::path::PathBuf::from(&out_dir).join("liblinguaspark.so");
        if !extracted_lib_path.exists() {
            panic!("Failed to find the extracted library file");
        }

        println!("cargo:rustc-link-search={}", out_dir);
    }

    if !target.contains("windows") {
        let rpath_arg = if target.contains("apple") {
            "-Wl,-rpath,@loader_path"
        } else {
            "-Wl,-rpath,$ORIGIN"
        };
        println!("cargo:rustc-link-arg={}", rpath_arg);
    }
    println!("cargo:rustc-link-lib=linguaspark");
    println!("cargo:rerun-if-changed=build.rs");
}
