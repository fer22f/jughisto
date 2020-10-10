use crate::language;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::str;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CommandError {
    #[error("couldn't open stdout")]
    StdoutIo(#[source] io::Error),
    #[error("couldn't open stdout")]
    StderrIo(#[source] io::Error),
    #[error("couldn't get command output")]
    CommandIo(#[source] io::Error),
    #[error("couldn't copy file")]
    CopyIo(#[source] io::Error),
    #[error("{0}")]
    IsolateCommandFailed(String),
    #[error(transparent)]
    Utf8(#[from] str::Utf8Error),
}

pub struct IsolateBox {
    pub id: i32,
    pub path: PathBuf,
}

pub fn create_box(isolate_executable_path: &PathBuf, id: i32) -> Result<IsolateBox, CommandError> {
    cleanup_box(isolate_executable_path, id)?;

    let output = Command::new(isolate_executable_path)
        .arg("--init")
        .arg("--cg")
        .arg(format!("--box-id={}", id))
        .output()
        .map_err(CommandError::CommandIo)?;

    if !output.status.success() {
        return Err(CommandError::IsolateCommandFailed(
            str::from_utf8(&output.stderr)?.into(),
        ));
    }

    Ok(IsolateBox {
        id,
        path: PathBuf::from(str::from_utf8(&output.stdout)?.trim_end()).join("box"),
    })
}

use std::ffi::OsStr;

pub struct RunParams<I, S>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    pub memory_limit_mib: i32,
    pub time_limit_ms: i32,
    pub stdin: bool,
    pub restricted: bool,
    pub command: I,
}

const WALL_TIME: i32 = 50_000;

#[derive(Debug, PartialEq)]
pub enum RunStatus {
    Ok,
    TimeLimitExceeded,
    MemoryLimitExceeded,
    RuntimeError,
    Signal,
    FailedToStart,
}

use std::io::Read;

#[derive(Debug)]
pub struct RunStats<R: Read> {
    pub time_ms: Option<i32>,
    pub time_wall_ms: Option<i32>,
    pub memory_kib: Option<i32>,
    pub exit_code: Option<i32>,
    pub message: Option<String>,
    pub exit_signal: Option<i32>,
    pub status: RunStatus,
    pub stdout: R,
    pub stderr: R,
}

pub struct ExecuteParams {
    pub memory_limit_mib: i32,
    pub time_limit_ms: i32,
    pub stdin: PathBuf,
}

use std::str::FromStr;

use std::fs::File;

pub fn execute(
    isolate_executable_path: &PathBuf,
    isolate_box: &IsolateBox,
    execute_params: &ExecuteParams,
) -> Result<RunStats<File>, CommandError> {
    let stdin_path = isolate_box.path.join("stdin");
    fs::copy(&execute_params.stdin, &stdin_path).map_err(CommandError::CopyIo)?;

    run(
        isolate_executable_path,
        isolate_box,
        RunParams {
            stdin: true,
            restricted: true,
            memory_limit_mib: execute_params.memory_limit_mib,
            time_limit_ms: execute_params.time_limit_ms,
            command: &["./exe"],
        },
    )
}

pub fn run<I, S>(
    isolate_executable_path: &PathBuf,
    isolate_box: &IsolateBox,
    run_params: RunParams<I, S>,
) -> Result<RunStats<File>, CommandError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(isolate_executable_path)
        .current_dir(&isolate_box.path)
        .arg("--run")
        .arg("--cg")
        .arg(format!("--box-id={}", isolate_box.id))
        .arg(format!(
            "--wall-time={}.{:03}",
            WALL_TIME / 1000,
            WALL_TIME % 1000
        ))
        .arg(format!(
            "--time={}.{:03}",
            run_params.time_limit_ms / 1000,
            run_params.time_limit_ms % 1000
        ))
        .arg(format!(
            "--mem={}",
            /* input is in KiB */ run_params.memory_limit_mib * 1024
        ))
        .args(if run_params.stdin {
            vec!["--stdin=stdin"]
        } else {
            vec![]
        })
        .arg("--stdout=stdout")
        .arg("--stderr=stderr")
        .arg("--meta=-")
        .args(if run_params.restricted {
            vec![
                "--no-default-dirs".into(),
                format!("--dir=box=./box:rw"),
                "--fsize=0".into(), // Don't write to the disk at all
            ]
        } else {
            vec!["--processes=0".into(), "--env=PATH=/usr/bin/".into()]
        })
        .arg("--")
        .args(run_params.command)
        .output()
        .map_err(CommandError::CommandIo)?;

    let stdout_path = isolate_box.path.join("stdout");
    let stderr_path = isolate_box.path.join("stderr");

    if match output.status.code() {
        // Ended by signal
        None => true,
        Some(c) => c > 1,
    } {
        return Err(CommandError::IsolateCommandFailed(
            str::from_utf8(&output.stderr)?.into(),
        ));
    }

    let mut stats = RunStats {
        time_ms: None,
        time_wall_ms: None,
        memory_kib: None,
        exit_code: None,
        message: None,
        exit_signal: None,
        status: RunStatus::Ok,
        stdout: File::open(stdout_path).map_err(CommandError::StdoutIo)?,
        stderr: File::open(stderr_path).map_err(CommandError::StderrIo)?,
    };

    fn parse_ms(input: &str) -> Option<i32> {
        match input.find('.') {
            Some(dot_index) => {
                let integer = i32::from_str(&input[..dot_index]).ok();
                let fractional = i32::from_str(&input[dot_index + 1..]).ok();
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
                "cg-mem" => stats.memory_kib = i32::from_str(value).ok(),
                "cg-oom-killed" => stats.status = RunStatus::MemoryLimitExceeded,
                "exitcode" => stats.exit_code = i32::from_str(value).ok(),
                "message" => stats.message = Some(value.into()),
                "exitsig" => stats.exit_signal = i32::from_str(value).ok(),
                "status" => {
                    stats.status = match value {
                        "RE" => RunStatus::RuntimeError,
                        "TO" => RunStatus::TimeLimitExceeded,
                        "XX" => RunStatus::FailedToStart,
                        "SG" => {
                            if stats.status == RunStatus::Ok {
                                RunStatus::Signal
                            } else {
                                stats.status
                            }
                        }
                        _ => RunStatus::RuntimeError,
                    }
                }
                _ => {}
            }
        }
    }

    if stats.status == RunStatus::Signal
        && stats.exit_signal == Some(6)
        && match stats.memory_kib {
            Some(memory_kib) => memory_kib >= run_params.memory_limit_mib * 1024,
            _ => false,
        }
    {
        stats.status = RunStatus::MemoryLimitExceeded;
    }

    Ok(stats)
}

pub struct CompileParams<'a> {
    pub memory_limit_mib: i32,
    pub time_limit_ms: i32,
    pub command: &'a language::Command,
}

pub fn compile(
    isolate_executable_path: &PathBuf,
    isolate_box: &IsolateBox,
    compile_params: CompileParams,
) -> Result<RunStats<File>, CommandError> {
    let mut command = vec![compile_params.command.binary_path.as_os_str()];
    let mut args = compile_params
        .command
        .args
        .iter()
        .map(|s| OsStr::new(s))
        .collect::<Vec<_>>();
    command.append(&mut args);

    run(
        isolate_executable_path,
        isolate_box,
        RunParams {
            stdin: false,
            restricted: false,
            memory_limit_mib: compile_params.memory_limit_mib,
            time_limit_ms: compile_params.time_limit_ms,
            command,
        },
    )
}

pub fn cleanup_box(isolate_executable_path: &PathBuf, box_id: i32) -> Result<bool, CommandError> {
    Ok(Command::new(isolate_executable_path)
        .arg("--cleanup")
        .arg("--cg")
        .arg(format!("--box-id={}", box_id))
        .output()
        .map_err(CommandError::CommandIo)?
        .status
        .success())
}
