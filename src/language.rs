use crate::isolate;
use crate::isolate::{CommandError, CompileParams, IsolateBox};
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::Read;
use std::path::PathBuf;

#[derive(Clone)]
pub struct Command {
    pub binary_path: PathBuf,
    pub args: Vec<String>,
}

#[derive(Clone)]
pub enum Run {
    RunExe,
    Commands(Vec<Command>),
}

#[derive(Clone)]
pub enum Compile {
    NoCompile,
    Commands(Vec<Command>),
}

#[derive(Clone)]
pub struct LanguageParams {
    pub order: i32,
    pub name: String,
    compile: Compile,
    run: Run,
}

use lazy_static::lazy_static;
use regex::Regex;
use std::process;
use std::str;
use std::sync::Arc;

pub fn get_supported_languages() -> Arc<HashMap<String, LanguageParams>> {
    fn build_gcc_params<'a>(
        order: i32,
        name: &'a str,
        binary_path: PathBuf,
        x: &'a str,
        std: &'a str,
    ) -> LanguageParams {
        let output = process::Command::new(&binary_path)
            .arg("--version")
            .output()
            .unwrap();
        let stdout = str::from_utf8(&output.stdout).unwrap();
        lazy_static! {
            static ref VERSION_REGEX: Regex = Regex::new(r"(?m)\d+\.\d+\.\d+$").unwrap();
        }
        let version = VERSION_REGEX.find(stdout).unwrap().as_str();

        LanguageParams {
            order,
            name: name.replace("{}", version),
            compile: Compile::Commands(vec![Command {
                binary_path,
                args: vec![
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
                ],
            }]),
            run: Run::RunExe,
        }
    }

    let mut languages = HashMap::new();
    languages.insert(
        "cpp.g++11".into(),
        build_gcc_params(0, "GNU G++11 {}", "/usr/bin/g++".into(), "c++", "c++11"),
    );
    languages.insert(
        "cpp.g++14".into(),
        build_gcc_params(1, "GNU G++14 {}", "/usr/bin/g++".into(), "c++", "c++14"),
    );
    languages.insert(
        "cpp.g++17".into(),
        build_gcc_params(2, "GNU G++17 {}", "/usr/bin/g++".into(), "c++", "c++17"),
    );
    languages.insert(
        "c99.gcc".into(),
        build_gcc_params(3, "GNU GCC C99 {}", "/usr/bin/gcc".into(), "c", "c99"),
    );
    languages.insert(
        "c11.gcc".into(),
        build_gcc_params(4, "GNU GCC C11 {}", "/usr/bin/gcc".into(), "c", "c11"),
    );
    languages.insert(
        "c.gcc".into(),
        build_gcc_params(5, "GNU GCC C18 {}", "/usr/bin/gcc".into(), "c", "c18"),
    );
    languages.insert(
        "pas.fpc".into(),
        LanguageParams {
            order: 6,
            name: "Free Pascal".into(),
            compile: Compile::Commands(vec![Command {
                binary_path: "/usr/bin/fpc".into(),
                args: vec![
                    // Level 2 optimizations
                    "-O2".into(),
                    // Strip the symbols from the executable
                    "-Xs".into(),
                    // Link statically
                    "-XS".into(),
                    // Allow label and goto, support C++ style inline and C-style operators
                    "-Sgic".into(),
                    // Show warnings and notes
                    "-vwn".into(),
                    // Define the symbol
                    "-dONLINE_JUDGE".into(),
                    // Set stack size
                    "-Cs67107839".into(),
                    // Language mode: Delphi compatibility
                    "-Mdelphi".into(),
                    "source".into(),
                    "-oexe".into(),
                ],
            }]),
            run: Run::RunExe,
        },
    );
    languages.insert(
        "java8".into(),
        LanguageParams {
            order: 7,
            name: "Java 8".into(),
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
        "python.3".into(),
        LanguageParams {
            order: 8,
            name: "Python 3".into(),
            compile: Compile::NoCompile,
            run: Run::Commands(vec![Command {
                binary_path: "/usr/bin/python3".into(),
                args: vec!["source".into()],
            }]),
        },
    );
    Arc::new(languages)
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
