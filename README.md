<div align="center">

<img width="75" height="75" alt="Icon" src="assets/icon/128.png"/>

# coppwr
Low level control GUI for [PipeWire](https://pipewire.org)
![Screenshot](assets/sc.png)

</div>

## Description
**coppwr** is a tool that provides low level control over the [PipeWire](https://pipewire.org) multimedia server.  
It aims to expose and provide as many ways to inspect and control the many aspects of the PipeWire multimedia server as possible.
It can be used as a diagnostic tool for PipeWire and to help develop software that interacts with it.
End-users of PipeWire that want to configure it should look into simpler tools
[recommended by the PipeWire devs](https://gitlab.freedesktop.org/pipewire/pipewire/-/wikis/FAQ#is-there-a-native-gui-tool-to-configure-pipewire).
If you want to learn the inner workings of PipeWire check out the [docs page on its design](https://docs.pipewire.org/page_pipewire.html)
and its [wiki](https://gitlab.freedesktop.org/pipewire/pipewire/-/wikis/home).

## Features
- Object inspection, creation & destruction
- Process monitoring & profiler statistics
- Metadata editing
- Module loading  
[More to be added...](https://github.com/dimtpap/coppwr/issues/1)

## Installing
### Flatpak
<a href='https://flathub.org/apps/xyz.dimtpap.coppwr'><img width='240' alt='Download on Flathub' src='https://dl.flathub.org/assets/badges/flathub-badge-en.png'/></a>
### Arch
[![coppwr AUR version](https://img.shields.io/aur/version/coppwr?label=coppwr&logo=archlinux)](https://aur.archlinux.org/packages/coppwr)
[![coppwr-bin AUR version](https://img.shields.io/aur/version/coppwr-bin?label=coppwr-bin&logo=archlinux)](https://aur.archlinux.org/packages/coppwr-bin)  
`coppwr-bin` is available from the [AUR](https://aur.archlinux.org/packages/coppwr-bin) (use `coppwr` for the non-prebuilt package).  
Use your AUR helper of choice or install it manually
```sh
git clone https://aur.archlinux.org/coppwr-bin.git
cd coppwr-bin
makepkg -i
```
### Debian, RPM
Debian and RPM packages are available from the [releases](https://github.com/dimtpap/coppwr/releases/latest).
### **Note**
coppwr does **not** self-update.

## Building
### Requirements
- Rust and Cargo from your distribution packages or see https://www.rust-lang.org/tools/install
- bindgen [requirements](https://rust-lang.github.io/rust-bindgen/requirements.html)
- PipeWire library headers/PipeWire development packages
### Build
In the repository's root directory
```sh
cargo build --release
```
### Arch
`coppwr` is available from the [AUR](https://aur.archlinux.org/packages/coppwr)
```sh
git clone https://aur.archlinux.org/coppwr.git
cd coppwr
makepkg
```
### Debian, RPM
Debian and RPM packages can be created using [cargo-deb](https://github.com/kornelski/cargo-deb#readme)
and [cargo-generate-rpm](https://github.com/cat-in-136/cargo-generate-rpm#cargo-generate-rpm) respectively.
See their usage instructions.

## Credits
- [egui](https://crates.io/crates/egui)+[eframe](https://crates.io/crates/eframe)
- [egui_dock](https://crates.io/crates/egui_dock)
- ([A fork](https://gitlab.freedesktop.org/dimtpap/pipewire-rs/-/tree/coppwr-next) of) [pipewire-rs](https://crates.io/crates/pipewire)
