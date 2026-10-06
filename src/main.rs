use std::process::ExitCode;

use asusdisplay::{Kind, Monitor, Options, Result, parse_number};
use clap::{Parser, Subcommand};

/// Control ASUS monitors over DDC/CI.
#[derive(Parser)]
#[command(version, after_help = "Values: 50, +5, -10, 0x32, a choice name from 'features', or on/off/toggle.")]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Monitor: index from 'list', part of the model name, serial, id, connector, or 'all'
    #[arg(short, long, global = true)]
    display: Option<String>,

    /// Also show features the monitor doesn't advertise
    #[arg(short, long, global = true)]
    all: bool,

    /// Allow disruptive settings and values the monitor doesn't advertise
    #[arg(short, long, global = true)]
    force: bool,

    /// Use /dev/i2c-N and skip discovery
    #[arg(short, long, global = true)]
    bus: Option<u32>,

    /// Scale DDC/CI delays; try 2 if reads fail
    #[arg(long, global = true, default_value_t = 1.0)]
    sleep_mult: f64,

    /// Show bus discovery
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Command {
    /// List monitors
    List,
    /// Show every setting and its value
    Status,
    /// Show feature names, VCP codes and accepted values
    Features,
    /// Read a setting
    Get { feature: String },
    /// Change a setting
    Set {
        feature: String,
        #[arg(allow_hyphen_values = true)]
        value: String,
    },
    /// List settings the monitor doesn't expose, and why
    Debug,
    /// Print the monitor's capabilities string
    Caps,
    /// Read or write any VCP code, without checks
    Raw {
        code: String,
        value: Option<String>,
    },
    /// Control the monitor light bar (USB HID 0b05:1ac8)
    Lightbar {
        #[command(subcommand)]
        action: LightbarAction,
    },
}

#[derive(Subcommand)]
enum LightbarAction {
    /// White light: brightness 0-100, temperature 2700-6500 K
    White { brightness: u8, kelvin: u16 },
    /// Colour: effect is static/breath/strobe/cycle/rainbow, then R G B (0-255)
    Rgb { effect: String, r: u8, g: u8, b: u8 },
    /// Turn the light bar off
    Off,
}

