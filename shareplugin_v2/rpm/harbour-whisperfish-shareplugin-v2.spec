%define _source_payload w6.xzdio
%define _binary_payload w6.xzdio

Name: harbour-whisperfish-shareplugin-v2
Summary: Shareplugin v2 for Whisperfish

Version: 1.1.0
Release: 0%{?dist}
License: GPLv3+
Group: Qt/Qt
URL: https://gitlab.com/whisperfish/whisperfish/
Source0: %{name}-%{version}.tar.gz
Requires:  harbour-whisperfish
Requires:  nemo-transferengine-qt5
Requires:  declarative-transferengine-qt5 >= 0.0.44
Requires:  qt5-qtdeclarative-import-sensors >= 5.2

Requires:   sailfish-version >= 4.3

Requires:   harbour-whisperfish
Provides:   harbour-whisperfish-shareplugin

BuildRequires: pkgconfig(Qt5Core)
BuildRequires: pkgconfig(Qt5Qml)
BuildRequires: pkgconfig(nemotransferengine-qt5)
BuildRequires: qt5-qttools
BuildRequires: qt5-qttools-linguist

%{!?qtc_qmake5:%define qtc_qmake5 %qmake5}
%{!?qtc_make:%define qtc_make make}

%description
%{summary}

%prep
%setup -q -n %{name}-%{version}

%build

rm -rf %{_srcdir}/*.o
rm -rf %{_srcdir}/moc_*

%qmake5
make %{?_smp_mflags}

%install

install -Dm 644 %{_sourcedir}/../WhisperfishShare.qml \
    %{buildroot}%{_datadir}/nemo-transferengine/plugins/sharing/WhisperfishShare.qml
install -Dm 755 %{_sourcedir}/../libwhisperfishshareplugin.so \
    %{buildroot}%{_libdir}/nemo-transferengine/plugins/sharing/libwhisperfishshareplugin.so

%clean
rm -rf %{buildroot}
rm -rf %{_srcdir}/*.o
rm -rf %{_srcdir}/moc_*

%files
%{_datadir}/nemo-transferengine/plugins/sharing/WhisperfishShare.qml
%{_libdir}/nemo-transferengine/plugins/sharing/libwhisperfishshareplugin.so
