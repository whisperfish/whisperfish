/* Copyright (C) 2018 Olivier Goffart <ogoffart@woboq.com>
Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the "Software"), to deal in the Software without restriction,
including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense,
and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:
The above copyright notice and this permission notice shall be included in all copies or substantial
portions of the Software.
THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES
OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
*/

use std::{io::Read, process::Command};

fn verify_sha384(path: &std::path::Path, hash: &str) -> bool {
    use sha2::{Digest, Sha384};
    let mut hasher = Sha384::new();

    let mut file = std::fs::File::open(path).unwrap();
    let mut buf = [0; 1024];
    loop {
        let n = file.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    let result = hasher.finalize();

    hex::encode(result) == hash
}

fn configure_webrtc() -> anyhow::Result<()> {
    let base_url = "https://nas.rubdos.be/~rsmet/webrtc/";

    // ringrtc expects `libwebrtc.a` in $OUTPUT_DIR/release/obj/libwebrtc.a
    // Don't confuse OUTPUT_DIR with OUT_DIR... ringrtc is special.
    // We download it, and verify the SHA384 hash.
    // Keep in sync with `fetch-webrtc.sh`
    let files = [
        ("arm", "56d4809b7d034816185b2f165a56514e29a799a6a5be1528a53c72a42990e275bf6c2895076fce991704f9899acfe280"),
        ("arm64", "28e0605917aa99b34303ee8b59eb0495b2bb3056ca9be2a5a553d34ac21d067324afd0bef06ac91cb266a7ad04dac4ba"),
        ("x64", "29db5abda6f5a9ccfa4d748f295a16b212b275bcf1441ac3856de6ee6cff855b89e6cf3a510d4da4d0abdcbcd3553434"),
        ("x86", "89143eb3464547263770cffc66bb741e4407366ac4a21e695510fb3474ddef4b5bf30eb5b1abac3060b1d9b562c6cbab"),
    ].iter().cloned().collect::<std::collections::HashMap<&str, &str>>();

    // This maps the target arch to the webrtc arch. Google has weird conventions
    let archs = [
        ("armv7-unknown-linux-gnueabihf", "arm"),
        ("aarch64-unknown-linux-gnu", "arm64"),
        ("x86_64-unknown-linux-gnu", "x64"),
        ("i686-unknown-linux-gnu", "x86"),
    ]
    .iter()
    .cloned()
    .collect::<std::collections::HashMap<&str, &str>>();

    let webrtc_arch = archs[&std::env::var("TARGET").unwrap().as_str()];
    let sha384 = files[webrtc_arch];

    let target_path = format!(
        "{}/release/obj/libwebrtc.a",
        std::env::var("OUTPUT_DIR").unwrap_or_else(|_| {
            let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
            let target = std::env::var("TARGET").unwrap();
            format!("{manifest}/../ringrtc/{target}")
        })
    );
    let target_path = std::path::Path::new(&target_path);

    if !target_path.exists() {
        panic!(
            "{} does not exist. Please download the correct libwebrtc.a from {} (use `bash fetch-webrtc.sh`)",
            target_path.display(),
            base_url,
        );
    }
    if !verify_sha384(target_path, sha384) {
        panic!(
            "SHA384 does not check out. Please download the correct libwebrtc.a from {} and place it in {} (use `bash fetch-webrtc.sh`)",
            base_url,
            target_path.display()
        );
    }

    Ok(())
}

fn qmake_query(var: &str) -> Result<String, std::io::Error> {
    let output = match std::env::var("QMAKE") {
        Ok(env_var_value) => Command::new(env_var_value).args(["-query", var]).output(),
        Err(_env_var_err) => Command::new("qmake")
            .args(["-query", var])
            .output()
            .or_else(|command_err| {
                // Some Linux distributions (Fedora, Arch) rename qmake to qmake-qt5.
                if command_err.kind() == std::io::ErrorKind::NotFound {
                    Command::new("qmake-qt5").args(["-query", var]).output()
                } else {
                    Err(command_err)
                }
            }),
    }?;
    if !output.status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "qmake returned with error:\n{}\n{}",
                std::str::from_utf8(&output.stderr).unwrap_or_default(),
                std::str::from_utf8(&output.stdout).unwrap_or_default()
            ),
        ));
    }

    Ok(std::str::from_utf8(&output.stdout)
        .expect("UTF-8 conversion failed")
        .trim()
        .to_string())
}

fn main() {
    let qt_include_path = qmake_query("QT_INSTALL_HEADERS").expect("QMAKE");
    let qt_include_path = qt_include_path.trim();

    let mut cfg = cpp_build::Config::new();

    // This is kinda hacky. Sorry.
    cfg.include(qt_include_path)
        .include(format!("{}/QtCore", qt_include_path))
        // -W deprecated-copy triggers some warnings in old Jolla's Qt distribution.
        // It is annoying to look at while developing, and we cannot do anything about it
        // ourselves.
        .flag("-Wno-deprecated-copy")
        .build("src/lib.rs");

    // Add lib.rs to the list, because it's the root of the CPP tree
    let contains_cpp = [
        "config/settings.rs",
        "lib.rs",
        "qblurhashimageprovider.rs",
        "qrustlegraphimageprovider.rs",
    ];
    for f in &contains_cpp {
        println!("cargo:rerun-if-changed=src/{}", f);
    }

    let macos_lib_search = if cfg!(target_os = "macos") {
        "=framework"
    } else {
        ""
    };
    let macos_lib_framework = if cfg!(target_os = "macos") { "" } else { "5" };

    let qt_libs = ["OpenGL", "Gui", "Core", "Quick", "Qml"];
    for lib in &qt_libs {
        println!(
            "cargo:rustc-link-lib{}=Qt{}{}",
            macos_lib_search, macos_lib_framework, lib
        );
    }

    let libs = ["dbus-1"];
    for lib in libs.iter() {
        println!("cargo:rustc-link-lib{}={}", macos_lib_search, lib);
    }

    configure_webrtc().unwrap();
}
