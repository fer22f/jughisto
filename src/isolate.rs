use log::info;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::str;
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct CommandTuple {
    pub binary_path: PathBuf,
    pub args: Vec<String>,
}

#[derive(Error, Debug)]
pub enum CommandError {
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

pub fn new_isolate_box(
    isolate_executable_path: &PathBuf,
    id: i32,
) -> Result<IsolateBox, CommandError> {
    let output = reset(isolate_executable_path, id)?;
    Ok(IsolateBox {
        id,
        path: PathBuf::from(str::from_utf8(&output.stdout)?.trim_end()).join("box"),
    })
}

pub fn reset(
    isolate_executable_path: &PathBuf,
    id: i32,
) -> Result<std::process::Output, CommandError> {
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

    Ok(output)
}

pub struct RunParams<'a> {
    pub memory_limit_kib: i32,
    pub time_limit_ms: i32,
    pub stdin_path: Option<&'a PathBuf>,
    pub uuid: &'a str,
    pub restricted: bool,
    pub process_limit: i32,
    pub command: &'a CommandTuple,
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

#[derive(Debug)]
pub struct RunStats {
    pub time_ms: Option<i32>,
    pub time_wall_ms: Option<i32>,
    pub memory_kib: Option<i32>,
    pub exit_code: Option<i32>,
    pub message: Option<String>,
    pub exit_signal: Option<i32>,
    pub status: RunStatus,
    pub stdout_path: PathBuf,
    pub stderr_path: PathBuf,
}

pub struct ExecuteParams<'a> {
    pub uuid: &'a str,
    pub memory_limit_kib: i32,
    pub time_limit_ms: i32,
    pub stdin_path: Option<&'a PathBuf>,
    pub process_limit: i32,
}

use std::str::FromStr;

pub fn execute(
    isolate_executable_path: &PathBuf,
    isolate_box: &IsolateBox,
    command: &CommandTuple,
    execute_params: &ExecuteParams,
) -> Result<RunStats, CommandError> {
    run(
        isolate_executable_path,
        isolate_box,
        RunParams {
            uuid: execute_params.uuid,
            stdin_path: execute_params.stdin_path,
            restricted: true,
            process_limit: execute_params.process_limit,
            memory_limit_kib: execute_params.memory_limit_kib,
            time_limit_ms: execute_params.time_limit_ms,
            command,
        },
    )
}

pub fn run(
    isolate_executable_path: &PathBuf,
    isolate_box: &IsolateBox,
    run_params: RunParams,
) -> Result<RunStats, CommandError> {
    let in_data_dir = PathBuf::from(format!("/data-{}", run_params.uuid));
    let out_data_dir = PathBuf::from("./data").canonicalize().unwrap();
    let stdin_path = run_params
        .stdin_path
        .map(|stdin_path| in_data_dir.join(stdin_path.strip_prefix("./data").unwrap()));
    info!("Binding in {:?} to out {:?}", in_data_dir, out_data_dir);
    info!("Using stdin path {:?}", stdin_path);

    let output = Command::new(isolate_executable_path)
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
            "--cg-mem={}",
            /* input is in KiB */ run_params.memory_limit_kib
        ))
        .args(if let Some(stdin_path) = stdin_path {
            vec![format!("--stdin={}", stdin_path.to_str().unwrap())]
        } else {
            vec![]
        })
        .arg("--stdout=stdout")
        .arg("--stderr=stderr")
        .arg("--meta=-")
        .arg("--env=PATH=/usr/bin")
        .arg("--no-default-dirs")
        .arg(format!(
            "--dir=box={}:rw",
            isolate_box.path.to_str().unwrap()
        ))
        .args(if run_params.restricted {
            vec![
                // python, java needs it
                "--dir=lib",
                "--dir=lib64:maybe",
                // java: /usr/bin/java and python: /usr/bin/python3
                "--dir=usr/lib",
                "--dir=usr/bin",
                // java: libjli.so
                "--dir=proc=proc:fs",
            ]
        } else {
            vec![
                "--dir=bin",
                "--dir=lib",
                "--dir=lib64:maybe",
                "--dir=usr/lib",
                "--dir=usr/libexec",
                "--dir=usr/bin",
                "--dir=usr/include",
                "--dir=proc=proc:fs",
            ]
        })
        .arg(format!("--processes={}", run_params.process_limit))
        .arg(format!(
            "--dir={}={}",
            in_data_dir.to_str().unwrap(),
            out_data_dir.to_str().unwrap()
        ))
        .args(if run_params.restricted {
            vec![
                "--fsize=0", // Don't write to the disk at all
            ]
        } else {
            vec![]
        })
        .arg("--")
        .arg(&run_params.command.binary_path)
        .args(&run_params.command.args)
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
        stdout_path,
        stderr_path,
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

    Ok(stats)
}

pub struct CompileParams<'a> {
    pub uuid: &'a str,
    pub memory_limit_kib: i32,
    pub time_limit_ms: i32,
    pub command: &'a CommandTuple,
}

pub fn compile(
    isolate_executable_path: &PathBuf,
    isolate_box: &IsolateBox,
    compile_params: CompileParams,
) -> Result<RunStats, CommandError> {
    run(
        isolate_executable_path,
        isolate_box,
        RunParams {
            uuid: compile_params.uuid,
            stdin_path: None,
            restricted: false,
            process_limit: 40,
            memory_limit_kib: compile_params.memory_limit_kib,
            time_limit_ms: compile_params.time_limit_ms,
            command: compile_params.command,
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
