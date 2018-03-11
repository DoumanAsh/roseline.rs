use std::process::Command;
use std::env;
use std::path::PathBuf;
use std::fs;
use std::thread;

fn get_dirs() -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let mut current_dir = env::current_exe().unwrap();
    current_dir.pop();

    let mut roseline_log = current_dir.clone();
    let mut roseline_web_log = current_dir.clone();
    roseline_log.push("roseline.log");
    roseline_web_log.push("roseline-web.log");

    let mut roseline_exe = current_dir;
    let mut roseline_web_exe = roseline_exe.clone();
    if cfg!(windows) {
        roseline_exe.push("roseline.exe");
        roseline_web_exe.push("roseline-web.exe");
    } else {
        roseline_exe.push("roseline");
        roseline_web_exe.push("roseline-web");
    };

    (roseline_log, roseline_exe, roseline_web_log, roseline_web_exe)
}

fn main() {
    let (roseline_log, roseline_exe, roseline_web_log, roseline_web_exe) = get_dirs();
    println!("Roseline={}\nLog={}", roseline_exe.display(), roseline_log.display());
    println!("Roseline-web={}\nLog={}", roseline_web_exe.display(), roseline_web_log.display());

    if !roseline_exe.exists() {
        eprintln!("Roseline exe is missing: {}", roseline_exe.display());
        return;
    }
    else if !roseline_web_exe.exists() {
        eprintln!("Roseline-web exe is missing: {}", roseline_exe.display());
        return;
    }

    let roseline_log = match fs::OpenOptions::new().append(true).create(true).open(&roseline_log) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("{}: Unable to open log file. Error: {}", roseline_log.display(), error);
            return;
        }
    };
    let roseline_web_log = match fs::OpenOptions::new().append(true).create(true).open(&roseline_web_log) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("{}: Unable to open log file. Error: {}", roseline_web_log.display(), error);
            return;
        }
    };

    thread::spawn(move || {
        loop {
            let stdout = roseline_log.try_clone().expect("Cannot clone log file");
            let stderr = roseline_log.try_clone().expect("Cannot clone log file");
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
    });

    loop {
        let stdout = roseline_web_log.try_clone().expect("Cannot clone log file");
        let stderr = roseline_web_log.try_clone().expect("Cannot clone log file");
        match Command::new(&roseline_web_exe).stdout(stdout).stderr(stderr).status() {
            Ok(status) => {
                match status.success() {
                    true => println!("Roseline-web successfully finished"),
                    false => println!("Roseline-web finished with errors"),
                }
            },
            Err(error) => {
                eprintln!("Couldn't run Roseline-web. Error: {}", error);
                return;
            }
        }
    }
}
