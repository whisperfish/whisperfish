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

use std::process::Command;

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

    let libs = [
        // dbus for share plugins
        "dbus-1",
        // webrtc needs system ssl, and webrtc is not linked to it.
        // Enforce the link order here.
        "webrtc", "ssl", "crypto",
    ];
    for lib in libs.iter() {
        println!("cargo:rustc-link-lib{}={}", macos_lib_search, lib);
    }
}
