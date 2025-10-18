%bcond_with harbour
%bcond_with console_subscriber
%bcond_with tracy
%bcond_with flame
%bcond_with coz
%bcond_with lto
%bcond_with sccache
%bcond_with tools
%bcond_with calling
%bcond_with diesel_instrumentation
%bcond_with vendor
%bcond_without xz

# Chum: _chum is set globally
# OBS: _obs has to be set manually in the project config.
# Elsewhere: sfdk build --define="_obs 1"
%if 0%{?_chum} || 0%{?_obs}
# "Enforce" a few --with build conditions.
%define with_lto 1
%define with_tools 1
%define with_vendor 1
%endif

%if %{with xz}
# Targets 4.5 and newer default to Zstd RPM compression,
# which is not supported on 4.4 and older
%define _source_payload w6.xzdio
%define _binary_payload w6.xzdio
%endif

%if %{with harbour}
%define builddir target/sailfishos-harbour/%{_target_cpu}
%else
%define builddir target/sailfishos/%{_target_cpu}
%endif

Name: harbour-whisperfish
Summary: Private messaging using Signal for SailfishOS

Version: 0.6.0
Release: 1
License: AGPLv3
Group: Qt/Qt
URL: https://gitlab.com/whisperfish/whisperfish/
Source0: %{name}-%{version}.tar.gz

%if %{with vendor}
# Note: these files don't exist in the git repository
Source1: vendor.tar.xz
Source2: vendor.toml
%endif

Requires:   sailfishsilica-qt5 >= 0.10.9
Requires:   libsailfishapp-launcher
Requires:   sailfish-components-contacts-qt5
Requires:   nemo-qml-plugin-contacts-qt5
Requires:   nemo-qml-plugin-configuration-qt5
Requires:   nemo-qml-plugin-notifications-qt5
Requires:   dbus

# For recording voice notes and voice/video calling
Requires:   gstreamer1.0
# For avmux_mp4 and avmux_aac
Requires:   gstreamer1.0-libav
Requires:   opus
BuildRequires:   gstreamer1.0-devel

# For the captcha QML application
Requires:   qtmozembed-qt5
Requires:   sailfish-components-webview-qt5
Requires:   sailfish-components-webview-qt5-popups
Requires:   sailfish-components-webview-qt5-pickers

Recommends:   sailjail
Recommends:   sailjail-permissions
Recommends:   harbour-whisperfish-shareplugin

# This comment lists SailfishOS-version specific code,
# for future reference, to track the reasoning behind the minimum SailfishOS version.
# We're aiming to support 3.4 as long as possible, since Jolla 1 will be stuck on that.
#
# - Contacts/contacts.db phoneNumbers.normalizedNumber: introduced in 3.3
Requires:   sailfish-version >= 3.3

BuildRequires:  pkgconfig(sailfishapp) >= 1.0.3
BuildRequires:  pkgconfig(Qt5Core)
BuildRequires:  pkgconfig(Qt5Qml)
BuildRequires:  pkgconfig(Qt5Quick)
BuildRequires:  pkgconfig(Qt5Widgets)
BuildRequires:  libatomic-static

BuildRequires:  rust >= 1.89
BuildRequires:  rust-std-static >= 1.89
BuildRequires:  cargo >= 1.89
BuildRequires:  git
BuildRequires:  protobuf-compiler
BuildRequires:  nemo-qml-plugin-notifications-qt5-devel
BuildRequires:  qt5-qtwebsockets-devel
BuildRequires:  dbus-devel
BuildRequires:  gcc-c++
BuildRequires:  zlib-devel
BuildRequires:  coreutils
BuildRequires:  perl-IPC-Cmd

# %if %%{with calling}
# # Ringrtc needs linking against -lssl and -lcrypto;
# # currently no way to link against our vendored openssl
# BuildRequires:  openssl-libs openssl-devel
# %endif

BuildRequires:  pkgconfig(systemd)

BuildRequires:  meego-rpm-config

# For vendored sqlcipher
BuildRequires:  tcl
BuildRequires:  automake

%{!?qtc_qmake5:%define qtc_qmake5 %qmake5}
%{!?qtc_make:%define qtc_make make}

%ifarch %arm
%define targetdir target/armv7-unknown-linux-gnueabihf/release
%endif
%ifarch aarch64
%define targetdir target/aarch64-unknown-linux-gnu/release
%endif
%ifarch %ix86
%define targetdir target/i686-unknown-linux-gnu/release
%endif

