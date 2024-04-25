use clap::{builder::Command, Arg, ArgAction, ArgMatches};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    process::{Command as ProcessCommand, Stdio},
};

#[derive(Serialize, Deserialize, Debug)]
struct Loudness {
    input_i: String,
    input_tp: String,
    input_lra: String,
    input_thresh: String,
    target_offset: String,
}

#[derive(Debug)]
struct CliConfig {
    input_path: String,
    integrated_loudness: String,
    loudness_range: String,
    true_peak: String,
    down_mix: bool,
    resample: bool,
}

impl CliConfig {
    fn new(matches: &ArgMatches) -> Result<Self, &'static str> {
        Ok(Self {
            input_path: matches
                .get_one::<String>("input")
                .ok_or("Missing input path")?
                .clone(),
            integrated_loudness: matches
                .get_one::<String>("integrated_loudness")
                .ok_or("Missing integrated loudness target")?
                .clone(),
            loudness_range: matches
                .get_one::<String>("loudness_range")
                .ok_or("Missing loudness range target")?
                .clone(),
            true_peak: matches
                .get_one::<String>("true_peak")
                .ok_or("Missing true peak")?
                .clone(),
            down_mix: matches.get_flag("down_mix"),
            resample: matches.get_flag("resample"),
        })
    }
}

fn analyze_loudness(input_path: &str, filter_settings: &str) -> Result<String, Box<dyn Error>> {
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

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stderr).to_string())
    } else {
        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to process the audio file.",
        )))
    }
}

/// Extracts the JSON part from the ffmpeg output.
fn extract_json(output: &str) -> String {
    if let Some(start) = output.rfind('{') {
        let json_part = &output[start..];
        json_part.to_string()
    } else {
        String::new()
    }
}

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
            setup_flag("down_mix", 'd', "Downmix to 16bit 48khz stereo."),
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

fn construct_filter_settings(config: &CliConfig) -> String {
    format!(
        "{}loudnorm=I={}:LRA={}:tp={}:print_format=json",
        if config.down_mix {
            "aformat=sample_fmts=s16:sample_rates=48000:channel_layouts=stereo,"
        } else {
            ""
        },
        config.integrated_loudness,
        config.loudness_range,
        config.true_peak,
    )
}

fn construct_audio_filter_chain(config: &CliConfig, loudness: &Loudness) -> String {
    format!(
        "{}loudnorm=linear=true:I={}:LRA={}:TP={}:measured_I={}:measured_TP={}:measured_LRA={}:measured_thresh={}:offset={}{}",
        if config.down_mix { "aformat=sample_fmts=s16:sample_rates=48000:channel_layouts=stereo," } else { "" },
        config.integrated_loudness,
        config.loudness_range,
        config.true_peak,
        loudness.input_i, loudness.input_tp, loudness.input_lra, loudness.input_thresh, loudness.target_offset,
        if config.resample { ",aresample=osr=48000,aresample=resampler=soxr:precision=28" } else { "" }
    )
}

fn main() {
    let matches = setup_cli();
    let config = CliConfig::new(&matches).expect("Error parsing command line arguments");

    match analyze_loudness(&config.input_path, &construct_filter_settings(&config)) {
        Ok(output) => {
            let loudness: Loudness = serde_json::from_str(&extract_json(&output)).unwrap();
            println!("{}", construct_audio_filter_chain(&config, &loudness));
        }
        Err(e) => eprintln!("Error processing file: {}", e),
    }
}
