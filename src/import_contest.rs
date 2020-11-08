use lazy_static::lazy_static;
use regex::Regex;
use std::io::{Read, Seek};
use zip::ZipArchive;

mod error {
    use quick_xml::de::DeError;
    use std::io;
    use thiserror::Error;
    use zip::result::ZipError;

    #[derive(Error, Debug)]
    pub enum ImportContestError {
        #[error(transparent)]
        Zip(#[from] ZipError),
        #[error(transparent)]
        XmlDecode(#[from] DeError),
        #[error(transparent)]
        Io(#[from] io::Error),
    }
}

pub use error::ImportContestError;

mod xml {
    use prelude::*;
    use quick_xml::de::from_str;

    pub fn get_from_zip<T: for<'de> Deserialize<'de>, R: Read + Seek>(
        zip: &mut ZipArchive<R>,
        path: &str,
    ) -> Result<T, ImportContestError> {
        let xml = super::read_string_from_zip_by_name(&mut *zip, path)?;
        let deserialized: T = from_str(&xml)?;
        Ok(deserialized)
    }

    pub mod prelude {
        pub use super::super::ImportContestError;
        pub use serde::Deserialize;
        pub use std::io::{Read, Seek};
        pub use zip::ZipArchive;
    }

    pub mod contest {
        use super::prelude::*;

        #[derive(Deserialize, Debug)]
        pub struct Contest {
            pub url: String,
            pub names: Names,
            pub problems: Problems,
        }

        #[derive(Deserialize, Debug)]
        pub struct Names {
            pub name: Vec<Name>,
        }

        #[derive(Deserialize, Debug)]
        pub struct Name {
            pub language: String,
            pub value: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct Problems {
            pub problem: Vec<Problem>,
        }

        #[derive(Deserialize, Debug)]
        pub struct Problem {
            pub index: String,
            pub url: String,
        }

        pub fn get_from_zip<R: Read + Seek>(
            zip: &mut ZipArchive<R>,
        ) -> Result<Contest, ImportContestError> {
            super::get_from_zip::<Contest, R>(&mut *zip, "contest.xml")
        }
    }

    pub mod problem {
        use super::prelude::*;

        #[derive(Deserialize, Debug)]
        pub struct Problem {
            pub url: String,
            pub revision: String,
            #[serde(rename = "short-name")]
            pub short_name: String,
            pub names: Names,
            pub statements: Statements,
            pub judging: Judging,
            pub files: Files,
            pub assets: Assets,
            pub properties: Properties,
            pub stresses: Stresses,
            pub tags: Tags,
        }

        #[derive(Deserialize, Debug)]
        pub struct Names {
            pub name: Vec<Name>,
        }

        #[derive(Deserialize, Debug)]
        pub struct Name {
            pub language: String,
            pub value: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct Statements {
            pub statement: Vec<Statement>,
        }

        #[derive(Deserialize, Debug)]
        pub struct Statement {
            pub charset: Option<String>,
            pub language: String,
            pub mathjax: Option<bool>,
            pub path: String,
            pub r#type: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct Judging {
            #[serde(rename = "cpu-name")]
            pub cpu_name: String,
            #[serde(rename = "cpu-speed")]
            pub cpu_speed: String,
            #[serde(rename = "input-file")]
            pub input_file: String,
            #[serde(rename = "output-file")]
            pub output_file: String,
            pub testset: Vec<Testset>,
        }

        #[derive(Deserialize, Debug)]
        pub struct Testset {
            pub name: String,
            #[serde(rename = "time-limit")]
            pub time_limit: TimeLimit,
            #[serde(rename = "memory-limit")]
            pub memory_limit: MemoryLimit,
            #[serde(rename = "test-count")]
            pub test_count: TestCount,
            #[serde(rename = "input-path-pattern")]
            pub input_path_pattern: InputPathPattern,
            #[serde(rename = "answer-path-pattern")]
            pub answer_path_pattern: AnswerPathPattern,
            pub tests: Tests,
        }

        #[derive(Deserialize, Debug)]
        pub struct TimeLimit {
            #[serde(rename = "$value")]
            pub value: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct MemoryLimit {
            #[serde(rename = "$value")]
            pub value: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct TestCount {
            #[serde(rename = "$value")]
            pub value: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct InputPathPattern {
            #[serde(rename = "$value")]
            pub value: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct AnswerPathPattern {
            #[serde(rename = "$value")]
            pub value: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct Tests {
            pub test: Vec<Test>,
        }

        #[derive(Deserialize, Debug)]
        pub struct Test {
            pub method: Option<String>,
            pub sample: Option<bool>,
            pub description: Option<String>,
            pub cmd: Option<String>,
        }

        #[derive(Deserialize, Debug)]
        pub struct Files {
            pub resources: Resources,
            pub executables: Executables,
        }

        #[derive(Deserialize, Debug)]
        pub struct Resources {
            pub file: Vec<File>,
        }

        #[derive(Deserialize, Debug)]
        pub struct File {
            pub path: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct Executables {
            pub executable: Vec<Executable>,
        }

        #[derive(Deserialize, Debug)]
        pub struct Executable {
            pub source: Source,
            pub binary: Binary,
        }

        #[derive(Deserialize, Debug)]
        pub struct Source {
            pub path: String,
            pub r#type: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct Binary {
            pub path: String,
            pub r#type: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct Assets {
            pub checker: Checker,
            pub validators: Validators,
            pub solutions: Solutions,
        }

        #[derive(Deserialize, Debug)]
        pub struct Checker {
            pub name: String,
            pub r#type: String,
            pub source: Source,
            pub binary: Binary,
            pub copy: Copy,
            pub testset: CheckerTestset,
        }

        #[derive(Deserialize, Debug)]
        pub struct CheckerTestset {
            #[serde(rename = "test-count")]
            pub test_count: TestCount,
            #[serde(rename = "input-path-pattern")]
            pub input_path_pattern: InputPathPattern,
            #[serde(rename = "answer-path-pattern")]
            pub answer_path_pattern: AnswerPathPattern,
            pub tests: VerdictTests,
        }

        #[derive(Deserialize, Debug)]
        pub struct Copy {
            pub path: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct Validators {
            pub validator: Vec<Validator>,
        }

        #[derive(Deserialize, Debug)]
        pub struct Validator {
            pub source: Source,
            pub binary: Binary,
            pub testset: ValidatorTestset,
        }

        #[derive(Deserialize, Debug)]
        pub struct ValidatorTestset {
            #[serde(rename = "test-count")]
            pub test_count: TestCount,
            #[serde(rename = "input-path-pattern")]
            pub input_path_pattern: InputPathPattern,
            pub tests: VerdictTests,
        }

        #[derive(Deserialize, Debug)]
        pub struct VerdictTests {
            pub test: Option<Vec<VerdictTest>>,
        }

        #[derive(Deserialize, Debug)]
        pub struct VerdictTest {
            pub verdict: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct Solutions {
            pub solution: Vec<Solution>,
        }

        #[derive(Deserialize, Debug)]
        pub struct Solution {
            pub tag: String,
            pub source: Source,
            pub binary: Binary,
        }

        #[derive(Deserialize, Debug)]
        pub struct Properties {
            pub property: Option<Vec<Property>>,
        }

        #[derive(Deserialize, Debug)]
        pub struct Property {
            pub name: String,
            pub value: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct Stresses {
            #[serde(rename = "stress-count")]
            pub stress_count: String,
            #[serde(rename = "stress-path-pattern")]
            pub stress_path_pattern: String,
            pub list: StressList,
        }

        #[derive(Deserialize, Debug)]
        pub struct StressList {}

        #[derive(Deserialize, Debug)]
        pub struct Tags {
            pub tag: Option<Vec<Tag>>,
        }

        #[derive(Deserialize, Debug)]
        pub struct Tag {
            pub value: String,
        }

        pub fn get_from_zip<R: Read + Seek>(
            zip: &mut ZipArchive<R>,
            name: &str,
        ) -> Result<Problem, ImportContestError> {
            super::get_from_zip::<Problem, R>(&mut *zip, name)
        }
    }
}

fn read_string_from_zip_by_name<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
) -> Result<String, ImportContestError> {
    let mut content = String::new();
    zip.by_name(name)?.read_to_string(&mut content)?;
    Ok(content)
}

pub use xml::contest::Contest;
pub use xml::problem::Problem;

pub fn import_file<R: Read + Seek>(
    reader: R,
) -> Result<(Contest, Vec<(String, Problem)>, ZipArchive<R>, String), ImportContestError> {
    let mut report = String::new();

    let mut zip = ZipArchive::new(reader)?;
    let contest = xml::contest::get_from_zip(&mut zip)?;

    report.push_str(&format!("{:#?}", contest));
    report.push_str("\n---\n");

    let mut problems: Vec<(String, Problem)> = Vec::new();

    lazy_static! {
        static ref PROBLEM_XML_PATH_REGEX: Regex =
            Regex::new(r"^(problems/.*)/problem.xml$").unwrap();
    }

    for name in zip
        .file_names()
        .filter(|name| PROBLEM_XML_PATH_REGEX.is_match(name))
        .map(|s| s.into())
        .collect::<Vec<String>>()
    {
        report.push_str(&name);
        let problem = xml::problem::get_from_zip(&mut zip, &name)?;
        report.push_str("\n---\n");
        report.push_str(&format!("{:#?}", problem));
        report.push_str("\n---\n");
        report.push_str(&read_string_from_zip_by_name(&mut zip, &name)?);
        report.push_str("---\n");
        report.push_str("\n");

        problems.push((
            PROBLEM_XML_PATH_REGEX
                .captures(&name)
                .unwrap()
                .get(1)
                .unwrap()
                .as_str()
                .into(),
            problem,
        ));
    }

    Ok((contest, problems, zip, report))
}

use regex::Captures;
use std::path::PathBuf;

pub fn format_width(pattern_path: &String, i: usize) -> PathBuf {
    lazy_static! {
        static ref WIDTH_REGEX: Regex = Regex::new(
            r"%0(\d)+d"
        ).unwrap();
    }
    PathBuf::from(String::from(
        WIDTH_REGEX.replace(pattern_path, |caps: &Captures| {
            return format!("{:0width$}", i, width = caps[1].parse().unwrap())
        })))
}
