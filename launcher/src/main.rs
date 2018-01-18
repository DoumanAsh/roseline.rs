use std::process::Command;
use std::env;
use std::path::PathBuf;
use std::fs;

fn get_dirs() -> (PathBuf, PathBuf) {
    let mut current_dir = env::current_exe().unwrap();
    current_dir.pop();

    let mut log_path = current_dir.clone();
    log_path.push("roseline.log");

    let mut roseline_exe = current_dir;
    if cfg!(windows) {
        roseline_exe.push("roseline.exe");
    } else {
        roseline_exe.push("roseline");
    };

    (log_path, roseline_exe)
}

fn main() {
    let (log_path, roseline_exe) = get_dirs();
    println!("Roseline={}\nLog={}", roseline_exe.display(), log_path.display());

    if !roseline_exe.exists() {
        eprintln!("Roseline exe is missing: {}", roseline_exe.display());
        return;
    }

    let log = match fs::OpenOptions::new().append(true).create(true).open(&log_path) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("{}: Unable to open log file. Error: {}", log_path.display(), error);
            return;
        }
    };

    loop {
        let stdout = log.try_clone().expect("Cannot clone log file");
        let stderr = log.try_clone().expect("Cannot clone log file");
        match Command::new(&roseline_exe).stdout(stdout).stderr(stderr).status() {
            Ok(status) => {
                match status.success() {
                    true => println!("Roseline successfully finished"),
                    false => println!("Roseline finished with errors"),
                }
            },
            Err(error) => {
                eprintln!("Couldn't run Roseline. Error: {}", error);
                return;
            }
        }
    }
}
