use crate::isolate;
use crate::isolate::{CommandError, CommandTuple, CompileParams, IsolateBox};
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone)]
pub enum Run {
    RunExe,
    Command(CommandTuple),
}

#[derive(Clone)]
pub enum Compile {
    NoCompile,
    Command(CommandTuple),
    TransformAndCommand(
        &'static (dyn Fn(String, String) -> String + Sync),
        CommandTuple,
    ),
}

#[derive(Clone)]
pub struct LanguageParams {
    pub order: i32,
    pub name: String,
    suffix: String,
    compile: Compile,
    run: Run,
}

use lazy_static::lazy_static;
use regex::Captures;
use regex::Regex;
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
        let output = Command::new(&binary_path)
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
            suffix: "".into(),
            name: name.replace("{}", version),
            compile: Compile::Command(CommandTuple {
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
            }),
            run: Run::RunExe,
        }
    }

    let mut languages = HashMap::new();
    languages.insert(
        "cpp.17.g++".into(),
        build_gcc_params(2, "GNU G++17 {}", "/usr/bin/g++".into(), "c++", "c++17"),
    );
    languages.insert(
        "c.18.gcc".into(),
        build_gcc_params(5, "GNU GCC C18 {}", "/usr/bin/gcc".into(), "c", "c18"),
    );
    languages.insert(
        "pascal.fpc".into(),
        LanguageParams {
            order: 6,
            name: "Free Pascal".into(),
            suffix: "".into(),
            compile: Compile::Command(CommandTuple {
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
            }),
            run: Run::RunExe,
        },
    );
    languages.insert(
        "java.8".into(),
        LanguageParams {
            order: 7,
            name: "Java 8".into(),
            suffix: ".java".into(),
            compile: Compile::TransformAndCommand(
                &|source_text, source_name| {
                    lazy_static! {
                        static ref PUBLIC_CLASS_REGEX: Regex =
                            Regex::new(r"(?i)([^{}]*public\s+class\s+)(\w+)").unwrap();
                    }
                    PUBLIC_CLASS_REGEX
                        .replacen(&source_text, 1, |caps: &Captures| {
                            format!("{}{}", &caps[1], source_name)
                        })
                        .into()
                },
                CommandTuple {
                    binary_path: "/usr/lib/jvm/java-1.8-openjdk/bin/javac".into(),
                    args: vec![
                        "-cp".into(),
                        "\".;*\"".into(),
                        "-J-Xmx512m".into(),
                        "-J-XX:MaxMetaspaceSize=128m".into(),
                        "-J-XX:CompressedClassSpaceSize=64m".into(),
                        "source.java".into(),
                    ],
                },
            ),
            run: Run::Command(CommandTuple {
                binary_path: "/usr/bin/java".into(),
                args: vec![
                    "-Xmx512m".into(),
                    "-Xss64m".into(),
                    "-DONLINE_JUDGE=true".into(),
                    "-Duser.language=en".into(),
                    "-Duser.region=US".into(),
                    "-Duser.variant=US".into(),
                    "source".into(),
                ],
            }),
        },
    );
    languages.insert(
        "python.3".into(),
        LanguageParams {
            order: 8,
            name: "Python 3".into(),
            suffix: "".into(),
            compile: Compile::NoCompile,
            run: Run::Command(CommandTuple {
                binary_path: "/usr/bin/python3".into(),
                args: vec!["source".into()],
            }),
        },
    );
    Arc::new(languages)
}

use std::io::Write;

pub fn compile_source<R>(
    isolate_executable_path: &PathBuf,
    isolate_box: &IsolateBox,
    language: &LanguageParams,
    reader: &mut R,
) -> Result<Option<isolate::RunStats<File>>, CommandError>
where
    R: Read,
{
    if let Compile::NoCompile = language.compile {
        return Ok(None);
    }

    let source_name = "source";

    let mut source_file = File::create(
        isolate_box
            .path
            .join(format!("{}{}", source_name, language.suffix)),
    )
    .map_err(CommandError::CopyIo)?;

    if let Compile::TransformAndCommand(transform, _) = language.compile {
        let mut string = String::new();
        reader
            .read_to_string(&mut string)
            .map_err(CommandError::CopyIo)?;
        source_file
            .write(transform(string, source_name.into()).as_bytes())
            .map_err(CommandError::CopyIo)?;
    } else {
        io::copy(reader, &mut source_file).map_err(CommandError::CopyIo)?;
    }

    source_file.sync_data().map_err(CommandError::CopyIo)?;

    match &language.compile {
        Compile::Command(command) | Compile::TransformAndCommand(_, command) => {
            let stats = isolate::compile(
                isolate_executable_path,
                isolate_box,
                CompileParams {
                    memory_limit_mib: 1_024,
                    time_limit_ms: 50_000,
                    command: &command,
                },
            )?;
            Ok(Some(stats))
        }
        Compile::NoCompile => Ok(None),
    }
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
    match &language.run {
        Run::RunExe => Ok(isolate::execute(
            &isolate_executable_path,
            &isolate_box,
            &CommandTuple {
                binary_path: "exe".into(),
                args: vec![],
            },
            &isolate::ExecuteParams {
                memory_limit_mib: execute_params.memory_limit_mib,
                time_limit_ms: execute_params.time_limit_ms,
                stdin: "./data/stdin".into(),
            },
        )?),
        Run::Command(command) => {
            let stats = isolate::execute(
                &isolate_executable_path,
                &isolate_box,
                &command,
                &isolate::ExecuteParams {
                    memory_limit_mib: execute_params.memory_limit_mib,
                    time_limit_ms: execute_params.time_limit_ms,
                    stdin: "./data/stdin".into(),
                },
            )?;
            Ok(stats)
        }
    }
}
