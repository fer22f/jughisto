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
            String::from("-static"),
            // ONLINE_JUDGE define as in Codeforces
            String::from("-DONLINE_JUDGE"),
            // Link to the math library
            String::from("-lm"),
            // Strip all symbols
            String::from("-s"),
            // Use std
            format!("-std={}", std),
            // Define language used
            String::from("-x"),
            String::from(x),
            // Level 2 optimization
            String::from("-O2"),
            // Output to exe
            String::from("-o"),
            String::from("exe"),
            // Input from source
            String::from("source"),
        ];
    }

    let mut languages = HashMap::new();
    languages.insert(
        String::from("GCC C99"),
        LanguageParams {
            compile: Compile::Commands(vec![Command {
                binary_path: PathBuf::from("/usr/bin/gcc"),
                args: build_gcc_args("c", "c99"),
            }]),
            run: Run::RunExe,
        },
    );
    languages.insert(
        String::from("GCC C11"),
        LanguageParams {
            compile: Compile::Commands(vec![Command {
                binary_path: PathBuf::from("/usr/bin/gcc"),
                args: build_gcc_args("c", "c11"),
            }]),
            run: Run::RunExe,
        },
    );
    languages.insert(
        String::from("GCC C14"),
        LanguageParams {
            compile: Compile::Commands(vec![Command {
                binary_path: PathBuf::from("/usr/bin/gcc"),
                args: build_gcc_args("c", "c14"),
            }]),
            run: Run::RunExe,
        },
    );
    languages.insert(
        String::from("GCC C++11"),
        LanguageParams {
            compile: Compile::Commands(vec![Command {
                binary_path: PathBuf::from("/usr/bin/g++"),
                args: build_gcc_args("c++", "c++11"),
            }]),
            run: Run::RunExe,
        },
    );
    languages.insert(
        String::from("GCC C++14"),
        LanguageParams {
            compile: Compile::Commands(vec![Command {
                binary_path: PathBuf::from("/usr/bin/g++"),
                args: build_gcc_args("c++", "c++14"),
            }]),
            run: Run::RunExe,
        },
    );
    languages.insert(
        String::from("GCC C++17"),
        LanguageParams {
            compile: Compile::Commands(vec![Command {
                binary_path: PathBuf::from("/usr/bin/g++"),
                args: build_gcc_args("c++", "c++17"),
            }]),
            run: Run::RunExe,
        },
    );
    languages.insert(
        String::from("Java"),
        LanguageParams {
            compile: Compile::Commands(vec![Command {
                binary_path: PathBuf::from("/usr/bin/javac"),
                args: vec![
                    String::from("-cp"),
                    String::from("\".;*\""),
                    String::from("source"),
                ],
            }]),
            run: Run::Commands(vec![Command {
                binary_path: PathBuf::from("/usr/bin/java"),
                args: vec![
                    String::from("-Xmx512M"),
                    String::from("-Xss64M"),
                    String::from("-DONLINE_JUDGE=true"),
                    String::from("-Duser.language=en"),
                    String::from("-Duser.region=US"),
                    String::from("-Duser.variant=US"),
                    String::from("-jar"),
                    String::from("exe"),
                ],
            }]),
        },
    );
    languages.insert(
        String::from("Python 3"),
        LanguageParams {
            compile: Compile::NoCompile,
            run: Run::Commands(vec![Command {
                binary_path: PathBuf::from("/usr/bin/python3"),
                args: vec![String::from("source")],
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
                    memory_limit_mib: 1000,
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
    pub memory_limit_mib: u32,
    pub time_limit_ms: u32,
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
                stdin: PathBuf::from("./data/stdin"),
            },
        ),
        Run::Commands(_) => Err(CommandError::IsolateCommandFailed(String::from(
            "Not implemented",
        ))),
    }
}
