%define __spec_install_post %{nil}
%define __os_install_post %{_dbpath}/brp-compress
%define debug_package %{nil}

Name: harbour-whisperfish
Summary: Private messaging using Signal for SailfishOS.

Version: @@VERSION@@
Release: @@RELEASE@@
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
Requires:   dbus

# This comment lists SailfishOS-version specific code,
# for future reference, to track the reasoning behind the minimum SailfishOS version.
# We're aiming to support 3.4 as long as possible, since Jolla 1 will be stuck on that.
#
# - Contacts/contacts.db phoneNumbers.normalizedNumber: introduced in 3.3
Requires:   sailfish-version >= 3.3

BuildRoot: %{_tmppath}/%{name}-%{version}-%{release}-root

%description
%{summary}

%prep
%setup -q

%build
for filename in .%{_datadir}/%{name}/translations/*.ts; do
    base="${filename%.*}"
    lrelease -idbased "$base.ts" -qm "$base.qm";
done
rm .%{_datadir}/%{name}/translations/*.ts

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}
cp -a * %{buildroot}

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
systemctl-user disable harbour-whisperfish.service || true
# end removable block
#}}]

%files
%defattr(-,root,root,-)
%{_bindir}/*
%{_datadir}/%{name}
%{_datadir}/applications/%{name}.desktop
%{_datadir}/icons/hicolor/*/apps/%{name}.png
%{_datadir}/lipstick/notificationcategories/%{name}-message.conf
#[{{ NOT HARBOUR
%{_exec_prefix}/lib/systemd/user/%{name}.service
#}}]
