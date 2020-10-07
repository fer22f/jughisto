use crate::isolate;
use crate::isolate::{CommandError, CompileParams, IsolateBox};
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::Read;
use std::path::PathBuf;

pub struct Command {
    pub binary_path: PathBuf,
    pub args: Vec<String>,
}

pub enum Run {
    RunExe,
    Commands(Vec<Command>),
}

pub enum Compile {
    NoCompile,
    Commands(Vec<Command>),
}

pub struct LanguageParams {
    compile: Compile,
    run: Run,
}

pub fn get_supported_languages() -> HashMap<String, LanguageParams> {
    fn build_gcc_args<'a>(x: &'a str, std: &'a str) -> Vec<String> {
        return vec![
            // Static linked, the judging process has no access to shared libraries
            "-static".into(),
            // ONLINE_JUDGE define as in Codeforces
            "-DONLINE_JUDGE".into(),
            // Link to the math library
            "-lm".into(),
            // Strip all symbols
            "-s".into(),
            // Use std
            format!("-std={}", std),
            // Define language used
            "-x".into(),
            x.into(),
            // Level 2 optimization
            "-O2".into(),
            // Output to exe
            "-o".into(),
            "exe".into(),
            // Input from source
            "source".into(),
        ];
    }

    let mut languages = HashMap::new();
    languages.insert(
        "GCC C99".into(),
        LanguageParams {
            compile: Compile::Commands(vec![Command {
                binary_path: "/usr/bin/gcc".into(),
                args: build_gcc_args("c", "c99"),
            }]),
            run: Run::RunExe,
        },
    );
    languages.insert(
        "GCC C11".into(),
        LanguageParams {
            compile: Compile::Commands(vec![Command {
                binary_path: "/usr/bin/gcc".into(),
                args: build_gcc_args("c", "c11"),
            }]),
            run: Run::RunExe,
        },
    );
    languages.insert(
        "GCC C14".into(),
        LanguageParams {
            compile: Compile::Commands(vec![Command {
                binary_path: "/usr/bin/gcc".into(),
                args: build_gcc_args("c", "c14"),
            }]),
            run: Run::RunExe,
        },
    );
    languages.insert(
        "GCC C++11".into(),
        LanguageParams {
            compile: Compile::Commands(vec![Command {
                binary_path: "/usr/bin/g++".into(),
                args: build_gcc_args("c++", "c++11"),
            }]),
            run: Run::RunExe,
        },
    );
    languages.insert(
        "GCC C++14".into(),
        LanguageParams {
            compile: Compile::Commands(vec![Command {
                binary_path: "/usr/bin/g++".into(),
                args: build_gcc_args("c++", "c++14"),
            }]),
            run: Run::RunExe,
        },
    );
    languages.insert(
        "GCC C++17".into(),
        LanguageParams {
            compile: Compile::Commands(vec![Command {
                binary_path: "/usr/bin/g++".into(),
                args: build_gcc_args("c++", "c++17"),
            }]),
            run: Run::RunExe,
        },
    );
    languages.insert(
        "Java".into(),
        LanguageParams {
            compile: Compile::Commands(vec![Command {
                binary_path: "/usr/bin/javac".into(),
                args: vec!["-cp".into(), "\".;*\"".into(), "source".into()],
            }]),
            run: Run::Commands(vec![Command {
                binary_path: "/usr/bin/java".into(),
                args: vec![
                    "-Xmx512M".into(),
                    "-Xss64M".into(),
                    "-DONLINE_JUDGE=true".into(),
                    "-Duser.language=en".into(),
                    "-Duser.region=US".into(),
                    "-Duser.variant=US".into(),
                    "-jar".into(),
                    "exe".into(),
                ],
            }]),
        },
    );
    languages.insert(
        "Python 3".into(),
        LanguageParams {
            compile: Compile::NoCompile,
            run: Run::Commands(vec![Command {
                binary_path: "/usr/bin/python3".into(),
                args: vec!["source".into()],
            }]),
        },
    );
    return languages;
}

pub fn compile_source<R>(
    isolate_executable_path: &PathBuf,
    isolate_box: &IsolateBox,
    language: &LanguageParams,
    reader: &mut R,
) -> Result<Vec<isolate::RunStats<File>>, CommandError>
where
    R: Read,
{
    let mut source_file =
        File::create(isolate_box.path.join("source")).map_err(CommandError::CopyIo)?;
    io::copy(reader, &mut source_file).map_err(CommandError::CopyIo)?;
    source_file.sync_data().map_err(CommandError::CopyIo)?;

    if let Compile::Commands(commands) = &language.compile {
        let mut result = Vec::<isolate::RunStats<File>>::new();
        for command in commands {
            let stats = isolate::compile(
                isolate_executable_path,
                isolate_box,
                CompileParams {
                    memory_limit_mib: 1_000,
                    time_limit_ms: 50_000,
                    command,
                },
            )?;
            if match stats.exit_code {
                Some(c) => c != 0,
                None => true,
            } {
                result.push(stats);
                return Ok(result);
            }
            result.push(stats);
        }
        return Ok(result);
    }

    Ok(vec![])
}

pub struct ExecuteParams {
    pub memory_limit_mib: i32,
    pub time_limit_ms: i32,
}

pub fn run(
    isolate_executable_path: &PathBuf,
    isolate_box: &IsolateBox,
    language: &LanguageParams,
    execute_params: &ExecuteParams,
) -> Result<isolate::RunStats<File>, CommandError> {
    match language.run {
        Run::RunExe => isolate::execute(
            &isolate_executable_path,
            &isolate_box,
            &isolate::ExecuteParams {
                memory_limit_mib: execute_params.memory_limit_mib,
                time_limit_ms: execute_params.time_limit_ms,
                stdin: "./data/stdin".into(),
            },
        ),
        Run::Commands(_) => Err(CommandError::IsolateCommandFailed("Not implemented".into())),
    }
}