%description
Whisperfish is an advanced but unofficial Signal client. Whisperfish should
be in a usable state for many users, but is still considered beta quality
software. Make sure to always have the latest version! Also, check our
wiki and feel free to contribute to the project!


# This description section includes metadata for SailfishOS:Chum, see
# https://github.com/sailfishos-chum/main/blob/main/Metadata.md
%if 0%{?_chum}
Title: Whisperfish
Type: desktop-application
DeveloperName: Ruben De Smet
Categories:
 - Network
 - InstantMessaging
Custom:
  Repo: https://gitlab.com/whisperfish/whisperfish
PackageIcon: https://gitlab.com/whisperfish/whisperfish/-/raw/main/icons/172x172/harbour-whisperfish.png
Screenshots:
 - https://gitlab.com/whisperfish/whisperfish/-/raw/main/screenshots/01-conversations.jpg
 - https://gitlab.com/whisperfish/whisperfish/-/raw/main/screenshots/02-attachments.jpg
 - https://gitlab.com/whisperfish/whisperfish/-/raw/main/screenshots/03-group-members.jpg
 - https://gitlab.com/whisperfish/whisperfish/-/raw/main/screenshots/04-cover.jpg
Links:
  Homepage: https://gitlab.com/whisperfish/whisperfish
  Help: https://gitlab.com/whisperfish/whisperfish-wiki
  Bugtracker: https://gitlab.com/whisperfish/whisperfish/-/issues
  Donation: https://liberapay.com/Whisperfish/
%endif

%prep
%setup -q -n %{name}-%{version}

%build

# export CARGO_HOME=target

rustc --version
cargo --version

%if %{with vendor}
echo "Setting up an OFFLINE vendored build."
export OFFLINE="--offline"
if [ -d "vendor" ]; then
  echo "Not overwriting existing vendored sources."
else
  tar xf %SOURCE1
  mkdir -p .cargo/
fi
cp %SOURCE2 .cargo/config.toml
%endif

export PROTOC=/usr/bin/protoc
protoc --version

%if %{with sccache}
%ifnarch %ix86
export RUSTC_WRAPPER=sccache
sccache --start-server
sccache -s
%endif
%endif

# https://git.sailfishos.org/mer-core/gecko-dev/blob/master/rpm/xulrunner-qt5.spec#L224
# When cross-compiling under SB2 rust needs to know what arch to emit
# when nothing is specified on the command line. That usually defaults
# to "whatever rust was built as" but in SB2 rust is accelerated and
# would produce x86 so this is how it knows differently. Not needed
# for native x86 builds
%ifarch %arm
export SB2_RUST_TARGET_TRIPLE=armv7-unknown-linux-gnueabihf
export CFLAGS_armv7_unknown_linux_gnueabihf=$CFLAGS
export CXXFLAGS_armv7_unknown_linux_gnueabihf=$CXXFLAGS
%endif
%ifarch aarch64
export SB2_RUST_TARGET_TRIPLE=aarch64-unknown-linux-gnu
export CFLAGS_aarch64_unknown_linux_gnu=$CFLAGS
export CXXFLAGS_aarch64_unknown_linux_gnu=$CXXFLAGS
%endif
%ifarch %ix86
export SB2_RUST_TARGET_TRIPLE=i686-unknown-linux-gnu
export CFLAGS_i686_unknown_linux_gnu=$CFLAGS
export CXXFLAGS_i686_unknown_linux_gnu=$CXXFLAGS
%endif

export CFLAGS="-O2 -g -pipe -Wall -Wp,-D_FORTIFY_SOURCE=2 -fexceptions -fstack-protector --param=ssp-buffer-size=4 -Wformat -Wformat-security -fmessage-length=0"
export CXXFLAGS=$CFLAGS
# This avoids a malloc hang in sb2 gated calls to execvp/dup2/chdir
# during fork/exec. It has no effect outside sb2 so doesn't hurt
# native builds.
# export SB2_RUST_EXECVP_SHIM="/usr/bin/env LD_PRELOAD=/usr/lib/libsb2/libsb2.so.1 /usr/bin/env"
# export SB2_RUST_USE_REAL_EXECVP=Yes
# export SB2_RUST_USE_REAL_FN=Yes
# export SB2_RUST_NO_SPAWNVP=Yes

