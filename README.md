# NetDia

<p align="center">
  <img src="resources/icon/nd-icon.png" alt="NetDia Logo" width="128" height="128" />
</p>

Cross-platform network diagnostic suite built with **Rust** + **Tauri**.  
Inspect, monitor, and analyze your network.

![GitHub release (latest SemVer)](https://img.shields.io/github/v/release/shellrow/netdia)
![License](https://img.shields.io/github/license/shellrow/netdia)
![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)

---

## Features

- **Interface Overview** - Active interfaces with IPs, gateways, and real-time stats  
- **Live Traffic Charts** - RX/TX throughput and AVG/MAX
- **Neighbor Scan** - Discover devices in your local network
- **Net Route** - Inspect routing tables, gateways, and metrics  
- **Socket Connection** - View active TCP/UDP sockets with process information  
- **Public IP Info** - Detect IPv4 / IPv6, ASN, and country  
- **Ping (ICMP / TCP / UDP / QUIC)** - Measure latency and reachability across protocols  
- **Traceroute (ICMP / UDP)** - Per-hop RTT with detailed hop visualization
- **Port Scan** - Detect open ports and services (Common, Top1000, or custom sets)  
- **Host Scan** - Scan your network to identify alive hosts  
- **Cross-Platform** - macOS, Windows, and Linux supported  

---

## 🚀 Getting Started

### Using Installer
Download the installer for your platform from the [releases page](https://github.com/shellrow/netdia/releases).

#### macOS Security
After installing NetDia on macOS, you may encounter a security warning that prevents the app from opening, stating that it is from an unidentified developer. This is a common macOS security measure for apps downloaded outside of the App Store.

To resolve this issue and open NetDia, you can remove the security attributes that macOS assigns to the application using the following command in the Terminal:

```bash
xattr -rc "/Applications/NetDia.app"
```

### 🦀 Build from source
```bash
# 1. Clone the repository
git clone https://github.com/shellrow/netdia.git
cd netdia

# 2. Install dependencies
cargo install tauri-cli
npm install

# 3. Run in development mode
cargo tauri dev

# 4. Build release package
cargo tauri build
```

## Screenshots
![NetDia Dashboard](resources/ss/nd-dashboard.png)
![NetDia Monitor](resources/ss/nd-monitor.png)
![NetDia Traceroute](resources/ss/nd-traceroute.png)
![NetDia Neighbor](resources/ss/nd-neighbor.png)