fn main() -> ExitCode {
    match run(&Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<()> {
    if let Command::Lightbar { action } = &cli.command {
        return lightbar(action);
    }

    let options = Options { bus: cli.bus, sleep_multiplier: cli.sleep_mult, verbose: cli.verbose };
    let mut monitors = asusdisplay::enumerate(&options);
    if monitors.is_empty() {
        let hint = if cfg!(target_os = "linux") { "Load i2c-dev and install the udev rule (see README), or retry with -v" } else { "check the monitor supports DDC/CI and it's enabled in the OSD" };
        return Err(format!("no DDC/CI monitors found. {hint}"));
    }

    if matches!(cli.command, Command::List) {
        for (i, m) in monitors.iter().enumerate() {
            let serial = m.edid.as_ref().and_then(|e| e.serial.as_deref()).unwrap_or("-");
            println!("{i}: {:<12} {:<14} {:<6} serial {serial}", m.model, m.id, m.connector.as_deref().unwrap_or(""));
        }
        return Ok(());
    }

    let selected = select(&monitors, cli.display.as_deref())?;
    for &i in &selected {
        let m = &mut monitors[i];
        if selected.len() > 1 {
            println!("== {} ({})", m.model, m.id);
        }
        command(m, cli)?;
    }
    Ok(())
}

fn select(monitors: &[Monitor], selector: Option<&str>) -> Result<Vec<usize>> {
    let Some(s) = selector else { return Ok(vec![0]) };
    if s.eq_ignore_ascii_case("all") {
        return Ok((0..monitors.len()).collect());
    }
    if let Ok(i) = s.parse::<usize>() {
        return if i < monitors.len() { Ok(vec![i]) } else { Err(format!("no monitor {i}, see 'list'")) };
    }

    let s = s.to_lowercase();
    let found: Vec<usize> = (0..monitors.len())
        .filter(|&i| {
            let m = &monitors[i];
            let serial = m.edid.as_ref().and_then(|e| e.serial.as_deref()).unwrap_or_default();
            m.model.to_lowercase().contains(&s)
                || [m.id.as_str(), m.connector.as_deref().unwrap_or_default(), serial].iter().any(|v| v.to_lowercase() == s)
        })
        .collect();
    if found.is_empty() { Err(format!("no monitor matches '{s}', see 'list'")) } else { Ok(found) }
}

fn command(m: &mut Monitor, cli: &Cli) -> Result<()> {
    match &cli.command {
        Command::List => unreachable!("handled before selecting monitors"),
        Command::Status => {
            println!("{} ({}, {:?})", m.model, m.id, m.product_line());
            let mut group = "";
            for f in m.features(cli.all).into_iter().filter(|f| f.kind != Kind::Action) {
                if f.group != group {
                    group = f.group;
                    println!("\n  {group}");
                }
                let value = m.read(&f).map_or_else(|e| format!("<{e}>"), |r| r.text);
                println!("    {:<24} {value}", f.key);
            }
        }
        Command::Features => {
            for f in m.features(cli.all) {
                let code = if f.kind == Kind::Toggle { format!("0x{:02X}/{:04X}", f.code, f.bit) } else { format!("0x{:02X}", f.code) };
                println!("{:<24} {code:<12} {}{}", f.key, f.label, if f.force { " (needs --force)" } else { "" });
                if !f.choices.is_empty() {
                    let names: Vec<_> = f.choices.iter().map(|c| c.name.as_str()).collect();
                    println!("{:<37} {}", "", names.join(", "));
                }
            }
        }
        Command::Get { feature } => {
            let f = m.feature(feature).ok_or_else(|| format!("unknown feature '{feature}', see 'features --all'"))?;
            println!("{}", m.read(&f)?.text);
        }
        Command::Set { feature, value } => {
            let f = m.feature(feature).ok_or_else(|| format!("unknown feature '{feature}', see 'features --all'"))?;
            m.write(&f, value, cli.force)?;
        }
        Command::Debug => {
            let un = m.unsupported();
            if un.is_empty() {
                println!("every known setting is supported on this monitor");
            }
            for s in un {
                println!("{:<24} 0x{:02X}  {}", s.feature.key, s.feature.code, s.reason);
            }
        }
        Command::Caps => {
            let caps = m.capabilities()?;
            println!("{}\n", caps.raw);
            for (code, values) in &caps.vcp {
                let values: Vec<_> = values.iter().map(|v| format!("{v:02X}")).collect();
                println!("0x{code:02X} {}", values.join(" "));
            }
        }
        Command::Raw { code, value } => {
            let code = parse_number(code).and_then(|c| u8::try_from(c).ok()).ok_or("code must be 0-255 (0xE2 or 226)")?;
            if let Some(value) = value {
                let value = parse_number(value).and_then(|v| u16::try_from(v).ok()).ok_or("value must be 0-65535")?;
                m.set_vcp(code, value)?;
            } else {
                let vcp = m.get_vcp(code)?;
                println!("current {0} (0x{0:04X}), max {1} (0x{1:04X})", vcp.current, vcp.max);
            }
        }
        Command::Lightbar { .. } => unreachable!("handled before selecting monitors"),
    }
    Ok(())
}

fn lightbar(action: &LightbarAction) -> Result<()> {
    use asusdisplay::lightbar::{self, Effect};

    let mut bars = lightbar::find();
    let bar = bars.first_mut().ok_or("no light bar found (USB 0b05:1ac8)")?;
    match action {
        LightbarAction::Off => bar.off(),
        LightbarAction::White { brightness, kelvin } => bar.set_white(*brightness, *kelvin),
        LightbarAction::Rgb { effect, r, g, b } => {
            let effect = match effect.to_ascii_lowercase().as_str() {
                "static" => Effect::Static,
                "breath" => Effect::Breath,
                "strobe" => Effect::Strobe,
                "cycle" => Effect::Cycle,
                "rainbow" => Effect::Rainbow,
                _ => return Err("effect is static, breath, strobe, cycle or rainbow".into()),
            };
            bar.set_rgb(effect, *r, *g, *b)
        }
    }
}
