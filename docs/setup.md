# Platform Setup

Installation instructions for screenshot-mcp.

## Prerequisites

- **Rust:** Install via [rustup.rs](https://rustup.rs/)
- **Git:** For cloning the repository

---

## Linux

### Dependencies by Distribution

#### Ubuntu / Debian (22.04+)

```bash
# Build dependencies
sudo apt-get update && sudo apt-get install -y \
  build-essential pkg-config libssl-dev \
  libwayland-dev libportal-dev libsecret-1-dev \
  libx11-dev libxcb1-dev libxcb-shape0-dev libxcb-xfixes0-dev

# Runtime dependencies
sudo apt-get install -y \
  xdg-desktop-portal xdg-desktop-portal-gtk pipewire
```

> **KDE users:** Replace `-gtk` with `-kde`

#### Fedora (39+)

```bash
sudo dnf install -y \
  gcc make pkg-config openssl-devel \
  wayland-devel libportal-devel libsecret-devel \
  libX11-devel libxcb-devel \
  xdg-desktop-portal xdg-desktop-portal-gtk pipewire
```

#### Arch Linux

```bash
sudo pacman -S \
  base-devel wayland libportal libsecret libx11 libxcb \
  xdg-desktop-portal xdg-desktop-portal-gtk pipewire
```

### Build

```bash
git clone https://github.com/username/screenshot-mcp.git
cd screenshot-mcp
cargo build --release
```

### Verify

```bash
# Check portal is running (Wayland)
systemctl --user status xdg-desktop-portal

# Check DISPLAY is set (X11)
echo $DISPLAY  # Should show :0 or similar
```

---

## Windows

### Requirements

- Windows 10 version 1803+ or Windows 11
- Visual Studio C++ Build Tools

### Install Build Tools

1. Download [Visual Studio Build Tools 2022](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
2. Run installer → Select **"Desktop development with C++"**
3. Ensure these are checked:
   - MSVC v143 build tools
   - Windows 10/11 SDK

### Install Rust

1. Download `rustup-init.exe` from [rustup.rs](https://rustup.rs/)
2. Run with default options (MSVC toolchain)
3. Restart terminal

```powershell
rustc --version
cargo --version
```

### Build

```powershell
git clone https://github.com/username/screenshot-mcp.git
cd screenshot-mcp
cargo build --release
```

Binary: `target\release\screenshot-mcp.exe`

---

## Claude Desktop Configuration

### Linux

Edit `~/.config/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "screenshot": {
      "command": "/home/user/screenshot-mcp/target/release/screenshot-mcp",
      "env": { "RUST_LOG": "screenshot_mcp=info" }
    }
  }
}
```

### Windows

Edit `%APPDATA%\Claude\claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "screenshot": {
      "command": "C:\\path\\to\\screenshot-mcp\\target\\release\\screenshot-mcp.exe",
      "env": { "RUST_LOG": "info" }
    }
  }
}
```

Restart Claude Desktop after saving.
