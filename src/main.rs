use clap::{builder::Command, Arg, ArgAction, ArgMatches};
use serde::{Deserialize, Serialize};
use std::io;
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
                    .default_value("-24.0")
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

        let loudness: Loudness = serde_json::from_str(&Self::extract_json(&output)).unwrap();
        println!("{}", FilterSettings::construct(config, Some(&loudness)));

        Ok(())
    }

    fn analyze_loudness(input_path: &str, filter_settings: &str) -> io::Result<String> {
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
            .spawn()?
            .wait_with_output()?;

        if process.status.success() {
            Ok(String::from_utf8_lossy(&process.stderr).to_string())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "FFmpeg process failed",
            ))
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
