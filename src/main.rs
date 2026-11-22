//! ascii-aquarium: a procedural 3D terminal aquarium with a single betta
//! fish. The scene is sampled from parametric 3D geometry each frame and
//! rendered as colored ASCII glyphs (with a per-cell depth buffer) on a
//! black backdrop — no video file, no sprites, no pixel framebuffer.
//!
//! Controls: q / Esc / Ctrl-C to quit. The tank resizes with the terminal.

mod camera;
mod cells;
mod fish;
mod math;
mod scene;

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{cursor, execute};

use camera::Camera;
use cells::GlyphBuffer;
use fish::Betta;
use scene::Scene;

struct Config {
    fps: u32,
    snapshot: Option<PathBuf>,
    snap_width: usize,  // character cells
    snap_height: usize, // character cells
    snap_time: f32,
    /// Pin the fish at a fixed heading (degrees) for snapshots.
    pose: Option<f32>,
}

fn parse_args() -> Result<Config, String> {
    let mut cfg = Config {
        fps: 30,
        snapshot: None,
        snap_width: 120,
        snap_height: 36,
        snap_time: 4.0,
        pose: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "--fps" => {
                let v = args.next().ok_or("--fps needs a value")?;
                cfg.fps = v.parse().map_err(|_| "invalid --fps value")?;
                if cfg.fps == 0 || cfg.fps > 120 {
                    return Err("--fps must be in 1..=120".into());
                }
            }
            "--snapshot" => {
                let v = args.next().ok_or("--snapshot needs a path")?;
                cfg.snapshot = Some(PathBuf::from(v));
            }
            "--width" => {
                let v = args.next().ok_or("--width needs a value")?;
                cfg.snap_width = v.parse().map_err(|_| "invalid --width value")?;
            }
            "--height" => {
                let v = args.next().ok_or("--height needs a value")?;
                cfg.snap_height = v.parse().map_err(|_| "invalid --height value")?;
            }
            "--time" => {
                let v = args.next().ok_or("--time needs a value")?;
                cfg.snap_time = v.parse().map_err(|_| "invalid --time value")?;
            }
            "--pose" => {
                let v = args.next().ok_or("--pose needs a value")?;
                let deg: f32 = v.parse().map_err(|_| "invalid --pose value")?;
                cfg.pose = Some(deg.to_radians());
            }
            other => return Err(format!("unknown argument: {other} (try --help)")),
        }
    }
    Ok(cfg)
}

fn print_help() {
    println!(
        "ascii-aquarium — a procedural 3D ASCII terminal aquarium with one betta fish

USAGE:
    ascii-aquarium [OPTIONS]

OPTIONS:
    --fps <N>          Target frames per second, 1..=120 (default: 30)
    --snapshot <FILE>  Render a single deterministic frame and exit.
                       '.txt' writes plain glyphs, anything else writes
                       ANSI true-color text (view with `less -R`, or raster
                       it with tools/ansi_to_png.py)
    --width <CELLS>    Snapshot width in terminal cells (default: 120)
    --height <CELLS>   Snapshot height in terminal cells (default: 36)
    --time <SECONDS>   Scene time for the snapshot (default: 4.0)
    --pose <DEG>       Pin the fish mid-tank at this heading in degrees
                       (0 = broadside facing +x, 90 = facing the camera);
                       fins keep undulating (snapshot only)
    -h, --help         Show this help

CONTROLS:
    q, Esc, Ctrl-C     Quit (terminal state is fully restored)

The whole scene is computed from 3D geometry each frame: a betta fish
(body, eyes, tail fan, dorsal/anal fins, ventral filaments, pectorals)
swims through the tank with real yaw/pitch/roll turns, projected
perspectively into a depth-buffered grid of colored ASCII glyphs. Sparse
bubbles, plant strands, floor pebbles and drifting motes share the same
character-art style. No video file or sprite art is used."
    );
}

/// Render one frame into `buf` at scene time `t`.
fn render_frame(buf: &mut GlyphBuffer, scene: &Scene, fish: &Betta, cam: &Camera, t: f32) {
    buf.clear();
    scene.paint(buf, cam, t);
    fish.paint(buf, cam);
}

fn run_snapshot(cfg: &Config) -> io::Result<()> {
    let path = cfg
        .snapshot
        .as_ref()
        .expect("snapshot path checked by caller");
    let mut buf = GlyphBuffer::new(cfg.snap_width, cfg.snap_height);
    let cam = Camera::new(cfg.snap_width, cfg.snap_height);
    let mut scene = Scene::new();
    let mut fish = Betta::new();

    // Step the simulation deterministically up to the requested scene time.
    let dt = 1.0 / 60.0;
    let steps = (cfg.snap_time / dt) as u32;
    for _ in 0..steps {
        scene.update(dt);
        fish.update(dt);
        if let Some(yaw) = cfg.pose {
            fish.pin(yaw);
        }
    }
    render_frame(&mut buf, &scene, &fish, &cam, cfg.snap_time);

    let plain = path.extension().is_some_and(|e| e == "txt");
    if plain {
        buf.write_text(path)?;
    } else {
        buf.write_ansi(path)?;
    }
    eprintln!(
        "wrote {} ({}x{} cells, t={}s, {} glyphs)",
        path.display(),
        buf.width(),
        buf.height(),
        cfg.snap_time,
        buf.nonblank()
    );
    Ok(())
}

fn main() -> io::Result<()> {
    let cfg = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };

    if cfg.snapshot.is_some() {
        return run_snapshot(&cfg);
    }

    run_interactive(&cfg)
}

fn run_interactive(cfg: &Config) -> io::Result<()> {
    // Restore the terminal even on panic.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;

    let result = event_loop(&mut stdout, cfg);

    restore_terminal();
    result
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), cursor::Show, LeaveAlternateScreen);
}

fn event_loop(stdout: &mut io::Stdout, cfg: &Config) -> io::Result<()> {
    let (cols, rows) = crossterm::terminal::size()?;
    let mut buf = GlyphBuffer::new(cols as usize, rows as usize);
    let mut cam = Camera::new(cols as usize, rows as usize);
    let mut scene = Scene::new();
    let mut fish = Betta::new();

    let frame_budget = Duration::from_secs_f64(1.0 / cfg.fps as f64);
    let mut last = Instant::now();
    let mut sim_time = 0.0f32;
    let mut frame = String::new();

    loop {
        let now = Instant::now();
        let mut dt = (now - last).as_secs_f32();
        last = now;
        // Clamp huge deltas (e.g. after a suspend) so nothing teleports.
        dt = dt.min(0.1);
        sim_time += dt;

        scene.update(dt);
        fish.update(dt);
        render_frame(&mut buf, &scene, &fish, &cam, sim_time);

        frame.clear();
        buf.to_ansi(&mut frame);
        stdout.write_all(frame.as_bytes())?;
        stdout.flush()?;

        // Wait for input up to the frame deadline; q/Esc/Ctrl-C quit.
        let deadline = frame_budget.saturating_sub(now.elapsed());
        if event::poll(deadline)? {
            match event::read()? {
                Event::Key(k) => match k.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break,
                    _ => {}
                },
                Event::Resize(w, h) => {
                    buf.resize(w as usize, h as usize);
                    cam = Camera::new(w as usize, h as usize);
                }
                _ => {}
            }
        }
    }
    Ok(())
}
