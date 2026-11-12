// No console window on Windows: this is a GUI app.
#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod server;

use eframe::egui::{self, Color32, RichText};
use proto::PORT;
use server::Shared;
use std::net::UdpSocket;
use std::sync::{atomic::Ordering, Arc};
use std::time::Duration;

fn local_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| { s.connect("8.8.8.8:80")?; s.local_addr() }) // no packet is sent
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "unknown".into())
}

struct App { sh: Arc<Shared>, ip: String }

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
        let sh = self.sh.clone();
        let running = sh.running.load(Ordering::SeqCst);
        let connected = sh.connected.load(Ordering::SeqCst);
        let (dot, label) = if connected { (Color32::from_rgb(60, 200, 110), "Phone connected") }
            else if running { (Color32::from_rgb(240, 180, 50), "Waiting for phone") }
            else { (Color32::from_rgb(150, 150, 160), "Stopped") };
        let addr = format!("{}:{}", self.ip, PORT);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new("RemoteDesk").size(30.0).strong());
                ui.label(RichText::new("Use your PC from your phone").color(Color32::GRAY));
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.add_space(110.0);
                    ui.label(RichText::new("●").color(dot).size(18.0));
                    ui.label(RichText::new(label).size(16.0));
                });
                let status = sh.status.lock().unwrap().clone();
                if status.starts_with("Error") { ui.colored_label(Color32::LIGHT_RED, status); }
                ui.add_space(14.0);

                card(ui, |ui| {
                    ui.label(RichText::new("PIN (type this on your phone)").color(Color32::GRAY));
                    let pin = sh.pin.lock().unwrap().clone();
                    ui.label(RichText::new(&pin).monospace().size(44.0).strong().color(Color32::from_rgb(120, 160, 255)));
                    if ui.add_enabled(!connected, egui::Button::new("New PIN")).clicked() {
                        *sh.pin.lock().unwrap() = server::random_pin();
                    }
                });
                ui.add_space(8.0);
                card(ui, |ui| {
                    ui.label(RichText::new("PC address").color(Color32::GRAY));
                    ui.label(RichText::new(&addr).monospace().size(22.0));
                    if ui.button("Copy address").clicked() { ctx.copy_text(addr.clone()); }
                });
                ui.add_space(8.0);
                card(ui, |ui| {
                    let (mut fps, mut mbps) = (sh.fps.load(Ordering::SeqCst), sh.mbps.load(Ordering::SeqCst));
                    ui.add_enabled(!connected, egui::Slider::new(&mut fps, 15..=60).text("FPS"));
                    ui.add_enabled(!connected, egui::Slider::new(&mut mbps, 2..=40).text("Mbps"));
                    sh.fps.store(fps, Ordering::SeqCst);
                    sh.mbps.store(mbps, Ordering::SeqCst);
                    if connected {
                        ui.label(format!("Streaming from {} at {} kbps", sh.peer.lock().unwrap(), sh.kbps.load(Ordering::SeqCst)));
                    }
                });
                ui.add_space(14.0);
                let btn = if running { "Stop server" } else { "Start server" };
                if ui.add_sized([200.0, 40.0], egui::Button::new(RichText::new(btn).size(16.0))).clicked() {
                    if running { server::stop(&sh); } else { server::start(&sh); }
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Same Wi-Fi only. Allow port 47990 in Windows Firewall.").small().color(Color32::GRAY));
            });
        });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(Color32::from_rgb(34, 36, 46))
        .rounding(12.0)
        .inner_margin(14.0)
        .show(ui, |ui| { ui.set_width(340.0); ui.vertical_centered(add); });
}

fn main() -> eframe::Result<()> {
    let sh = Shared::new();
    server::start(&sh); // auto-start
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 640.0])
            .with_resizable(false),
        ..Default::default()
    };
    eframe::run_native(
        "RemoteDesk Host",
        opts,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(App { sh, ip: local_ip() }))
        }),
    )
}
