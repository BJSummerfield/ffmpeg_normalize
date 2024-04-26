use clap::{builder::Command, Arg, ArgAction, ArgMatches};
use core::time::Duration;
use serde::{Deserialize, Serialize};
use std::io;
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

#[derive(Serialize, Deserialize, Debug)]
struct Loudness {
    input_i: String,
    input_tp: String,
    input_lra: String,
    input_thresh: String,
    target_offset: String,
}

struct CliConfig {
    input_path: String,
    integrated_loudness: String,
    loudness_range: String,
    true_peak: String,
    down_mix: bool,
}

struct ProgressSpinner;

impl ProgressSpinner {
    fn show_progress() -> (Arc<AtomicBool>, thread::JoinHandle<()>) {
        const PROGRESS_CHARS: [&str; 12] =
            ["⠂", "⠃", "⠁", "⠉", "⠈", "⠘", "⠐", "⠰", "⠠", "⠤", "⠄", "⠆"];
        let finished = Arc::new(AtomicBool::new(false));
        let stop_signal = Arc::clone(&finished);
        let handle = thread::spawn(move || {
            for pc in PROGRESS_CHARS.iter().cycle() {
                if stop_signal.load(Ordering::Relaxed) {
                    break;
                };
                eprint!("Processing 1st Loudnorm Pass {}\r", pc);
                thread::sleep(Duration::from_millis(250));
            }
        });
        (finished, handle)
    }
}

impl CliConfig {
    fn from_matches(matches: &ArgMatches) -> Result<Self, &'static str> {
        Ok(Self {
            input_path: matches
                .get_one::<String>("input")
                .ok_or("Missing input file path")?
                .clone(),
            integrated_loudness: matches
                .get_one::<String>("integrated_loudness")
                .unwrap()
                .clone(),
            loudness_range: matches.get_one::<String>("loudness_range").unwrap().clone(),
            true_peak: matches.get_one::<String>("true_peak").unwrap().clone(),
            down_mix: matches.get_flag("down_mix"),
        })
    }

    fn setup_cli() -> ArgMatches {
        Command::new("ffmpeg-loudnorm-helper")
            .about("Helps normalize loudness of audio files.")
            .arg(
                Arg::new("input")
                    .help("Path to the input file.")
                    .required(true),
            )
            .arg(
                Arg::new("integrated_loudness")
                    .short('i')
                    .long("integrated_loudness")
                    .default_value("-23.0")
                    .help("Integrated loudness target."),
            )
            .arg(
                Arg::new("loudness_range")
                    .short('l')
                    .long("loudness_range")
                    .default_value("7.0")
                    .help("Loudness range target."),
            )
            .arg(
                Arg::new("true_peak")
                    .short('t')
                    .long("true_peak")
                    .default_value("-2.0")
                    .help("Maximum true peak."),
            )
            .arg(
                Arg::new("down_mix")
                    .short('d')
                    .long("down_mix")
                    .action(ArgAction::SetTrue)
                    .help("Downmix to 16bit 48kHz stereo."),
            )
            .get_matches()
    }
}

struct LoudnessAnalyzer;

impl LoudnessAnalyzer {
    fn analyze_and_print_loudness(config: &CliConfig) -> io::Result<()> {
        let filter_settings = FilterSettings::construct(config, None);
        let output = Self::analyze_loudness(&config.input_path, &filter_settings)?;

        match serde_json::from_str::<Loudness>(&Self::extract_json(&output)) {
            Ok(loudness) => {
                println!("{}", FilterSettings::construct(config, Some(&loudness)));
                Ok(())
            }
            Err(e) => {
                eprintln!("Failed to parse JSON: {}", e);
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid JSON data",
                ))
            }
        }
    }

    fn analyze_loudness(input_path: &str, filter_settings: &str) -> io::Result<String> {
        let (finished, spinner_handle) = ProgressSpinner::show_progress();

        let process = ProcessCommand::new("ffmpeg")
            .args(&[
                "-i",
                input_path,
                "-hide_banner",
                "-vn",
                "-af",
                filter_settings,
                "-f",
                "null",
                "-",
            ])
            .stderr(Stdio::piped())
            .spawn()?;

        let output = process.wait_with_output()?;

        finished.store(true, Ordering::Relaxed);

        if let Err(e) = spinner_handle.join() {
            eprintln!("Error stopping the spinner: {:?}", e);
        }

        // Check if FFmpeg was successful
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stderr).to_string())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "FFmpeg process failed",
            ))
        }
    }

    fn extract_json(output: &str) -> String {
        let json_start = output.rfind('{').unwrap_or(0);
        let json_end = output[json_start..].find('}').unwrap_or(output.len() - 1) + json_start + 1;
        let json = &output[json_start..json_end];
        json.to_string()
    }
}

struct FilterSettings;

impl FilterSettings {
    fn construct(config: &CliConfig, loudness: Option<&Loudness>) -> String {
        let mut filter = format!(
            "{}loudnorm=I={}:LRA={}:TP={}",
            if config.down_mix {
                "aformat=sample_fmts=s16:sample_rates=48000:channel_layouts=stereo,"
            } else {
                ""
            },
            config.integrated_loudness,
            config.loudness_range,
            config.true_peak,
        );

        if let Some(l) = loudness {
            filter += &format!(
                ":linear=true:measured_I={}:measured_TP={}:measured_LRA={}:measured_thresh={}:offset={}",
                l.input_i, l.input_tp, l.input_lra, l.input_thresh, l.target_offset
            );
        } else {
            filter += ":print_format=json";
        }

        filter
    }
}

fn main() -> io::Result<()> {
    let matches = CliConfig::setup_cli();
    let config = CliConfig::from_matches(&matches).expect("Error parsing command line arguments");
    LoudnessAnalyzer::analyze_and_print_loudness(&config)
}
