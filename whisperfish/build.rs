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

#[cfg(feature = "calling")]
use std::io::Read;
use std::process::Command;

#[cfg(feature = "calling")]
fn verify_sha384(path: &std::path::Path, hashes: &[&str]) -> bool {
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

    hashes.contains(&hex::encode(result).as_str())
}

#[cfg(feature = "calling")]
fn configure_webrtc() -> anyhow::Result<()> {
    let base_url = "https://nas.rubdos.be/~rsmet/webrtc/";

    // ringrtc expects `libwebrtc.a` in $OUTPUT_DIR/release/obj/libwebrtc.a
    // Don't confuse OUTPUT_DIR with OUT_DIR... ringrtc is special.
    // We download it, and verify the SHA384 hash.
    // Keep in sync with `fetch-webrtc.sh`
    //
    // There are three possible legal hashes for each arch, because we have two different versions of the library:
    // one is built with OpenSSL 1.1.1, one with OpenSSL 3.5.8, one with OpenSSL 3.6.5. These differ in ABI.
    let files = [
        ("arm", &[
            "3ee0e141db54729e3224f95032f02672f277fe462536d734bf4c21c42e4cad373e773a7ee327222bd1aba2b907bf9443",
            "7eccc8fc8347dce26d1f54a67d25c4b137035b73f59816a0cc30d79cfb0d9da5e5b39334b9564025c142cedc3b56a35d",
            "7a86dacfc4dea3fd94284ee22c6221ed9c4858ae23cd05bea8c335f25d0f3881ecf92fceacee346b2201e2205af79c3f",
        ] as &[&str]),
        ("arm64", &[
            "a30ec6e40c37fb61ec608373d68b2ce4e8c82cbdacc05967e37a293fe8f9bf16266d88b9b408ca409027d806bcb7d269",
            "7d1bf9a285ef55c4547fe013b6660e0600afd7fa3ac895af000264cde6d1b22891d42e970da892e5c809d71f1dbb4660",
            "0de6c8cec509cbaa7f5a33760ae87fdfac0e8e80c63247572064da11cf24ee5bdccc258f29486d016b2883fd8f2152c1",
        ]),
        ("x86", &[
            "697e6c14f3ee32bd80f69db12e062f6be16fedfbc9969911fc4a85e9004c801a8d59aea0b1c04d94cd183098f404dbe8",
            "a04061f6c83d407bc1ace23109ba4772a2d1ba2c3346f34d93c191a5947ccb0119eaba3ba4ab1e6b543e3c161c0fa0b2",
            "0702a115ff693eef0ba4534e2af9de43c3eaebf7bae58bfaf108a35ee5c0199e0c6e5adf6f0d7ef6ef4964be3f07c875",
        ]),
        ("x64", &[
            "1c1e0cf95316e914bf73a7962f92c00b5fd5d4297553a86b3693f62dfcec12cd34664a7d0b3ebd56188652322ebc8092",
            "dc077a619de00a7c57c5d87dcedcec59656bf51f7d02b733bdf851defc9496e71124b948f22455410a045d180053ffbd",
            "9cf4d8b9d036c784f92a1b8216879bb71f4939329eca09cace6f1496dda1af06a141c2c304d6e5d0b857755e542a6945",
        ]),
    ].iter().cloned().collect::<std::collections::HashMap<&str, &[&str]>>();

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
            format!("{manifest}/../ringrtc/358/{target}")
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
        return Err(std::io::Error::other(format!(
            "qmake returned with error:\n{}\n{}",
            std::str::from_utf8(&output.stderr).unwrap_or_default(),
            std::str::from_utf8(&output.stdout).unwrap_or_default()
        )));
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

    let qt_libs = ["Gui", "Core", "Quick", "Qml"];
    for lib in &qt_libs {
        println!(
            "cargo:rustc-link-lib{}=Qt{}{}",
            macos_lib_search, macos_lib_framework, lib
        );
    }

    #[cfg(feature = "voice-note-transcription")]
    println!("cargo:rustc-link-lib{}={}", macos_lib_search, "dbus-1");

    #[cfg(feature = "calling")]
    configure_webrtc().unwrap();
}

#[cfg(all(feature = "harbour", feature = "voice-note-transcription"))]
compile_error!("`harbour` feature cannot be used together with `voice-note-transcription`");

#[cfg(all(feature = "harbour", feature = "voice-note-recording"))]
compile_error!("`harbour` feature cannot be used together with `voice-note-recording`");
