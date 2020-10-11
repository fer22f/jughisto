CREATE TABLE contest_problems (
  id integer primary key autoincrement not null,
  label text not null,
  contest_id integer references contest(id) not null,
  problem_id text references problem(id) not null
)
