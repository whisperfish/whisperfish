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
            .env("QT_SELECT", "5")
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
        "cargo:rustc-link-arg-bins=-rpath-link,{}/usr/{}",
        mer_target_root, lib_dir
    );
    println!(
        "cargo:rustc-link-arg-bins=-rpath-link,{}/{}",
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

fn prepare_rpm_build() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_HARBOUR");

    // create tmp folders where feature-dependend files are copied to if they should be included.
    // Define the whole folder in Cargo.toml for inlusion in the rpm
    let rpm_extra_dir = std::path::PathBuf::from(".rpm/tmp_feature_files");
    if rpm_extra_dir.exists() {
        std::fs::remove_dir_all(&rpm_extra_dir)
            .expect(&format!("Could not remove {:?} for cleanup", rpm_extra_dir));
    }
    let cond_folder: &[&str] = &["systemd"];
    for d in cond_folder.iter() {
        let nd = rpm_extra_dir.join(d);
        std::fs::create_dir_all(&nd).expect(&format!("Could not create {:?}", &nd));
    }
    let cond_files: &[(&str, &str)] = if env::var("CARGO_FEATURE_HARBOUR").is_err() {
        &[("harbour-whisperfish.service", "systemd")]
    } else {
        &[]
    };
    for (file, dest) in cond_files.iter() {
        let dest_dir = rpm_extra_dir.join(dest);
        if !dest_dir.exists() {
            std::fs::create_dir_all(&dest_dir).expect(&format!("Could not create {:?}", dest_dir));
        }
        let dest_file = dest_dir.join(file);
        std::fs::copy(Path::new(file), &dest_file)
            .expect(&format!("failed to copy {} to {:?}", file, dest_file));
        println!("cargo:rerun-if-changed={}", file);
    }

    // Build RPM Spec
    // Lines between `#[{{ NOT FEATURE_FLAG` and `#}}]` are only copied if the feature is disabled
    // (or enabled without NOT).
    println!("cargo:rerun-if-changed=rpm/harbour-whisperfish.spec");
    let src = std::fs::File::open("rpm/harbour-whisperfish.spec")
        .expect("Failed to read rpm spec at rpm/harbour-whisperfish.spec");
    let mut spec = std::fs::File::create(".rpm/harbour-whisperfish.spec")
        .expect("Failed to write rpm spec to .rpm/harbour-whisperfish.spec");
    writeln!(spec, "### WARNING: auto-generated file - please only edit the original source file: ../rpm/harbour-whisperfish.spec")
        .expect("Failed to write to spec file");

    let mut ignore = 0;
    let feature_re = regex::Regex::new(r"^\s*#\[\{\{\s+(NOT)?\s+([A-Z_0-9]+)").unwrap();

    for line in BufReader::new(src).lines() {
        let line = line.unwrap();
        if let Some(cap) = feature_re.captures(&line) {
            if ignore > 0
                || (cap.get(1) == None
                    && env::var(format!("CARGO_FEATURE_{}", cap.get(2).unwrap().as_str())).is_err())
                || (cap.get(1) != None
                    && env::var(format!("CARGO_FEATURE_{}", cap.get(2).unwrap().as_str())).is_ok())
            {
                ignore += 1;
            }
            println!("reg {:?}", cap);
        } else if line.trim_start().starts_with("#}}]") {
            if ignore > 0 {
                ignore -= 1;
            }
        } else if ignore == 0 {
            writeln!(spec, "{}", line).expect("Failed to write to spec file");
        }
    }
}

