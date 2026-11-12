//! Streaming server: runs on a background thread, controlled by the GUI via `Shared`.
use anyhow::Result;
use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use openh264::encoder::{Encoder, EncoderConfig};
use openh264::formats::{RgbaSliceU8, YUVBuffer};
use proto::{Packet, PORT};
use scrap::{Capturer, Display};
use std::io::{BufWriter, ErrorKind, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub struct Shared {
    pub pin: Mutex<String>,
    pub status: Mutex<String>,
    pub peer: Mutex<String>,
    pub running: AtomicBool,
    pub connected: AtomicBool,
    pub stop: AtomicBool,
    pub fps: AtomicU32,
    pub mbps: AtomicU32,
    pub kbps: AtomicU32,
}

pub fn random_pin() -> String {
    format!("{:06}", rand::random::<u32>() % 1_000_000)
}

impl Shared {
    pub fn new() -> Arc<Self> {
        Arc::new(Shared {
            pin: Mutex::new(random_pin()),
            status: Mutex::new("Stopped".into()),
            peer: Mutex::new(String::new()),
            running: AtomicBool::new(false),
            connected: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            fps: AtomicU32::new(30),
            mbps: AtomicU32::new(8),
            kbps: AtomicU32::new(0),
        })
    }
    fn say(&self, s: &str) { *self.status.lock().unwrap() = s.to_string(); }
}

pub fn start(sh: &Arc<Shared>) {
    if sh.running.swap(true, Ordering::SeqCst) { return; }
    sh.stop.store(false, Ordering::SeqCst);
    let sh = sh.clone();
    thread::spawn(move || {
        if let Err(e) = run(&sh) { sh.say(&format!("Error: {e}")); } else { sh.say("Stopped"); }
        sh.connected.store(false, Ordering::SeqCst);
        sh.running.store(false, Ordering::SeqCst);
    });
}

pub fn stop(sh: &Arc<Shared>) { sh.stop.store(true, Ordering::SeqCst); }

fn run(sh: &Arc<Shared>) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", PORT))?;
    listener.set_nonblocking(true)?;
    sh.say("Waiting for phone...");
    while !sh.stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((s, addr)) => {
                *sh.peer.lock().unwrap() = addr.ip().to_string();
                let r = session(sh, s);
                sh.connected.store(false, Ordering::SeqCst);
                sh.kbps.store(0, Ordering::SeqCst);
                match r {
                    Ok(()) => sh.say("Waiting for phone..."),
                    Err(e) => sh.say(&format!("Waiting for phone... (last: {e})")),
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

fn session(sh: &Arc<Shared>, stream: TcpStream) -> Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    let r = stream_loop(sh, &stream);
    let _ = stream.shutdown(Shutdown::Both);
    r
}

fn stream_loop(sh: &Arc<Shared>, stream: &TcpStream) -> Result<()> {
    let mut rd = stream.try_clone()?;
    rd.set_read_timeout(Some(Duration::from_secs(10)))?;
    let pin = sh.pin.lock().unwrap().clone();
    match Packet::read(&mut rd)? {
        Packet::Hello { pin: p } if p == pin => {}
        _ => {
            Packet::Welcome { ok: false, width: 0, height: 0 }.write(&mut &*stream)?;
            thread::sleep(Duration::from_secs(1)); // slow down PIN guessing
            anyhow::bail!("wrong PIN");
        }
    }
    rd.set_read_timeout(None)?;

    let display = Display::primary()?;
    let (w, h) = (display.width(), display.height());
    let mut out = BufWriter::new(stream.try_clone()?);
    Packet::Welcome { ok: true, width: w as u16, height: h as u16 }.write(&mut out)?;
    out.flush()?;
    sh.connected.store(true, Ordering::SeqCst);
    sh.say("Phone connected");

    let fps = sh.fps.load(Ordering::SeqCst).clamp(5, 120) as u64;
    let bitrate = sh.mbps.load(Ordering::SeqCst).clamp(1, 100) * 1_000_000;

    let want_key = Arc::new(AtomicBool::new(true));
    let alive = Arc::new(AtomicBool::new(true));
    {
        let (want_key, alive) = (want_key.clone(), alive.clone());
        thread::spawn(move || {
            let mut en = Enigo::new(&Settings::default()).expect("enigo");
            loop {
                match Packet::read(&mut rd) {
                    Ok(p) => handle_input(&mut en, p, w as i32, h as i32, &want_key),
                    Err(_) => { alive.store(false, Ordering::SeqCst); break; }
                }
            }
        });
    }

    let mut cap = Capturer::new(display)?;
    let mut enc = Encoder::with_api_config(
        openh264::OpenH264API::from_source(),
        EncoderConfig::new()
            .set_bitrate_bps(bitrate)
            .max_frame_rate(fps as f32)
            .enable_skip_frame(true),
    )?;
    let start = Instant::now();
    let frame_time = Duration::from_millis(1000 / fps);
    let mut rgba = vec![0u8; w * h * 4];
    let (mut bytes, mut tick) = (0usize, Instant::now());

    while alive.load(Ordering::SeqCst) && !sh.stop.load(Ordering::SeqCst) {
        let t0 = Instant::now();
        match cap.frame() {
            Ok(f) => {
                let stride = f.len() / h;
                for y in 0..h {
                    for x in 0..w {
                        let s = y * stride + x * 4;
                        let d = (y * w + x) * 4;
                        rgba[d] = f[s + 2]; rgba[d + 1] = f[s + 1];
                        rgba[d + 2] = f[s]; rgba[d + 3] = 255;
                    }
                }
                if want_key.swap(false, Ordering::SeqCst) { enc.force_intra_frame(); }
                let yuv = YUVBuffer::from_rgb_source(RgbaSliceU8::new(&rgba, (w, h)));
                let bs = enc.encode(&yuv)?;
                let data = bs.to_vec();
                if !data.is_empty() {
                    let key = matches!(bs.frame_type(), openh264::encoder::FrameType::IDR | openh264::encoder::FrameType::I);
                    bytes += data.len();
                    Packet::Video { ts_ms: start.elapsed().as_millis() as u64, keyframe: key, data }.write(&mut out)?;
                    out.flush()?;
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }
        if tick.elapsed() >= Duration::from_secs(1) {
            sh.kbps.store((bytes as f32 * 8.0 / 1000.0 / tick.elapsed().as_secs_f32()) as u32, Ordering::SeqCst);
            bytes = 0; tick = Instant::now();
        }
        if let Some(rest) = frame_time.checked_sub(t0.elapsed()) { thread::sleep(rest); }
    }
    Ok(())
}

fn handle_input(en: &mut Enigo, p: Packet, w: i32, h: i32, want_key: &AtomicBool) {
    match p {
        Packet::MouseMove { x, y } => {
            let _ = en.move_mouse((x.clamp(0.0, 1.0) * w as f32) as i32,
                                  (y.clamp(0.0, 1.0) * h as f32) as i32, Coordinate::Abs);
        }
        Packet::MouseButton { button, down } => {
            let b = match button { 1 => Button::Right, 2 => Button::Middle, _ => Button::Left };
            let _ = en.button(b, if down { Direction::Press } else { Direction::Release });
        }
        Packet::Scroll { dy } => { let _ = en.scroll(-(dy as i32), Axis::Vertical); }
        Packet::Key { vk, down } => {
            let _ = en.key(Key::Other(vk as u32), if down { Direction::Press } else { Direction::Release });
        }
        Packet::RequestKeyframe => want_key.store(true, Ordering::SeqCst),
        _ => {}
    }
}
