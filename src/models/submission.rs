use crate::schema::submission as submission_column;
use crate::schema::submission;
use crate::schema::submission::dsl::submission as submission_table;
use chrono::prelude::*;
use diesel::prelude::*;

#[derive(Queryable)]
pub struct Submission {
    pub uuid: String,
    pub verdict: Option<String>,
    pub source_text: String,
    pub language: String,
    pub submission_instant: NaiveDateTime,
    pub judge_start_instant: Option<NaiveDateTime>,
    pub judge_end_instant: Option<NaiveDateTime>,
    pub memory_kib: Option<i32>,
    pub time_ms: Option<i32>,
    pub time_wall_ms: Option<i32>,
    pub compilation_stderr: Option<String>,
    pub problem_id: i32,
    pub user_id: i32,
}

#[table_name = "submission"]
#[derive(Insertable)]
pub struct NewSubmission {
    pub uuid: String,
    pub source_text: String,
    pub language: String,
    pub submission_instant: NaiveDateTime,
}

pub fn insert_submission(
    connection: &SqliteConnection,
    new_submission: NewSubmission,
) -> QueryResult<()> {
    diesel::insert_into(submission_table)
        .values(new_submission)
        .execute(connection)?;
    Ok(())
}

pub struct SubmissionCompletion {
    pub uuid: String,
    pub verdict: String,
    pub judge_start_instant: NaiveDateTime,
    pub judge_end_instant: NaiveDateTime,
    pub memory_kib: Option<i32>,
    pub time_ms: Option<i32>,
    pub time_wall_ms: Option<i32>,
    pub compilation_stderr: Option<String>,
}

pub fn complete_submission(
    connection: &SqliteConnection,
    submission: SubmissionCompletion,
) -> QueryResult<()> {
    diesel::update(submission_table.filter(submission_column::uuid.eq(submission.uuid)))
        .set((
            submission_column::verdict.eq(submission.verdict),
            submission_column::judge_start_instant.eq(submission.judge_start_instant),
            submission_column::judge_end_instant.eq(submission.judge_end_instant),
            submission_column::memory_kib.eq(submission.memory_kib),
            submission_column::time_ms.eq(submission.time_ms),
            submission_column::time_wall_ms.eq(submission.time_wall_ms),
            submission_column::compilation_stderr.eq(submission.compilation_stderr),
        ))
        .execute(connection)?;
    Ok(())
}

pub fn get_submissions(connection: &SqliteConnection) -> QueryResult<Vec<Submission>> {
    submission_table
        .order_by(submission_column::submission_instant.desc())
        .load::<Submission>(connection)
}
