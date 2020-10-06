use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::str;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CommandError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Utf8(#[from] str::Utf8Error),
    #[error("Couldn't convert path to string")]
    PathToStr(),
}

pub struct IsolateBox {
    pub id: u32,
    pub path: PathBuf,
}

pub fn create_box(isolate_executable_path: &PathBuf, id: u32) -> Result<IsolateBox, CommandError> {
    cleanup_box(isolate_executable_path, id)?;

    let process = Command::new(isolate_executable_path)
        .arg("--init")
        .arg(format!("--box-id={}", id))
        .output()?;

    Ok(IsolateBox {
        id,
        path: PathBuf::from(str::from_utf8(&process.stdout)?.trim_end()).join("box"),
    })
}

pub struct RunParams {
    pub memory_limit_mib: u32,
    pub time_limit_ms: u32,
    pub binary_path: PathBuf,
    pub stdin: PathBuf,
}

const WALL_TIME: u32 = 10000;

#[derive(Debug)]
pub enum Status {
    Ok,
    TimeLimitExceeded,
    RuntimeError,
    Signal,
    FailedToStart,
}

#[derive(Debug)]
pub struct RunStats {
    pub time_ms: Option<u32>,
    pub time_wall_ms: Option<u32>,
    pub memory_kib: Option<u32>,
    pub exit_code: Option<u32>,
    pub message: Option<String>,
    pub status: Status,
    pub status_message: Option<String>,
}

use std::str::FromStr;

pub fn run(
    isolate_executable_path: &PathBuf,
    isolate_box: IsolateBox,
    execute_params: RunParams,
) -> Result<RunStats, CommandError> {
    fs::copy(execute_params.binary_path, &isolate_box.path.join("exe"))?;

    let stdin_path = isolate_box.path.join("stdin");
    fs::copy(execute_params.stdin, &stdin_path)?;

    let stdout_path = isolate_box.path.join("stdout");
    let stderr_path = isolate_box.path.join("stderr");

    let output = Command::new(isolate_executable_path)
        .current_dir(&isolate_box.path)
        .arg("--run")
        .arg(format!("--box-id={}", isolate_box.id))
        .arg(format!(
            "--wall-time={}.{:03}",
            WALL_TIME / 1000,
            WALL_TIME % 1000
        ))
        .arg(format!(
            "--time={}.{:03}",
            execute_params.time_limit_ms / 1000,
            execute_params.time_limit_ms % 1000
        ))
        .arg(format!(
            "--mem={}",
            /* input is in KiB */ execute_params.memory_limit_mib * 1024
        ))
        .arg("--stdin=stdin")
        .arg("--stdout=stdout")
        .arg("--stderr=stderr")
        .arg("--meta=-")
        .arg("--fsize=0") // Don't write to the disk at all
        .arg("--no-default-dirs")
        .arg(format!("--dir=box=./box:rw"))
        .arg("--")
        .arg("./exe")
        .output()?;

    let mut stats = RunStats {
        time_ms: None,
        time_wall_ms: None,
        memory_kib: None,
        exit_code: None,
        message: None,
        status: Status::Ok,
        status_message: None,
    };

    fn parse_ms(input: &str) -> Option<u32> {
        match input.find('.') {
            Some(dot_index) => {
                let integer = u32::from_str(&input[..dot_index]).ok();
                let fractional = u32::from_str(&input[dot_index + 1..]).ok();
                integer.and_then(|i| fractional.map(|f| i * 1000 + f))
            }
            None => None,
        }
    }

    for line in str::from_utf8(&output.stdout)?.split('\n') {
        if let Some(colon_index) = line.find(':') {
            let key = &line[..colon_index];
            let value = &line[colon_index + 1..];

            match key {
                "time" => stats.time_ms = parse_ms(value),
                "time-wall" => stats.time_wall_ms = parse_ms(value),
                "max-rss" => stats.memory_kib = u32::from_str(value).ok(),
                "exitcode" => stats.exit_code = u32::from_str(value).ok(),
                "message" => stats.message = Some(String::from(value)),
                "status" => {
                    let (code, message) = match value.find('.') {
                        Some(dot_index) => (&value[..dot_index], Some(&value[dot_index + 1..])),
                        None => (value, None),
                    };

                    match code {
                        // TODO: Check if MLE == RE
                        "RE" => stats.status = Status::RuntimeError,
                        "TO" => stats.status = Status::TimeLimitExceeded,
                        "XX" => stats.status = Status::FailedToStart,
                        "SG" => stats.status = Status::Signal,
                        _ => {}
                    }

                    stats.status_message = message.map(String::from);
                }
                _ => {}
            }
        }
    }

    Ok(stats)
}

pub struct CompileParams<'a> {
    pub memory_limit_mib: u32,
    pub time_limit_ms: u32,
    pub cmd: &'a [&'a str],
}

pub fn compile(
    isolate_executable_path: &PathBuf,
    isolate_box: IsolateBox,
    execute_params: CompileParams,
) -> Result<(), CommandError> {
    let stdout_path = isolate_box.path.join("stdout");
    let stderr_path = isolate_box.path.join("stderr");
    let meta_path = isolate_box.path.join("meta");

    Command::new(isolate_executable_path)
        .current_dir(&isolate_box.path)
        .arg("--run")
        .arg(format!("--box-id={}", isolate_box.id))
        .arg(format!(
            "--wall-time={}.{:03}",
            WALL_TIME / 1000,
            WALL_TIME % 1000
        ))
        .arg(format!(
            "--time={}.{:03}",
            execute_params.time_limit_ms / 1000,
            execute_params.time_limit_ms % 1000
        ))
        .arg(format!(
            "--mem={}",
            /* input is in KiB */ execute_params.memory_limit_mib * 1024
        ))
        .arg("--stdout=stdout")
        .arg("--stderr=stderr")
        .arg("--meta=meta")
        .arg("--fsize=0") // Don't write to the disk at all
        .arg("--")
        .args(execute_params.cmd)
        .output()?;
    Ok(())
}

pub fn cleanup_box(isolate_executable_path: &PathBuf, box_id: u32) -> Result<bool, CommandError> {
    Ok(Command::new(isolate_executable_path)
        .arg("--cleanup")
        .arg(format!("--box-id={}", box_id))
        .output()?
        .status
        .success())
}
