use chrono::prelude::*;
use diesel::insert_into;
use diesel::prelude::*;

use crate::schema::contest;
use crate::schema::contest_problems;

#[derive(Queryable)]
pub struct Contest {
    pub id: i32,
    pub name: String,
    pub start_instant: Option<NaiveDateTime>,
    pub end_instant: Option<NaiveDateTime>,
    pub creation_user_id: i32,
    pub creation_instant: NaiveDateTime,
}

#[table_name = "contest"]
#[derive(Insertable)]
pub struct NewContest {
    pub name: String,
    pub start_instant: Option<NaiveDateTime>,
    pub end_instant: Option<NaiveDateTime>,
    pub creation_user_id: i32,
    pub creation_instant: NaiveDateTime,
}

pub fn insert_contest(
    connection: &SqliteConnection,
    new_contest: NewContest,
) -> QueryResult<Contest> {
    insert_into(contest::table)
        .values(new_contest)
        .execute(connection)?;
    contest::table.order(contest::id.desc()).first(connection)
}

pub fn get_contests(connection: &SqliteConnection) -> QueryResult<Vec<Contest>> {
    contest::table.load(connection)
}

#[table_name = "contest_problems"]
#[derive(Insertable)]
pub struct NewContestProblems {
    pub label: String,
    pub contest_id: i32,
    pub problem_id: String,
}

pub fn relate_problem(
    connection: &SqliteConnection,
    new_contest_problems: NewContestProblems,
) -> QueryResult<()> {
    insert_into(contest_problems::table)
        .values(new_contest_problems)
        .execute(connection)?;
    Ok(())
}
