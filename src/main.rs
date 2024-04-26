use clap::{builder::Command, Arg, ArgAction, ArgMatches};
use core::time::Duration;
use serde::{Deserialize, Serialize};
use std::{
    io::{self, IsTerminal},
    process::{Command as ProcessCommand, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};

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

impl CliConfig {
    fn new(matches: &ArgMatches) -> Result<Self, io::Error> {
        Ok(Self {
            input_path: matches
                .get_one::<String>("input")
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Missing input file path"))?
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

        serde_json::from_str::<Loudness>(&Self::extract_json(&output))
            .map(|loudness| println!("{}", FilterSettings::construct(config, Some(&loudness))))
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Failed to parse JSON: {}", e),
                )
            })
    }

    fn analyze_loudness(input_path: &str, filter_settings: &str) -> io::Result<String> {
        let spinner = ProgressSpinner::show_progress();

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
        spinner.store(false, Ordering::SeqCst);

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
        output[json_start..].find('}').map_or_else(
            || String::new(),
            |end| output[json_start..=json_start + end].to_string(),
        )
    }
}

struct FilterSettings;

impl FilterSettings {
    fn construct(config: &CliConfig, loudness: Option<&Loudness>) -> String {
        let base = if config.down_mix {
            "aformat=sample_fmts=s16:sample_rates=48000:channel_layouts=stereo,"
        } else {
            ""
        };
        let loudness_params = loudness.map_or_else(
            || ":print_format=json".to_string(),
            |l| format!(":linear=true:measured_I={}:measured_TP={}:measured_LRA={}:measured_thresh={}:offset={}",
                        l.input_i, l.input_tp, l.input_lra, l.input_thresh, l.target_offset));
        format!(
            "{}loudnorm=I={}:LRA={}:TP={}{}",
            base,
            config.integrated_loudness,
            config.loudness_range,
            config.true_peak,
            loudness_params
        )
    }
}

struct ProgressSpinner;

impl ProgressSpinner {
    fn show_progress() -> Arc<AtomicBool> {
        const PROGRESS_CHARS: [&str; 12] =
            ["⠂", "⠃", "⠁", "⠉", "⠈", "⠘", "⠐", "⠰", "⠠", "⠤", "⠄", "⠆"];
        let finished = Arc::new(AtomicBool::new(false));
        if io::stderr().is_terminal() {
            let stop_signal = Arc::clone(&finished);
            let _ = thread::spawn(move || {
                for pc in PROGRESS_CHARS.iter().cycle() {
                    if stop_signal.load(Ordering::Relaxed) {
                        break;
                    };
                    eprint!("Processing {}\r", pc);
                    thread::sleep(Duration::from_millis(250));
                }
            });
        }
        finished
    }
}

fn main() -> io::Result<()> {
    let matches = CliConfig::setup_cli();
    let config = CliConfig::new(&matches).expect("Error parsing command line arguments");
    LoudnessAnalyzer::analyze_and_print_loudness(&config)
}
