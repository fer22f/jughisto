use chrono::prelude::*;
use diesel::insert_into;
use diesel::prelude::*;
use diesel::pg::PgConnection;
use serde::Serialize;

use crate::schema::contest_problems;
use crate::schema::problem;

#[derive(Queryable)]
pub struct Problem {
    pub id: String,
    pub name: String,
    pub memory_limit_bytes: i32,
    pub time_limit_ms: i32,
    pub checker_path: String,
    pub checker_language: String,
    pub validator_path: String,
    pub validator_language: String,
    pub main_solution_path: String,
    pub main_solution_language: String,
    pub test_count: i32,
    pub test_pattern: String,
    pub status: String,
    pub creation_user_id: i32,
    pub creation_instant: NaiveDateTime,
}

#[derive(Insertable)]
#[table_name = "problem"]
pub struct NewProblem {
    pub id: String,
    pub name: String,
    pub memory_limit_bytes: i32,
    pub time_limit_ms: i32,
    pub checker_path: String,
    pub checker_language: String,
    pub validator_path: String,
    pub validator_language: String,
    pub main_solution_path: String,
    pub main_solution_language: String,
    pub test_count: i32,
    pub test_pattern: String,
    pub status: String,
    pub creation_user_id: i32,
    pub creation_instant: NaiveDateTime,
}

#[derive(Queryable, Serialize)]
pub struct ProblemByContest {
    pub id: i32,
    pub name: String,
    pub label: String,
}

pub fn get_problems_by_contest_id(
    connection: &PgConnection,
    contest_id: i32,
) -> QueryResult<Vec<ProblemByContest>> {
    problem::table
        .inner_join(contest_problems::table)
        .filter(contest_problems::contest_id.eq(contest_id))
        .select((contest_problems::id, problem::name, contest_problems::label))
        .order(contest_problems::label)
        .load(connection)
}

use diesel::sql_types;

#[derive(QueryableByName)]
pub struct ProblemByContestWithScore {
    #[sql_type = "sql_types::Nullable<sql_types::Timestamp>"]
    pub first_ac_submission_instant: Option<NaiveDateTime>,
    #[sql_type = "sql_types::Integer"]
    pub failed_submissions: i32,
    #[sql_type = "sql_types::Integer"]
    pub id: i32,
    #[sql_type = "sql_types::Text"]
    pub name: String,
    #[sql_type = "sql_types::Text"]
    pub label: String,
    #[sql_type = "sql_types::Integer"]
    pub memory_limit_bytes: i32,
    #[sql_type = "sql_types::Integer"]
    pub time_limit_ms: i32,
}

pub fn get_problems_by_contest_id_with_score(
    connection: &PgConnection,
    contest_id: i32
) -> QueryResult<Vec<ProblemByContestWithScore>> {
    diesel::sql_query("
        with first_ac as (
            select
                min(submission_instant) as first_ac_submission_instant,
                contest_problem_id
            from submission
            where submission.verdict = 'AC'
            group by submission.contest_problem_id
        )
        select
            first_ac.first_ac_submission_instant, cast((
                select count(*) from submission
                where submission.contest_problem_id = contest_problems.id
                and (
                    first_ac.first_ac_submission_instant is null or
                    submission.submission_instant < first_ac.first_ac_submission_instant
                )
            ) as int) as failed_submissions,
            contest_problems.id,
            problem.name,
            contest_problems.label,
            problem.memory_limit_bytes,
            problem.time_limit_ms
        from contest_problems
        inner join problem on problem.id = contest_problems.problem_id
        left join first_ac on first_ac.contest_problem_id = contest_problems.id
        where contest_id = $1
        order by contest_problems.label
    ")
    .bind::<sql_types::Integer, _>(contest_id)
    .load(connection)
}

pub fn get_problem_by_contest_id_label(
    connection: &PgConnection,
    contest_id: i32,
    problem_label: &str,
) -> QueryResult<ProblemByContest> {
    problem::table
        .inner_join(contest_problems::table)
        .filter(contest_problems::contest_id.eq(contest_id))
        .filter(contest_problems::label.eq(problem_label))
        .select((contest_problems::id, problem::name, contest_problems::label))
        .order(contest_problems::label)
        .first(connection)
}

#[derive(Queryable)]
pub struct ProblemByContestMetadata {
    pub id: String,
    pub memory_limit_bytes: i32,
    pub time_limit_ms: i32,
    pub checker_path: String,
    pub checker_language: String,
    pub validator_path: String,
    pub validator_language: String,
    pub main_solution_path: String,
    pub main_solution_language: String,
    pub test_count: i32,
    pub test_pattern: String,
    pub status: String,
}

pub fn get_problem_by_contest_id_metadata(
    connection: &PgConnection,
    contest_problem_id: i32,
) -> QueryResult<ProblemByContestMetadata> {
    problem::table
        .inner_join(contest_problems::table)
        .filter(contest_problems::id.eq(contest_problem_id))
        .select((
            problem::id,
            problem::memory_limit_bytes,
            problem::time_limit_ms,
            problem::checker_path,
            problem::checker_language,
            problem::validator_path,
            problem::validator_language,
            problem::main_solution_path,
            problem::main_solution_language,
            problem::test_count,
            problem::test_pattern,
            problem::status,
        ))
        .first(connection)
}

pub fn upsert_problem(
    connection: &PgConnection,
    new_problem: NewProblem,
) -> QueryResult<Problem> {
    match problem::table
        .filter(problem::id.eq(&new_problem.id))
        .first::<Problem>(connection)
        .optional()?
    {
        Some(p) => Ok(p),
        None => {
            insert_into(problem::table)
                .values(&new_problem)
                .execute(connection)?;
            problem::table
                .filter(problem::id.eq(&new_problem.id))
                .first::<Problem>(connection)
        }
    }
}
