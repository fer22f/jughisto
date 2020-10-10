use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use serde::Serialize;

use crate::schema::problem as problem_column;
use crate::schema::problem;
use crate::schema::problem::dsl::problem as problem_table;

#[derive(Queryable, Serialize)]
pub struct Problem {
    pub id: i32,
    pub label: String,
    pub contest_id: i32,
    pub name: String,
}

pub fn get_problems(connection: &SqliteConnection) -> QueryResult<Vec<Problem>> {
    problem_table.load::<Problem>(connection)
}
