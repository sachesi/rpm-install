%define _debugsource_template %{nil}
%define debug_package %{nil}

%global app_id com.example.RpmInstallerGui

Name:           rpm-installer-gui
Version:        0.1.1
Release:        1%{?dist}
Summary:        GTK4/libadwaita GUI installer for local RPM files via dnf5daemon
License:        MIT
URL:            https://example.com/rpm-installer-gui
%if ! 0%{?_build_in_place}
Source0:        %{url}/archive/refs/tags/%{version}/%{name}-%{version}.tar.gz
%endif

BuildRequires:  cargo
BuildRequires:  cargo-rpm-macros
BuildRequires:  rust >= 1.74
BuildRequires:  pkgconfig(gtk4)
BuildRequires:  pkgconfig(libadwaita-1)
BuildRequires:  desktop-file-utils
BuildRequires:  appstream

Requires:       dnf5daemon-server

%description
%{summary}.

The application accepts local .rpm files (paths or file:// URIs), reads package
metadata, and installs or reinstalls packages using dnf5daemon transactions.
It integrates with desktop MIME handling so RPM files can be opened directly in
the GUI.

%prep
%if 0%{?_build_in_place}
# Build directly from the current checkout when rpmbuild is called with:
#   --define '_build_in_place 1'
%else
%autosetup -n %{name}-%{version}
%endif

%generate_buildrequires
%if ! 0%{?_build_in_place}
%cargo_generate_buildrequires
%endif

%build
%if 0%{?_build_in_place}
cargo build --release
%else
%cargo_build --release
%endif

%install
%if 0%{?_build_in_place}
install -Dm755 target/release/%{name} %{buildroot}%{_bindir}/%{name}
%else
%cargo_install
%endif

install -Dm644 packaging/%{app_id}.desktop \
    %{buildroot}%{_datadir}/applications/%{app_id}.desktop
install -Dm644 packaging/%{app_id}.metainfo.xml \
    %{buildroot}%{_datadir}/metainfo/%{app_id}.metainfo.xml

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/%{app_id}.desktop
if command -v appstream-util >/dev/null 2>&1; then
    appstream-util validate-relax --nonet \
        %{buildroot}%{_datadir}/metainfo/%{app_id}.metainfo.xml
elif command -v appstreamcli >/dev/null 2>&1; then
    appstreamcli validate --no-net \
        %{buildroot}%{_datadir}/metainfo/%{app_id}.metainfo.xml
else
    echo "Neither appstream-util nor appstreamcli is available" >&2
    exit 1
fi

%post
update-desktop-database %{_datadir}/applications &> /dev/null || :

%postun
update-desktop-database %{_datadir}/applications &> /dev/null || :

%files
%doc README.md
%{_bindir}/%{name}
%{_datadir}/applications/%{app_id}.desktop
%{_datadir}/metainfo/%{app_id}.metainfo.xml

%changelog
* Mon Apr 06 2026 rpm-installer-gui packager <packager@example.com> - 0.1.1-1
- Stabilization release: improved state handling, error messages, and metadata safety bounds
- Align app metadata/docs/spec text with dnf5daemon backend

* Mon Apr 06 2026 rpm-installer-gui packager <packager@example.com> - 0.1.0-1
- Initial Fedora package with COPR-friendly Source0 mode
- Keep local build-in-place workflow via --define '_build_in_place 1'
