use clap::{builder::Command, Arg, ArgAction};
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
        let output_s = String::from_utf8_lossy(&output.stderr);
        Ok(extract_json(&output_s))
    } else {
        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            String::from_utf8_lossy(&output.stderr).to_string(),
        )))
    }
}

fn extract_json(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let (_, json_lines) = lines.split_at(lines.len() - 12);
    json_lines.join("\n")
}

fn setup_cli() -> clap::ArgMatches {
    Command::new("ffmpeg-loudnorm-helper")
        .arg(
            Arg::new("input")
                .help("Path to the input file.")
                .required(true),
        )
        .args(&[
            Arg::new("integrated_loudness")
                .short('i')
                .long("integrated_loudness")
                .ignore_case(true)
                .default_value("-24.0")
                .help("Integrated loudness target."),
            Arg::new("loudness_range")
                .short('l')
                .long("loudness_range")
                .ignore_case(true)
                .default_value("7.0")
                .help("Loudness range target."),
            Arg::new("true_peak")
                .short('t')
                .long("true_peak")
                .ignore_case(true)
                .default_value("-2.0")
                .help("Maximum true peak."),
            Arg::new("resample")
                .short('r')
                .long("resample")
                .action(ArgAction::SetTrue)
                .help("Add a resampling filter hardcoded to 48kHz after loudnorm."),
        ])
        .get_matches()
}

fn main() {
    let matches = setup_cli();

    let input_path = matches.get_one::<String>("input").unwrap();
    let settings = format!(
        "loudnorm=I={}:LRA={}:tp={}:print_format=json",
        matches
            .get_one::<String>("integrated_loudness")
            .unwrap()
            .parse::<f32>()
            .unwrap()
            .clamp(-70.0, -5.0),
        matches
            .get_one::<String>("loudness_range")
            .unwrap()
            .parse::<f32>()
            .unwrap()
            .clamp(1.0, 20.0),
        matches
            .get_one::<String>("true_peak")
            .unwrap()
            .parse::<f32>()
            .unwrap()
            .clamp(-9.0, 0.0),
    );

    match analyze_loudness(input_path, &settings) {
        Ok(json_output) => {
            let loudness: Loudness = serde_json::from_str(&json_output).unwrap();
            let af = format!(
                "-af loudnorm=linear=true:I={}:LRA={}:TP={}:measured_I={}:measured_TP={}:measured_LRA={}:measured_thresh={}:offset={}{}",
                matches.get_one::<String>("integrated_loudness").unwrap(),
                matches.get_one::<String>("loudness_range").unwrap(),
                matches.get_one::<String>("true_peak").unwrap(),
                loudness.input_i, loudness.input_tp, loudness.input_lra, loudness.input_thresh, loudness.target_offset,
                if matches.get_flag("resample") {
                    ",aresample=osr=48000,aresample=resampler=soxr:precision=28"
                } else {
                    ""
                }
            );
            println!("{}", af);
        }
        Err(e) => eprintln!("Error processing file: {}", e),
    }
}
