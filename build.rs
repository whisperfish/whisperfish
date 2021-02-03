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
use std::env;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::Path;
use std::process::Command;

use failure::*;
use vergen::*;

fn qmake_query(var: &str) -> String {
    let qmake = std::env::var("QMAKE").unwrap_or("qmake".to_string());
    String::from_utf8(
        Command::new(qmake)
            .env("QT_SELECT", "qt5")
            .args(&["-query", var])
            .output()
            .expect("Failed to execute qmake. Make sure 'qmake' is in your path")
            .stdout,
    )
    .expect("UTF-8 conversion failed")
}

fn mock_pthread(mer_root: &str, arch: &str) -> Result<String, Error> {
    let out_dir = env::var("OUT_DIR")?;
    let qml_path = &Path::new(&out_dir).join("libpthread.so");

    let mut f = std::fs::File::create(qml_path)?;
    match arch {
        "armv7hl" => {
            writeln!(f, "OUTPUT_FORMAT(elf32-littlearm)")?;
        }
        "i486" => {
            writeln!(f, "OUTPUT_FORMAT(elf32-i386)")?;
        }
        "aarch64" => {
            writeln!(f, "OUTPUT_FORMAT(elf64-littleaarch64)")?;
        }
        _ => unreachable!(),
    }

    match arch {
        "armv7hl" | "i486" => writeln!(
            f,
            "GROUP ( {}/lib/libpthread.so.0 {}/usr/lib/libpthread_nonshared.a )",
            mer_root, mer_root
        )?,
        "aarch64" => writeln!(
            f,
            "GROUP ( {}/lib64/libpthread.so.0 {}/usr/lib64/libpthread_nonshared.a )",
            mer_root, mer_root
        )?,
        _ => unreachable!(),
    }

    Ok(out_dir)
}

fn mock_libc(mer_root: &str, arch: &str) -> Result<String, Error> {
    let out_dir = env::var("OUT_DIR")?;
    let qml_path = &Path::new(&out_dir).join("libc.so");

    let mut f = std::fs::File::create(qml_path)?;
    match arch {
        "armv7hl" => {
            writeln!(f, "OUTPUT_FORMAT(elf32-littlearm)")?;
            writeln!(f, "GROUP ( {}/lib/libc.so.6 {}/usr/lib/libc_nonshared.a  AS_NEEDED ( {}/lib/ld-linux-armhf.so.3 ))",
                mer_root, mer_root, mer_root)?;
        }
        "i486" => {
            writeln!(f, "OUTPUT_FORMAT(elf32-i386)")?;
            writeln!(f, "GROUP ( {}/lib/libc.so.6 {}/usr/lib/libc_nonshared.a  AS_NEEDED ( {}/lib/ld-linux.so.2 ))",
                mer_root, mer_root, mer_root)?;
        }
        "aarch64" => {
            writeln!(f, "OUTPUT_FORMAT(elf64-littleaarch64)")?;
            writeln!(f, "GROUP ( {}/lib64/libc.so.6 {}/usr/lib64/libc_nonshared.a  AS_NEEDED ( {}/lib64/ld-linux-aarch64.so.1 ))",
                mer_root, mer_root, mer_root)?;
        }
        _ => unreachable!(),
    }

    Ok(out_dir)
}

fn install_mer_hacks() -> (String, bool) {
    let mer_sdk = match std::env::var("MERSDK").ok() {
        Some(path) => path,
        None => return ("".into(), false),
    };

    let mer_target = std::env::var("MER_TARGET")
        .ok()
        .unwrap_or("SailfishOS-latest".into());

    let arch = match &std::env::var("CARGO_CFG_TARGET_ARCH").unwrap() as &str {
        "arm" => "armv7hl",
        "i686" => "i486",
        "x86" => "i486",
        "aarch64" => "aarch64",
        unsupported => panic!("Target {} is not supported for Mer", unsupported),
    };

    let lib_dir = match arch {
        "armv7hl" | "i486" => "lib",
        "aarch64" => "lib64",
        _ => unreachable!(),
    };

    println!("cargo:rustc-cfg=feature=\"sailfish\"");

    let mer_target_root = format!("{}/targets/{}-{}", mer_sdk, mer_target, arch);

    let mock_libc_path = mock_libc(&mer_target_root, arch).unwrap();
    let mock_pthread_path = mock_pthread(&mer_target_root, arch).unwrap();

    let macos_lib_search = if cfg!(target_os = "macos") {
        "=framework"
    } else {
        ""
    };

    println!(
        "cargo:rustc-link-search{}={}",
        macos_lib_search, mock_pthread_path,
    );
    println!(
        "cargo:rustc-link-search{}={}",
        macos_lib_search, mock_libc_path,
    );

    println!(
        "cargo:rustc-bin-link-arg=-rpath-link,{}/usr/{}",
        mer_target_root, lib_dir
    );
    println!(
        "cargo:rustc-bin-link-arg=-rpath-link,{}/{}",
        mer_target_root, lib_dir
    );

    println!(
        "cargo:rustc-link-search{}={}/toolings/{}/opt/cross/{}-meego-linux-gnueabi/{}",
        macos_lib_search, mer_sdk, mer_target, arch, lib_dir
    );

    println!(
        "cargo:rustc-link-search{}={}/usr/{}/qt5/qml/Nemo/Notifications/",
        macos_lib_search, mer_target_root, lib_dir
    );

    println!(
        "cargo:rustc-link-search{}={}/toolings/{}/opt/cross/{}/gcc/{}-meego-linux-gnueabi/4.9.4/",
        macos_lib_search, mer_sdk, mer_target, arch, lib_dir
    );

    println!(
        "cargo:rustc-link-search{}={}/usr/{}/",
        macos_lib_search, mer_target_root, lib_dir
    );

    (mer_target_root, true)
}

