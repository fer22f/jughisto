use diesel::insert_into;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use serde::Serialize;

use crate::schema::problem;
use crate::schema::contest_problems;

#[derive(Queryable, Serialize)]
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
    pub status: String,
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
    pub status: String,
}

pub fn get_problems(connection: &SqliteConnection) -> QueryResult<Vec<Problem>> {
    problem::table.load(connection)
}

#[derive(Queryable, Serialize)]
pub struct ProblemByContest {
    pub id: i32,
    pub name: String,
    pub label: String,
}

pub fn get_problems_by_contest_id(connection: &SqliteConnection, contest_id: i32) -> QueryResult<Vec<ProblemByContest>> {
    problem::table
        .inner_join(contest_problems::table)
        .filter(contest_problems::contest_id.eq(contest_id))
        .select((
            contest_problems::id,
            problem::name,
            contest_problems::label,
        ))
        .order(contest_problems::label)
        .load(connection)
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
