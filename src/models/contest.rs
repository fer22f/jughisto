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

#[derive(Insertable)]
#[table_name = "contest"]
pub struct NewContest {
    pub name: String,
    pub start_instant: Option<NaiveDateTime>,
    pub end_instant: Option<NaiveDateTime>,
    pub creation_user_id: i32,
    pub creation_instant: NaiveDateTime,
}

pub fn insert_contest(
    connection: &PgConnection,
    new_contest: NewContest,
) -> QueryResult<Contest> {
    insert_into(contest::table)
        .values(new_contest)
        .execute(connection)?;
    contest::table.order(contest::id.desc()).first(connection)
}

pub fn get_contests(connection: &PgConnection) -> QueryResult<Vec<Contest>> {
    contest::table.load(connection)
}

pub fn get_contest_by_id(connection: &PgConnection, id: i32) -> QueryResult<Contest> {
    contest::table.filter(contest::id.eq(id))
        .first(connection)
}

#[derive(Insertable)]
#[table_name = "contest_problems"]
pub struct NewContestProblems {
    pub label: String,
    pub contest_id: i32,
    pub problem_id: String,
}

pub fn relate_problem(
    connection: &PgConnection,
    new_contest_problems: NewContestProblems,
) -> QueryResult<()> {
    insert_into(contest_problems::table)
        .values(new_contest_problems)
        .execute(connection)?;
    Ok(())
}
