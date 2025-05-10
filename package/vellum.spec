Name:           vellum
Version:        @@VERSION@@
Release:        @@RELEASE@@
Summary:        sync shell history using git

License:        MIT
URL:            https://vellum.qur.me

%description
vellum syncs shell command history between hosts using a git repository as a
central synchronisation point.

%build

%install
rm -rf $RPM_BUILD_ROOT
mkdir -p $RPM_BUILD_ROOT/usr/share/licenses/vellum
cp /package/files/LICENSE $RPM_BUILD_ROOT/usr/share/licenses/vellum/
mkdir -p $RPM_BUILD_ROOT/%{_bindir}
install /package/files/vellum $RPM_BUILD_ROOT/%{_bindir}
mkdir -p $RPM_BUILD_ROOT/usr/share/bash-completion/completions
mkdir -p $RPM_BUILD_ROOT/usr/share/man/man1
install -Dm644 /package/completion/bash "$RPM_BUILD_ROOT/usr/share/bash-completion/completions/vellum"
install -Dm644 /package/completion/zsh "$RPM_BUILD_ROOT/usr/share/zsh/site-functions/_vellum"
install -Dm644 /package/completion/fish "$RPM_BUILD_ROOT/usr/share/fish/vendor_completions.d/vellum.fish"
install -Dm644 /package/man1/* "$RPM_BUILD_ROOT/usr/share/man/man1"


%files
%license /usr/share/licenses/vellum/LICENSE
#%doc add-docs-here
%{_bindir}/*
/usr/share/bash-completion/completions/vellum
/usr/share/zsh/site-functions/_vellum
/usr/share/fish/vendor_completions.d/vellum.fish
/usr/share/man/man1/*


%post
#!/bin/sh
killall vellum


%changelog
* Sun May 4 2025 jp3
-
