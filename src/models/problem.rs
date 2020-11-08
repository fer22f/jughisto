use chrono::prelude::*;
use diesel::insert_into;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
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
    connection: &SqliteConnection,
    contest_id: i32,
) -> QueryResult<Vec<ProblemByContest>> {
    problem::table
        .inner_join(contest_problems::table)
        .filter(contest_problems::contest_id.eq(contest_id))
        .select((contest_problems::id, problem::name, contest_problems::label))
        .order(contest_problems::label)
        .load(connection)
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
    connection: &SqliteConnection,
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
    connection: &SqliteConnection,
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
