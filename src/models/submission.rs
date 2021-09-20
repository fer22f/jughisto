use crate::schema::submission;
use chrono::prelude::*;
use diesel::insert_into;
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
    pub error_output: Option<String>,
    pub contest_problem_id: i32,
    pub user_id: i32,
}

#[derive(Insertable)]
#[table_name = "submission"]
pub struct NewSubmission {
    pub uuid: String,
    pub source_text: String,
    pub language: String,
    pub submission_instant: NaiveDateTime,
    pub contest_problem_id: i32,
    pub user_id: i32,
}

pub fn insert_submission(
    connection: &PgConnection,
    new_submission: NewSubmission,
) -> QueryResult<()> {
    insert_into(submission::table)
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
    pub error_output: Option<String>,
}

pub fn complete_submission(
    connection: &PgConnection,
    submission: SubmissionCompletion,
) -> QueryResult<()> {
    diesel::update(submission::table)
        .filter(submission::uuid.eq(submission.uuid))
        .set((
            submission::verdict.eq(submission.verdict),
            submission::judge_start_instant.eq(submission.judge_start_instant),
            submission::judge_end_instant.eq(submission.judge_end_instant),
            submission::memory_kib.eq(submission.memory_kib),
            submission::time_ms.eq(submission.time_ms),
            submission::time_wall_ms.eq(submission.time_wall_ms),
            submission::error_output.eq(submission.error_output),
        ))
        .execute(connection)?;
    Ok(())
}

pub fn get_submissions(connection: &PgConnection) -> QueryResult<Vec<Submission>> {
    submission::table
        .order_by(submission::submission_instant.desc())
        .load::<Submission>(connection)
}