fn build_sqlcipher(mer_target_root: &str) {
    // static sqlcipher handling. Needed for compatibility with
    // sailfish-components-webview.
    // This may become obsolete with an sqlcipher upgrade from jolla or when
    // https://gitlab.com/rubdos/whisperfish/-/issues/227 is implemented.

    if !Path::new("sqlcipher/sqlite3.c").is_file() {
        // Download and prepare sqlcipher source
        let stat = Command::new("sqlcipher/get-sqlcipher.sh")
            .status()
            .expect("Failed to download sqlcipher");
        assert!(stat.success());
    }

    // Build static sqlcipher
    cc::Build::new()
        .flag(&format!("--sysroot={}", mer_target_root))
        .flag("-isysroot")
        .flag(&mer_target_root)
        .include(format!("{}/usr/include/", mer_target_root))
        .include(format!("{}/usr/include/openssl", mer_target_root))
        .file("sqlcipher/sqlite3.c")
        .warnings(false)
        .flag("-Wno-stringop-overflow")
        .flag("-Wno-return-local-addr")
        .flag("-DSQLITE_CORE")
        .flag("-DSQLITE_DEFAULT_FOREIGN_KEYS=1")
        .flag("-DSQLITE_ENABLE_API_ARMOR")
        .flag("-DSQLITE_HAS_CODEC")
        .flag("-DSQLITE_TEMP_STORE=2")
        .flag("-DHAVE_ISNAN")
        .flag("-DHAVE_LOCALTIME_R")
        .flag("-DSQLITE_ENABLE_COLUMN_METADATA")
        .flag("-DSQLITE_ENABLE_DBSTAT_VTAB")
        .flag("-DSQLITE_ENABLE_FTS3")
        .flag("-DSQLITE_ENABLE_FTS3_PARENTHESIS")
        .flag("-DSQLITE_ENABLE_FTS5")
        .flag("-DSQLITE_ENABLE_JSON1")
        .flag("-DSQLITE_ENABLE_LOAD_EXTENSION=1")
        .flag("-DSQLITE_ENABLE_MEMORY_MANAGEMENT")
        .flag("-DSQLITE_ENABLE_RTREE")
        .flag("-DSQLITE_ENABLE_STAT2")
        .flag("-DSQLITE_ENABLE_STAT4")
        .flag("-DSQLITE_SOUNDEX")
        .flag("-DSQLITE_THREADSAFE=1")
        .flag("-DSQLITE_USE_URI")
        .flag("-DHAVE_USLEEP=1")
        .compile("sqlcipher");

    println!("cargo:lib_dir={}", env::var("OUT_DIR").unwrap());
    println!("cargo:rustc-link-lib=static=sqlcipher");
    println!("cargo:rerun-if-changed={}", "sqlcipher/sqlite3.c");
}

fn main() {
    protobuf().unwrap();

    // Print a warning when rustc is too old.
    if !version_check::is_min_version("1.48.0").unwrap_or(false) {
        if let Some(version) = version_check::Version::read() {
            panic!(
                "Whisperfish requires Rust 1.48.0 or later.  You are using rustc {}",
                version
            );
        } else {
            panic!(
                "Whisperfish requires Rust 1.48.0 or later, but could not determine Rust version.",
            );
        }
    }

    let (mer_target_root, cross_compile) = install_mer_hacks();
    let qt_include_path = if cross_compile {
        format!("{}/usr/include/qt5/", mer_target_root)
    } else {
        qmake_query("QT_INSTALL_HEADERS")
    };
    let qt_include_path = qt_include_path.trim();

    let mut cfg = cpp_build::Config::new();

    cfg.flag(&format!("--sysroot={}", mer_target_root));
    cfg.flag("-isysroot");
    cfg.flag(&mer_target_root);

    // https://github.com/rust-lang/cargo/pull/8441/files
    // currently requires -Zextra-link-arg, so we're duplicating this in dotenv
    println!("cargo:rustc-link-arg=--sysroot={}", mer_target_root);

    cfg.include(format!(
        "{}/QtGui/{}",
        qt_include_path,
        detect_qt_version(std::path::Path::new(&qt_include_path)).unwrap()
    ));

    // This is kinda hacky. Sorry.
    if cross_compile {
        std::env::set_var("CARGO_FEATURE_SAILFISH", "");
    }
    cfg.include(format!("{}/usr/include/sailfishapp/", mer_target_root))
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

    let sailfish_libs: &[&str] = if true {
        // I was not able to get ld link against `nemonotificationas` in the sfdk build env
        //&["nemonotifications", "sailfishapp", "qt5embedwidget"]
        &["sailfishapp", "qt5embedwidget"]
    } else {
        &[]
    };
    let libs = ["EGL", "dbus-1"];
    for lib in libs.iter().chain(sailfish_libs.iter()) {
        println!("cargo:rustc-link-lib{}={}", macos_lib_search, lib);
    }

    prepare_rpm_build();

    if cross_compile {
        build_sqlcipher(&mer_target_root);
    }

    // vergen
    let flags = ConstantsFlags::all();
    // Generate the 'cargo:' key output
    generate_cargo_keys(flags).expect("Unable to generate the cargo keys!");
}