fn detect_qt_version(qt_include_path: &Path) -> Result<String, Error> {
    let path = qt_include_path.join("QtCore").join("qconfig.h");
    let f = std::fs::File::open(&path).expect(&format!("Cannot open `{:?}`", path));
    let b = BufReader::new(f);

    // append qconfig-64.h or config-32.h, depending on TARGET_POINTER_WIDTH
    let arch_specific: Box<dyn BufRead> = {
        let pointer_width = std::env::var("CARGO_CFG_TARGET_POINTER_WIDTH").unwrap();
        let path = qt_include_path
            .join("QtCore")
            .join(format!("qconfig-{}.h", pointer_width));
        match std::fs::File::open(&path) {
            Ok(f) => Box::new(BufReader::new(f)),
            Err(_) => Box::new(std::io::Cursor::new("")),
        }
    };

    let regex = regex::Regex::new("#define +QT_VERSION_STR +\"(.*)\"")?;

    for line in b.lines().chain(arch_specific.lines()) {
        let line = line.expect("qconfig.h is valid UTF-8");
        if let Some(capture) = regex.captures_iter(&line).next() {
            return Ok(capture[1].into());
        }
        if line.contains("QT_VERION_STR") {
            bail!("QT_VERSION_STR: {}, not matched by regex", line);
        }
    }
    bail!("Could not detect Qt version");
}

fn protobuf() -> Result<(), Error> {
    let protobuf = Path::new("protobuf").to_owned();

    let input: Vec<_> = protobuf
        .read_dir()
        .expect("protobuf directory")
        .filter_map(|entry| {
            let entry = entry.expect("readable protobuf directory");
            let path = entry.path();
            if Some("proto") == path.extension().and_then(std::ffi::OsStr::to_str) {
                assert!(path.is_file());
                println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
                Some(path)
            } else {
                None
            }
        })
        .collect();

    prost_build::compile_protos(&input, &[protobuf])?;
    Ok(())
}

fn main() {
    protobuf().unwrap();

    let (mer_target_root, cross_compile) = install_mer_hacks();
    let qt_include_path = if cross_compile {
        format!("{}/usr/include/qt5/", mer_target_root)
    } else {
        qmake_query("QT_INSTALL_HEADERS")
    };
    let qt_include_path = qt_include_path.trim();

    let mut cfg = cpp_build::Config::new();

    cfg.include(format!(
        "{}/QtGui/{}",
        qt_include_path,
        detect_qt_version(std::path::Path::new(&qt_include_path)).unwrap()
    ));

    // This is kinda hacky. Sorry.
    if cross_compile {
        std::env::set_var("CARGO_FEATURE_SAILFISH", "");
    }
    cfg.include(format!("{}/usr/include/", mer_target_root))
        .include(format!("{}/usr/include/sailfishapp/", mer_target_root))
        .include(&qt_include_path)
        .include(format!("{}/QtCore", qt_include_path))
        // -W deprecated-copy triggers some warnings in old Jolla's Qt distribution.
        // It is annoying to look at while developing, and we cannot do anything about it
        // ourselves.
        .flag("-Wno-deprecated-copy")
        .build("src/lib.rs");

    let contains_cpp = [
        "sfos/mod.rs",
        "sfos/tokio_qt.rs",
        "settings.rs",
        "sfos/native.rs",
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

    let sailfish_libs: &[&str] = if cross_compile {
        &["nemonotifications", "sailfishapp", "qt5embedwidget"]
    } else {
        &[]
    };
    let libs = ["EGL", "dbus-1"];
    for lib in libs.iter().chain(sailfish_libs.iter()) {
        println!("cargo:rustc-link-lib{}={}", macos_lib_search, lib);
    }

    // vergen
    let flags = ConstantsFlags::all();
    // Generate the 'cargo:' key output
    generate_cargo_keys(flags).expect("Unable to generate the cargo keys!");
}
