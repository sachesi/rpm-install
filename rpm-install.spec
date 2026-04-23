%define _debugsource_template %{nil}
%define debug_package %{nil}

%global app_id com.github.sachesi.rpminstall

Name:           rpm-install
Version:        0.3.7
Release:        1%{?dist}
Summary:        GTK4/libadwaita GUI installer for local RPM files via dnf5daemon
License:        GPL-3.0-or-later
URL:            https://github.com/sachesi/rpm-install
Source0:        %{url}/archive/refs/tags/v%{version}.tar.gz#/%{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.zst

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  pkgconfig(gtk4)
BuildRequires:  pkgconfig(libadwaita-1)
BuildRequires:  desktop-file-utils

Requires:       dnf5daemon-server

%description
GTK4/libadwaita GUI installer for local RPM files via dnf5daemon.

The application accepts local .rpm files (paths or file:// URIs), reads package
metadata, and installs or reinstalls packages using dnf5daemon transactions.
It integrates with desktop MIME handling so RPM files can be opened directly in
the GUI.

%prep
%autosetup -n %{name}-%{version}
tar -xaf %{SOURCE1}

mkdir -p .cargo
cat > .cargo/config.toml <<'EOF'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
EOF

%build
export CARGO_HOME=$PWD/.cargo-home
cargo build --release --frozen --offline

%install
install -Dm755 target/release/rpm-install \
    %{buildroot}%{_bindir}/rpm-install

install -Dm644 assets/%{app_id}.desktop \
    %{buildroot}%{_datadir}/applications/%{app_id}.desktop

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/%{app_id}.desktop

%files
%license LICENSE*
%doc README.md
%{_bindir}/rpm-install
%{_datadir}/applications/%{app_id}.desktop

%changelog
* Sat Apr 18 2026 rpm-install packager <packager@example.com> - 0.3.7-1
- Change license to GPL-3.0
- Add actual LICENSE file
- Bump version to 0.3.7

* Sat Apr 18 2026 rpm-install packager <packager@example.com> - 0.3.6-1
- Drop appstream metainfo
- Add Settings to .desktop category

* Sat Apr 18 2026 rpm-install packager <packager@example.com> - 0.3.5-2
- Use vendored offline COPR build
- Add .copr/Makefile

* Sat Apr 18 2026 rpm-install packager <packager@example.com> - 0.3.5-1
- Use system themed RPM icon (application-x-rpm) for desktop integration
- Refresh AppStream/README release metadata for 0.3.5

* Mon Apr 06 2026 rpm-install packager <packager@example.com> - 0.3.4-1
- Rename project from rpm-installer to rpm-install
- Rename compiled binary to rpm-install

* Mon Apr 06 2026 rpm-install packager <packager@example.com> - 0.1.2-1
- Add colored install-state heading cues and uninstall action for installed packages
- Keep metadata/path validation and error handling hardening from stabilization pass

* Mon Apr 06 2026 rpm-install packager <packager@example.com> - 0.1.0-1
- Initial Fedora package with COPR-friendly Source0 mode
- Keep local build-in-place workflow via --define '_build_in_place 1'
