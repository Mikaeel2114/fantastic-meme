# RemoteDesk (Rust host + Android client)

## Windows host
1. Install Rust (rustup) + Visual Studio Build Tools (C++), and NASM (for openh264 build).
2. `cargo run --release -p host`
3. A window opens with your PIN + PC address (no console). Allow port 47990 (TCP) in Windows Firewall.

## Phone
Open `android/` in Android Studio, run on your phone (same Wi-Fi). Enter PC IP + PIN.
Touch = mouse, tap-drag = drag, two-finger drag = scroll.

## v1 limits / roadmap (boost)
- Software H.264 -> next: NVENC/AMF/QSV hardware encode (huge latency win)
- TCP -> UDP + FEC, then QUIC for over-internet use
- No encryption yet: LAN only! Add TLS (rustls) + paired-device keys
- Keyboard UI, right-click (long press), audio (WASAPI + Opus), gamepad (ViGEm)
- Windows tray UI, auto-start service, mDNS discovery