# Set meego cross compilers
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=armv7hl-meego-linux-gnueabi-gcc
export CC_armv7_unknown_linux_gnueabihf=armv7hl-meego-linux-gnueabi-gcc
export CXX_armv7_unknown_linux_gnueabihf=armv7hl-meego-linux-gnueabi-g++
export AR_armv7_unknown_linux_gnueabihf=armv7hl-meego-linux-gnueabi-ar
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-meego-linux-gnu-gcc
export CC_aarch64_unknown_linux_gnu=aarch64-meego-linux-gnu-gcc
export CXX_aarch64_unknown_linux_gnu=aarch64-meego-linux-gnu-g++
export AR_aarch64_unknown_linux_gnu=aarch64-meego-linux-gnu-ar

# Hack for qmetaobject on QT_SELECT=5 platforms
# export QMAKE=rpm/qmake-sailfish

# qttypes tries to call qmake6 first, which results in
# an error something else than io::ErrorKind::NotFound,
# so we'll have to point it straight to qmake.
export QMAKE=/usr/bin/qmake

# Hack for cross linking against dbus
export PKG_CONFIG_ALLOW_CROSS_i686_unknown_linux_gnu=1
export PKG_CONFIG_ALLOW_CROSS_armv7_unknown_linux_gnueabihf=1
export PKG_CONFIG_ALLOW_CROSS_aarch64_unknown_linux_gnu=1

%if %{without harbour}
FEATURES=sailfish
%endif
%if %{with harbour}
FEATURES="sailfish,harbour"
%endif

%if %{with console_subscriber}
export RUSTFLAGS="%{?rustflags} --cfg tokio_unstable"
FEATURES="$FEATURES,console-subscriber"
%else
export RUSTFLAGS="%{?rustflags}"
%endif

%if %{with tracy}
FEATURES="$FEATURES,tracy"
%endif

%if %{with flame}
FEATURES="$FEATURES,flame"
%endif

%if %{with coz}
FEATURES="$FEATURES,coz"
%endif

%if %{with diesel_instrumentation}
FEATURES="$FEATURES,diesel-instrumentation"
%endif

%if %{with calling}
FEATURES="$FEATURES,calling"
# ringrtc requires an output directory for the WebRTC artifacts
export OUTPUT_DIR=`realpath .`/ringrtc/322/${SB2_RUST_TARGET_TRIPLE}
%endif

# We could use the %%(version) and %%(release), but SFDK will include a datetime stamp,
# ordering Cargo to recompile literally every second when the workspace is dirty.
# git describe is a lot stabler, because it only uses the commit number and potentially a -dirty flag
export GIT_VERSION=$(git describe  --exclude release,tag --dirty=-dirty)

# Configure Cargo.toml
# https://blog.rust-lang.org/2022/09/22/Rust-1.64.0.html#cargo-improvements-workspace-inheritance-and-multi-target-builds
%if 0%{?cargo_version:1}
for TOML in $(ls Cargo.toml */Cargo.toml) ; do
  sed -i.bak "s/^version\s*=\s*\"[-\.0-9a-zA-Z]*\"$/version = \"%{cargo_version}\"/" "$TOML"
done
export CARGO_PROFILE_RELEASE_LTO=thin
%endif
cat Cargo.toml

%if %{with lto}
export CARGO_PROFILE_RELEASE_LTO=thin
%endif

%if %{with tools}
BINS="--bins"
%else
BINS="--bin harbour-whisperfish"
%endif

# Workaround a Scratchbox bug - /tmp/[...]/symbols.o not found
export TMPDIR=${TMPDIR:-$(realpath ".tmp")}
mkdir -p $TMPDIR

%if 0%{?taskset:1}
export TASKSET="taskset %{taskset}"
%else
export JOBS="-j 1"
%endif

$TASKSET cargo build $JOBS \
          -v \
          --release \
          --no-default-features \
          $BINS \
          --features $FEATURES \
          $OFFLINE \
          %nil

%if %{with sccache}
sccache -s
%endif

