use crate::isolate::CommandTuple;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

fn no_transform(source_text: String, _source_name: String) -> String {
    return source_text;
}

#[derive(Clone)]
pub enum Compile {
    NoCompile,
    Command(
        &'static (dyn Fn(String, String) -> String + Sync),
        CommandTuple,
        String,
    ),
}

#[derive(Clone)]
pub struct LanguageParams {
    pub order: i32,
    pub name: String,
    pub suffix: String,
    pub compile: Compile,
    pub run: CommandTuple,
    pub process_limit: i32,
}

use lazy_static::lazy_static;
use regex::Captures;
use regex::Regex;
use std::str;

pub fn get_supported_languages() -> HashMap<String, LanguageParams> {
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
            static ref VERSION_REGEX: Regex = Regex::new(r"(?m)\d+\.\d+\.\d+").unwrap();
        }
        let version = VERSION_REGEX.find(stdout).unwrap().as_str();

        LanguageParams {
            order,
            suffix: ".cpp".into(),
            name: name.replace("{}", version),
            compile: Compile::Command(&no_transform, CommandTuple {
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
                    "{}".into(),
                    // Input from source
                    "{.}".into(),
                ],
            }, "{}".into()),
            run: CommandTuple {
                binary_path: "{}".into(),
                args: vec![]
            },
            process_limit: 1,
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
            suffix: ".pas".into(),
            compile: Compile::Command(&no_transform, CommandTuple {
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
                    "{.}".into(),
                    "-o{}".into(),
                ],
            }, "{}".into()),
            run: CommandTuple {
                binary_path: "{}".into(),
                args: vec![]
            },
            process_limit: 1,
        },
    );
    languages.insert(
        "java.8".into(),
        LanguageParams {
            order: 7,
            name: "Java 8".into(),
            suffix: ".java".into(),
            compile: Compile::Command(
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
                        "{.}".into(),
                    ],
                },
                "{}.class".into()
            ),
            run: CommandTuple {
                binary_path: "/usr/bin/java".into(),
                args: vec![
                    "-Xmx512m".into(),
                    "-Xss64m".into(),
                    "-DONLINE_JUDGE=true".into(),
                    "-Duser.language=en".into(),
                    "-Duser.region=US".into(),
                    "-Duser.variant=US".into(),
                    "{}".into(),
                ],
            },
            process_limit: 19,
        },
    );
    languages.insert(
        "python.3".into(),
        LanguageParams {
            order: 8,
            name: "Python 3".into(),
            suffix: ".py".into(),
            compile: Compile::NoCompile,
            run: CommandTuple {
                binary_path: "/usr/bin/python3".into(),
                args: vec!["{.}".into()],
            },
            process_limit: 1,
        },
    );
    languages
}
