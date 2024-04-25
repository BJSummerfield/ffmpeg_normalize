use clap::{builder::Command, Arg, ArgAction, ArgMatches};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::process::{Command as ProcessCommand, Stdio};

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
    resample: bool,
}

impl CliConfig {
    fn from_matches(matches: &ArgMatches) -> Result<Self, CliError> {
        Ok(Self {
            input_path: matches
                .get_one::<String>("input")
                .ok_or(CliError::MissingInput)?
                .clone(),
            integrated_loudness: matches
                .get_one::<String>("integrated_loudness")
                .ok_or(CliError::MissingArgument("integrated loudness"))?
                .clone(),
            loudness_range: matches
                .get_one::<String>("loudness_range")
                .ok_or(CliError::MissingArgument("loudness range"))?
                .clone(),
            true_peak: matches
                .get_one::<String>("true_peak")
                .ok_or(CliError::MissingArgument("true peak"))?
                .clone(),
            down_mix: matches.get_flag("down_mix"),
            resample: matches.get_flag("resample"),
        })
    }
}

struct LoudnessAnalyzer;

impl LoudnessAnalyzer {
    fn analyze_loudness(input_path: &str, filter_settings: &str) -> Result<String, CliError> {
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
            .spawn()
            .map_err(|_| CliError::ProcessFailed)?;

        let output = process
            .wait_with_output()
            .map_err(|_| CliError::ProcessFailed)?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stderr).to_string())
        } else {
            Err(CliError::ProcessFailed)
        }
    }

    fn extract_json(output: &str) -> String {
        output
            .rfind('{')
            .map(|start| output[start..].to_string())
            .unwrap_or_default()
    }
}

struct FilterSettings;

impl FilterSettings {
    fn construct(filter_config: &CliConfig, loudness: Option<&Loudness>) -> String {
        match loudness {
            Some(l) => {
                // Construct filter string using loudness measurements
                format!(
                    "{}loudnorm=linear=true:I={}:LRA={}:TP={}:measured_I={}:measured_TP={}:measured_LRA={}:measured_thresh={}:offset={}{}",
                    if filter_config.down_mix {
                        "aformat=sample_fmts=s16:sample_rates=48000:channel_layouts=stereo,"
                    } else {
                        ""
                    },
                    filter_config.integrated_loudness,
                    filter_config.loudness_range,
                    filter_config.true_peak,
                    l.input_i, l.input_tp, l.input_lra, l.input_thresh, l.target_offset,
                    if filter_config.resample {
                        ",aresample=osr=48000,aresample=resampler=soxr:precision=28"
                    } else {
                        ""
                    }
                )
            }
            None => {
                // Construct initial filter settings without loudness measurements
                format!(
                    "{}loudnorm=I={}:LRA={}:tp={}:print_format=json",
                    if filter_config.down_mix {
                        "aformat=sample_fmts=s16:sample_rates=48000:channel_layouts=stereo,"
                    } else {
                        ""
                    },
                    filter_config.integrated_loudness,
                    filter_config.loudness_range,
                    filter_config.true_peak
                )
            }
        }
    }
}

#[derive(Debug)]
enum CliError {
    MissingInput,
    MissingArgument(&'static str),
    ProcessFailed,
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::MissingInput => write!(f, "Input path is required."),
            CliError::MissingArgument(arg) => write!(f, "Missing argument: {}", arg),
            CliError::ProcessFailed => write!(f, "FFmpeg process failed."),
        }
    }
}

impl Error for CliError {}

fn setup_cli() -> clap::ArgMatches {
    Command::new("ffmpeg-loudnorm-helper")
        .about("Helps normalize loudness of audio files.")
        .arg(
            Arg::new("input")
                .help("Path to the input file.")
                .required(true),
        )
        .args(&[
            setup_arg(
                "integrated_loudness",
                'i',
                "-24.0",
                "Integrated loudness target.",
            ),
            setup_arg("loudness_range", 'l', "7.0", "Loudness range target."),
            setup_arg("true_peak", 't', "-2.0", "Maximum true peak."),
            setup_flag("down_mix", 'd', "Downmix to 16bit 48kHz stereo."),
            setup_flag(
                "resample",
                'r',
                "Add a resampling filter hardcoded to 48kHz after loudnorm.",
            ),
        ])
        .get_matches()
}

fn setup_flag(name: &'static str, short: char, help: &'static str) -> Arg {
    Arg::new(name)
        .short(short)
        .long(name)
        .action(ArgAction::SetTrue)
        .help(help)
}

fn setup_arg(name: &'static str, short: char, default: &'static str, help: &'static str) -> Arg {
    Arg::new(name)
        .short(short)
        .long(name)
        .default_value(default)
        .help(help)
}

fn main() {
    let matches = setup_cli();
    let config = CliConfig::from_matches(&matches).expect("Error parsing command line arguments");
    let filter_settings = FilterSettings::construct(&config, None);

    match LoudnessAnalyzer::analyze_loudness(&config.input_path, &filter_settings) {
        Ok(output) => {
            let loudness: Loudness =
                serde_json::from_str(&LoudnessAnalyzer::extract_json(&output)).unwrap();
            println!("{}", FilterSettings::construct(&config, Some(&loudness)));
        }
        Err(e) => eprintln!("{}", e),
    }
}