lrelease -idbased translations/*.ts

%install

install -d %{buildroot}%{_datadir}/harbour-whisperfish/translations
install -Dm 644 translations/*.qm \
        %{buildroot}%{_datadir}/harbour-whisperfish/translations

install -D %{targetdir}/harbour-whisperfish %{buildroot}%{_bindir}/harbour-whisperfish
%if %{without harbour}
%if %{with tools}
install -D %{targetdir}/storage_key %{buildroot}%{_bindir}/whisperfish-storage-key
install -D %{targetdir}/whisperfish-migration-dry-run %{buildroot}%{_bindir}/whisperfish-migration-dry-run
%endif
%endif

desktop-file-install \
  --dir %{buildroot}%{_datadir}/applications \
   harbour-whisperfish.desktop

install -Dm 644 harbour-whisperfish.profile \
    %{buildroot}%{_sysconfdir}/sailjail/permissions/harbour-whisperfish.profile
install -Dm 644 harbour-whisperfish.privileges \
    %{buildroot}%{_datadir}/mapplauncherd/privileges.d/harbour-whisperfish.privileges
install -Dm 644 harbour-whisperfish-message.conf \
    %{buildroot}%{_datadir}/lipstick/notificationcategories/harbour-whisperfish-message.conf
install -Dm 644 harbour-whisperfish-call.conf \
    %{buildroot}%{_datadir}/lipstick/notificationcategories/harbour-whisperfish-call.conf

# Application icons
for RES in 86x86 108x108 128x128 172x172; do
    install -Dm 644 \
        icons/${RES}/harbour-whisperfish.png \
        %{buildroot}%{_datadir}/icons/hicolor/${RES}/apps/harbour-whisperfish.png
done

# In-application icons
find ./icons -maxdepth 1 -type f -exec \
    install -Dm 644 "{}" "%{buildroot}%{_datadir}/harbour-whisperfish/{}" \;

# QML files
find ./qml -type f -exec \
    install -Dm 644 "{}" "%{buildroot}%{_datadir}/harbour-whisperfish/{}" \;

%if %{without harbour}
# Dbus service
install -Dm 644 be.rubdos.whisperfish.service \
    %{buildroot}%{_unitdir}/be.rubdos.whisperfish.service
install -Dm 644 harbour-whisperfish.service \
    %{buildroot}%{_userunitdir}/harbour-whisperfish.service
%endif

%if %{without harbour}
%post
systemctl-user daemon-reload
if pidof harbour-whisperfish >/dev/null; then
  kill -INT $(pidof harbour-whisperfish) || true
fi
%endif

%if %{without harbour}
%preun
systemctl-user stop harbour-whisperfish.service || true
systemctl-user disable harbour-whisperfish.service || true
%endif

%files
%{_bindir}/*
%{_datadir}/%{name}
%{_datadir}/applications/%{name}.desktop
%{_datadir}/mapplauncherd/privileges.d/%{name}.privileges
%{_datadir}/icons/hicolor/*/apps/%{name}.png
%{_datadir}/lipstick/notificationcategories/%{name}*.conf

%config %{_sysconfdir}/sailjail/permissions/harbour-whisperfish.profile

%if %{without harbour}
%{_userunitdir}/harbour-whisperfish.service
%{_unitdir}/be.rubdos.whisperfish.service
%endif

%changelog
* Sun Nov 17 2024 Ruben De Smet <ruben.de.smet@rubdos.be> 0.6.0-beta.32
- Allow retrying attachment downloads from the UI
- Try to reduce notification flooding on startup
- Update emoji support to Emoji 15.1
- Don’t reset the text field on incoming messages
- Improve logging

* Mon Oct 28 2024 Ruben De Smet <ruben.de.smet@rubdos.be> 0.6.0-beta.31
- Fix text field not showing up.
- Send note-to-self messages as sync messages to show up on the “right side” of sync conversations

* Mon Oct 28 2024 Ruben De Smet <ruben.de.smet@rubdos.be> 0.6.0-beta.30
- Fix linking and initial link synchronisation (implement master key and other sync messages)
- Fix wrong indication/disambiguation in UI between session resets and identity resets
- Some initial patches to get WF to compile on OBS (some day, Chum!)
- More compact logs
- Expiry timer versions (disable expiry timer changes in groups for now)
- Read receipts
- Cleaner migration paths when rsync-ing Whisperfish data directories from nemo to defaultuser phones
- Rewrite Qt model logic to allow asynchronous model updates
- Fix a lot of UI glitches; a.o., unread count on cover
- Introduce a whole lot of new UI glitches; please report them!
- Initial voice/video call boiler plate
- “Missed voice call” / “Missed video call” notifications for direct calls (no group calls)
- Incoming message requests (no group chats)
- Fix sending attachments, including to Apple users (implement attachment V4 protocol)

* Thu Sep 19 2024 Ruben De Smet <ruben.de.smet@rubdos.be> 0.6.0-beta.29
- Implements PNI endpoint receiving, PNI-sent endpoint receiving
- Performance improvement for blurhash rendering
- Empty GV2 update message fixes
- libsignal bump
- Use Speech Note automatic model instead of English
