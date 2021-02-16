%define __spec_install_post %{nil}
%define __os_install_post %{_dbpath}/brp-compress
%define debug_package %{nil}
%global rust_version 1.49.0

Name: harbour-whisperfish
Summary: Private messaging using Signal for SailfishOS.

Version: 0.6.0_dev
Release: f83e8d
License: GPLv3+
Group: Qt/Qt
URL: https://gitlab.com/rubdos/whisperfish/
Source0: %{name}-%{version}.tar.gz
Requires:   sailfishsilica-qt5 >= 0.10.9
Requires:   sailfish-components-contacts-qt5
Requires:   nemo-qml-plugin-contacts-qt5
Requires:   nemo-qml-plugin-configuration-qt5
Requires:   nemo-qml-plugin-notifications-qt5
Requires:   sailfish-components-webview-qt5
Requires:   openssl-libs
Requires:   qtmozembed-qt5
Requires:   dbus
BuildRequires:   pkgconfig(sqlcipher)
BuildRequires:   qtmozembed-qt5
BuildRequires:   qtmozembed-qt5-devel
BuildRequires:   nemo-qml-plugin-notifications-qt5
BuildRequires:   nemo-qml-plugin-notifications-qt5-devel

# This comment lists SailfishOS-version specific code,
# for future reference, to track the reasoning behind the minimum SailfishOS version.
# We're aiming to support 3.4 as long as possible, since Jolla 1 will be stuck on that.
#
# - Contacts/contacts.db phoneNumbers.normalizedNumber: introduced in 3.3
Requires:   sailfish-version >= 3.3

#BuildRoot: %{_tmppath}/%{name}-%{version}-%{release}-root

%description
%{summary}

%prep
%setup -q

%ifarch %ix86
%define rust_triple i686-unknown-linux-gnu
%else
%ifarch %{arm}
%define rust_triple armv7-unknown-linux-gnueabihf
%else
%ifarch aarch64
%define rust_triple aarch64-unknown-linux-gnu
%endif
%endif
%endif

# This is a hack, but currently the only way to use sfdk with a recent rust version.
%define rust_name rust-%{rust_version}-%{rust_triple}
%define rust_install_dir /home/mersdk/%{rust_name}
%define rust_download_dir /home/mersdk/downloads/%{rust_name}

# not sure, but I think we need to cleanup .cargo in case via switch arch
rm -rf /home/mersdk/.cargo

# download rust
mkdir -p %{rust_download_dir}
curl https://static.rust-lang.org/dist/%{rust_name}.tar.gz | \
	tar -xz --strip-components=1 -C %{rust_download_dir}

# install rust
mkdir -p %{rust_install_dir}
%{rust_download_dir}/install.sh --prefix=%{rust_install_dir}

%build
# this is needed to bypass mb2 wrappers
export QMAKE=/usr/bin/qmake

export PROTOC=/usr/bin/protoc
# PROTOC_INCLUDE=$(brew --prefix)/include
# add our rust version to path
export PATH=%{rust_install_dir}/bin:$PATH
export RUSTFLAGS="-Clink-arg=-Wl,-z,relro,-z,now -Ccodegen-units=1 -Clink-arg=-rdynamic"

# release
cargo build --release --features=sailfish --target-dir=target --manifest-path %{_sourcedir}/../Cargo.toml
# debug
#cargo build --target-dir=target --locked --manifest-path %{_sourcedir}/../Cargo.toml

# check that main symbol exists
#nm -D target/release/%{name} | grep main

for filename in %{_sourcedir}/../translations/*.ts; do
    base="${filename%.*}"
    lrelease -idbased "$base.ts" -qm "$base.qm";
done
#rm %{buildroot}%{_datadir}/%{name}/translations/*.ts

%install
rm -rf %{buildroot}
#mkdir -p %{buildroot}
#cp -a * %{buildroot}

install -d %{buildroot}%{_datadir}/%{name}

install -Dm 755 target/release/%{name} -t %{buildroot}%{_bindir}

install -Dm 644 %{_sourcedir}/../%{name}.desktop -t %{buildroot}%{_datadir}/applications
cp -r %{_sourcedir}/../qml %{buildroot}%{_datadir}/%{name}/qml

install -Dm 644 %{_sourcedir}/../%{name}.privileges -t %{buildroot}%{_datadir}/mapplauncherd/privileges.d
install -Dm 644 %{_sourcedir}/../%{name}-message.conf -t %{buildroot}%{_datadir}/lipstick/notificationcategories
cp -r %{_sourcedir}/../qml %{buildroot}%{_datadir}/%{name}/qml
cp -r %{_sourcedir}/../icons %{buildroot}%{_datadir}/%{name}/icons
cp -r %{_sourcedir}/../translations %{buildroot}%{_datadir}/%{name}/translations

install -Dm 755 %{_sourcedir}/../%{name}.service -t %{buildroot}/usr/lib/systemd/user

install -Dm 644 %{_sourcedir}/../icons/86x86/%{name}.png -t %{buildroot}%{_datadir}/icons/hicolor/86x86/apps

desktop-file-install --delete-original       \
  --dir %{buildroot}%{_datadir}/applications             \
   %{buildroot}%{_datadir}/applications/*.desktop

%clean
rm -rf %{buildroot}

#[{{ NOT HARBOUR
# This block will be removed by build.rs when building with feature "harbour" enabled.
%post
systemctl-user daemon-reload

%preun
systemctl-user disable harbour-wisperfish.service || true
# end removable block
#}}]

%files
%defattr(-,root,root,-)
%{_bindir}/*
%{_datadir}/%{name}
%{_datadir}/%{name}/qml
%{_datadir}/%{name}/icons
%{_datadir}/%{name}/translations
%{_datadir}/applications/%{name}.desktop
%{_datadir}/mapplauncherd/privileges.d/%{name}.privileges
%{_datadir}/icons/hicolor/*/apps/%{name}.png
%{_datadir}/lipstick/notificationcategories/%{name}-message.conf
#[{{ NOT HARBOUR
%{_exec_prefix}/lib/systemd/user/%{name}.service
#}}]
